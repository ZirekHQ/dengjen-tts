mod utils;
pub use dengjen_core::*;

use flume::{Receiver, SendError, Sender};
use once_cell::sync::Lazy;
use rayon::prelude::*;
use rayon::{ThreadPool, ThreadPoolBuilder};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

struct ParamRange {
    min: f32,
    max: f32,
}

const RATE_RANGE: ParamRange = ParamRange { min: 0.5, max: 5.5 };
const VOLUME_RANGE: ParamRange = ParamRange { min: 0.0, max: 1.0 };
const PITCH_RANGE: ParamRange = ParamRange { min: 0.5, max: 1.5 };

pub static SYNTHESIS_THREAD_POOL: Lazy<ThreadPool> = Lazy::new(|| {
    let available = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(4);
    ThreadPoolBuilder::new()
        .num_threads(available * 4)
        .thread_name(|index| format!("dengjen_synth_{index}"))
        .build()
        .unwrap()
});

#[derive(Clone)]
pub struct AudioOutputConfig {
    pub rate: Option<u8>,
    pub volume: Option<u8>,
    pub pitch: Option<u8>,
    pub appended_silence_ms: Option<u32>,
}

impl AudioOutputConfig {
    fn apply(&self, mut audio: Audio) -> DengjenAudioResult {
        let mut raw_samples = audio.samples.take();
        if let Some(silence_ms) = self.appended_silence_ms {
            let silence = self.generate_silence(
                silence_ms as usize,
                audio.info.sample_rate,
                audio.info.num_channels,
            )?;
            raw_samples.extend(silence.into_vec());
        }
        let processed = self.apply_to_raw_samples(
            raw_samples.into(),
            audio.info.sample_rate,
            audio.info.num_channels,
        )?;
        audio.samples.merge(processed);
        Ok(audio)
    }

    fn apply_to_raw_samples(
        &self,
        samples: AudioSamples,
        sample_rate: usize,
        num_channels: usize,
    ) -> DengjenResult<AudioSamples> {
        if samples.is_empty() {
            return Ok(samples);
        }
        let input = samples.into_vec();

        // SAFETY: `stream` is created, fed, flushed, and destroyed within this block on
        // every path (including the error path), so no libsonic resource escapes it.
        unsafe {
            let stream = sonic_sys::sonicCreateStream(sample_rate as i32, num_channels as i32);

            if let Some(pct) = self.rate {
                let speed = utils::percent_to_param(pct, RATE_RANGE.min, RATE_RANGE.max);
                sonic_sys::sonicSetSpeed(stream, speed);
            }
            if let Some(pct) = self.volume {
                let volume = utils::percent_to_param(pct, VOLUME_RANGE.min, VOLUME_RANGE.max);
                sonic_sys::sonicSetVolume(stream, volume);
            }
            if let Some(pct) = self.pitch {
                let pitch = utils::percent_to_param(pct, PITCH_RANGE.min, PITCH_RANGE.max);
                sonic_sys::sonicSetPitch(stream, pitch);
            }

            sonic_sys::sonicWriteFloatToStream(stream, input.as_ptr(), input.len() as i32);
            sonic_sys::sonicFlushStream(stream);

            let available = sonic_sys::sonicSamplesAvailable(stream);
            if available <= 0 {
                sonic_sys::sonicDestroyStream(stream);
                return Err(DengjenError::OperationError(
                    "Sonic Error: failed to apply audio config. Invalid parameter value for rate, volume, or pitch".to_string(),
                ));
            }

            let mut output: Vec<f32> = Vec::with_capacity(available as usize);
            sonic_sys::sonicReadFloatFromStream(
                stream,
                output.spare_capacity_mut().as_mut_ptr().cast(),
                available,
            );
            output.set_len(available as usize);

            sonic_sys::sonicDestroyStream(stream);

            Ok(output.into())
        }
    }

    fn generate_silence(
        &self,
        time_ms: usize,
        sample_rate: usize,
        num_channels: usize,
    ) -> DengjenResult<AudioSamples> {
        let num_samples = (time_ms * sample_rate) / 1000;
        let silence = vec![0f32; num_samples];
        self.apply_to_raw_samples(silence.into(), sample_rate, num_channels)
    }
}

/// Wraps a backend model behind the higher-level synthesis entry points
/// (`synthesize_lazy`/`synthesize_parallel`/`synthesize_streamed`/`synthesize_to_file`).
pub struct DengjenSpeechSynthesizer {
    model: Arc<dyn DengjenModel + Sync + Send>,
}

impl DengjenSpeechSynthesizer {
    pub fn new(model: Arc<dyn DengjenModel + Sync + Send>) -> DengjenResult<Self> {
        Ok(Self { model })
    }

    #[inline(always)]
    pub fn clone_model(&self) -> Arc<dyn DengjenModel + Send + Sync> {
        Arc::clone(&self.model)
    }

    fn create_synthesis_task_provider(
        &self,
        text: String,
        output_config: Option<AudioOutputConfig>,
    ) -> SpeechSynthesisTaskProvider {
        let model = self.clone_model();
        SpeechSynthesisTaskProvider { model, text, output_config }
    }

    pub fn synthesize_lazy(
        &self,
        text: String,
        output_config: Option<AudioOutputConfig>,
    ) -> DengjenResult<DengjenSpeechStreamLazy> {
        let provider = self.create_synthesis_task_provider(text, output_config);
        DengjenSpeechStreamLazy::new(provider)
    }

    pub fn synthesize_parallel(
        &self,
        text: String,
        output_config: Option<AudioOutputConfig>,
    ) -> DengjenResult<DengjenSpeechStreamParallel> {
        let provider = self.create_synthesis_task_provider(text, output_config);
        DengjenSpeechStreamParallel::new(provider)
    }

    pub fn synthesize_streamed(
        &self,
        text: String,
        output_config: Option<AudioOutputConfig>,
        chunk_size: usize,
        chunk_padding: usize,
        cancel_token: CancellationToken,
    ) -> DengjenResult<RealtimeSpeechStream> {
        let output_info = self.model.audio_output_info()?;
        let provider = self.create_synthesis_task_provider(text, output_config);
        RealtimeSpeechStream::new(
            provider,
            chunk_size,
            chunk_padding,
            output_info.sample_rate,
            output_info.num_channels,
            cancel_token,
        )
    }

    pub fn synthesize_to_file(
        &self,
        filename: &Path,
        text: String,
        output_config: Option<AudioOutputConfig>,
    ) -> DengjenResult<()> {
        let mut all_samples: Vec<f32> = Vec::new();
        for result in self.synthesize_parallel(text, output_config)? {
            let audio = result?;
            all_samples.extend(audio.into_vec());
        }
        if all_samples.is_empty() {
            return Err(DengjenError::OperationError(
                "No speech data to write".to_string(),
            ));
        }

        let output_info = self.model.audio_output_info()?;
        let samples = AudioSamples::from(all_samples);
        audio_ops::write_wave_samples_to_file(
            filename,
            samples.to_i16_vec().iter(),
            output_info.sample_rate as u32,
            output_info.num_channels.try_into().unwrap(),
            output_info.sample_width.try_into().unwrap(),
        )?;
        Ok(())
    }
}

impl DengjenModel for DengjenSpeechSynthesizer {
    fn audio_output_info(&self) -> DengjenResult<AudioInfo> {
        self.model.audio_output_info()
    }
    fn phonemize_text(&self, text: &str) -> DengjenResult<Phonemes> {
        self.model.phonemize_text(text)
    }
    fn speak_batch(&self, phoneme_batches: Vec<String>) -> DengjenResult<Vec<Audio>> {
        self.model.speak_batch(phoneme_batches)
    }
    fn speak_one_sentence(&self, phonemes: String) -> DengjenAudioResult {
        self.model.speak_one_sentence(phonemes)
    }
    fn get_default_synthesis_config(&self) -> DengjenResult<SynthesisConfig> {
        self.model.get_default_synthesis_config()
    }
    fn get_fallback_synthesis_config(&self) -> DengjenResult<SynthesisConfig> {
        self.model.get_fallback_synthesis_config()
    }
    fn set_fallback_synthesis_config(&self, synthesis_config: &SynthesisConfig) -> DengjenResult<()> {
        self.model.set_fallback_synthesis_config(synthesis_config)
    }
    fn get_language(&self) -> DengjenResult<Option<String>> {
        self.model.get_language()
    }
    fn get_speakers(&self) -> DengjenResult<Option<&HashMap<i64, String>>> {
        self.model.get_speakers()
    }
    fn properties(&self) -> DengjenResult<HashMap<String, String>> {
        self.model.properties()
    }
    fn supports_streaming_output(&self) -> bool {
        self.model.supports_streaming_output()
    }
    fn stream_synthesis<'a>(
        &'a self,
        phonemes: String,
        chunk_size: usize,
        chunk_padding: usize,
        cancel_token: CancellationToken,
    ) -> DengjenResult<Box<dyn Iterator<Item = DengjenResult<AudioSamples>> + Send + Sync + 'a>> {
        self.model
            .stream_synthesis(phonemes, chunk_size, chunk_padding, cancel_token)
    }
}

/// Bundles a model handle, the input text, and an optional output-shaping
/// config so the various stream constructors don't each need their own
/// (model, text, output_config) triple.
struct SpeechSynthesisTaskProvider {
    model: Arc<dyn DengjenModel + Sync + Send>,
    text: String,
    output_config: Option<AudioOutputConfig>,
}

impl SpeechSynthesisTaskProvider {
    fn get_phonemes(&self) -> DengjenResult<Vec<String>> {
        let phonemes = self.model.phonemize_text(&self.text)?;
        Ok(phonemes.to_vec())
    }

    fn process_one_sentence(&self, phonemes: String) -> DengjenAudioResult {
        let audio = self.model.speak_one_sentence(phonemes)?;
        match &self.output_config {
            Some(config) => config.apply(audio),
            None => Ok(audio),
        }
    }

    #[allow(dead_code)]
    fn process_batches(&self, phonemes: Vec<String>) -> DengjenResult<Vec<Audio>> {
        let batch = self.model.speak_batch(phonemes)?;
        match &self.output_config {
            Some(config) => batch.into_iter().map(|audio| config.apply(audio)).collect(),
            None => Ok(batch),
        }
    }
}

/// Synthesizes sentences one at a time as the caller pulls from the iterator.
pub struct DengjenSpeechStreamLazy {
    provider: SpeechSynthesisTaskProvider,
    remaining_phonemes: std::vec::IntoIter<String>,
}

impl DengjenSpeechStreamLazy {
    fn new(provider: SpeechSynthesisTaskProvider) -> DengjenResult<Self> {
        let remaining_phonemes = provider.get_phonemes()?.into_iter();
        Ok(Self {
            provider,
            remaining_phonemes,
        })
    }
}

impl Iterator for DengjenSpeechStreamLazy {
    type Item = DengjenAudioResult;

    fn next(&mut self) -> Option<Self::Item> {
        let phonemes = self.remaining_phonemes.next()?;
        Some(self.provider.process_one_sentence(phonemes))
    }
}

/// Synthesizes every sentence up front, in parallel via rayon, then hands out
/// the precomputed results one at a time. `par_iter().map().collect()` is
/// order-preserving, so results come out in the same order as the input
/// sentences despite being computed concurrently.
#[must_use]
pub struct DengjenSpeechStreamParallel {
    results: std::vec::IntoIter<DengjenAudioResult>,
}

impl DengjenSpeechStreamParallel {
    fn new(provider: SpeechSynthesisTaskProvider) -> DengjenResult<Self> {
        let phonemes = provider.get_phonemes()?;
        let results: Vec<DengjenAudioResult> = phonemes
            .par_iter()
            .map(|sentence| provider.process_one_sentence(sentence.clone()))
            .collect();
        Ok(Self {
            results: results.into_iter(),
        })
    }
}

impl Iterator for DengjenSpeechStreamParallel {
    type Item = DengjenAudioResult;

    fn next(&mut self) -> Option<Self::Item> {
        self.results.next()
    }
}

/// Backstop for `next_chunk_size`, in whatever unit the active backend's
/// `chunk_size` parameter uses (mel frames for Piper, samples for Kokoro).
const MAX_STREAM_CHUNK_SIZE: usize = 1_000_000;

pub struct RealtimeSpeechStream {
    rx: Receiver<DengjenResult<AudioSamples>>,
    cancel_token: CancellationToken,
}

impl RealtimeSpeechStream {
    /// Ramps the chunk size additively as the stream progresses: each sentence
    /// contributes one more multiple of the *original* base chunk size, up to
    /// a cap of 4 multiples (5x base), so later sentences synthesize in fewer,
    /// larger chunks without ever compounding on a previously grown value or
    /// dropping back toward `base` between sentences (issue #28). Clamped to
    /// `MAX_STREAM_CHUNK_SIZE` as a backstop.
    fn next_chunk_size(base_chunk_size: usize, sentences_seen: usize) -> usize {
        let ramp_multiple = sentences_seen.min(4);
        base_chunk_size
            .saturating_add(base_chunk_size.saturating_mul(ramp_multiple))
            .min(MAX_STREAM_CHUNK_SIZE)
    }

    fn new(
        provider: SpeechSynthesisTaskProvider,
        chunk_size: usize,
        chunk_padding: usize,
        sample_rate: usize,
        num_channels: usize,
        cancel_token: CancellationToken,
    ) -> DengjenResult<Self> {
        let sentences = provider.get_phonemes()?;
        let (tx, rx) = flume::unbounded();
        let producer_cancel_token = cancel_token.clone();
        SYNTHESIS_THREAD_POOL.spawn(move || {
            let cancel_token = producer_cancel_token;
            for (sentences_seen, phonemes) in sentences.into_iter().enumerate() {
                if cancel_token.is_cancelled() {
                    return;
                }
                let sentence_chunk_size = Self::next_chunk_size(chunk_size, sentences_seen);
                let stream = match provider.model.stream_synthesis(
                    phonemes,
                    sentence_chunk_size,
                    chunk_padding,
                    cancel_token.clone(),
                ) {
                    Ok(stream) => stream,
                    Err(e) => {
                        tx.send(Err(e)).ok();
                        return;
                    }
                };
                let drained = Self::process_rt_stream(
                    stream,
                    &tx,
                    provider.output_config.as_ref(),
                    sample_rate,
                    num_channels,
                    &cancel_token,
                );
                if drained.is_err() {
                    return;
                }
            }
        });
        Ok(Self { rx, cancel_token })
    }

    #[inline(always)]
    fn process_rt_stream(
        stream: AudioStreamIterator,
        tx: &Sender<DengjenResult<AudioSamples>>,
        audio_output_config: Option<&AudioOutputConfig>,
        sample_rate: usize,
        num_channels: usize,
        cancel_token: &CancellationToken,
    ) -> Result<(), SendError<DengjenResult<AudioSamples>>> {
        for result in stream {
            if cancel_token.is_cancelled() {
                return Ok(());
            }
            let outgoing = match (result, audio_output_config) {
                (Ok(samples), Some(output_config)) => {
                    output_config.apply_to_raw_samples(samples, sample_rate, num_channels)
                }
                (Ok(samples), None) => Ok(samples),
                (Err(e), _) => Err(e),
            };
            tx.send(outgoing)?;
        }
        if !cancel_token.is_cancelled() {
            if let Some(output_config) = audio_output_config {
                if let Some(silence_ms) = output_config.appended_silence_ms {
                    let silence_result =
                        output_config.generate_silence(silence_ms as usize, sample_rate, num_channels);
                    tx.send(silence_result)?;
                }
            }
        }
        Ok(())
    }
}

impl Iterator for RealtimeSpeechStream {
    type Item = DengjenResult<AudioSamples>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cancel_token.is_cancelled() {
            None
        } else {
            self.rx.recv().ok()
        }
    }
}

#[cfg(test)]
mod chunk_size_growth_tests {
    use super::*;

    #[test]
    fn first_sentence_uses_base_chunk_size_unmodified() {
        assert_eq!(RealtimeSpeechStream::next_chunk_size(72, 0), 72);
    }

    #[test]
    fn ramps_additively_up_to_the_cap() {
        assert_eq!(RealtimeSpeechStream::next_chunk_size(72, 0), 72);
        assert_eq!(RealtimeSpeechStream::next_chunk_size(72, 1), 144);
        assert_eq!(RealtimeSpeechStream::next_chunk_size(72, 2), 216);
        assert_eq!(RealtimeSpeechStream::next_chunk_size(72, 3), 288);
        assert_eq!(RealtimeSpeechStream::next_chunk_size(72, 4), 360);
    }

    #[test]
    fn plateaus_after_the_ramp_cap_instead_of_continuing_to_grow() {
        let at_cap = RealtimeSpeechStream::next_chunk_size(72, 4);
        let well_past_cap = RealtimeSpeechStream::next_chunk_size(72, 50);
        assert_eq!(
            at_cap, well_past_cap,
            "growth must stop increasing once sentences_seen exceeds the ramp cap"
        );
    }

    #[test]
    fn result_is_clamped_to_max_chunk_size() {
        assert_eq!(
            RealtimeSpeechStream::next_chunk_size(300_000, 4),
            MAX_STREAM_CHUNK_SIZE,
            "300_000 * 5 = 1_500_000 must be clamped down to MAX_STREAM_CHUNK_SIZE"
        );
    }

    #[test]
    fn never_overflows_or_panics_on_pathological_inputs() {
        let result = RealtimeSpeechStream::next_chunk_size(usize::MAX, usize::MAX);
        assert_eq!(result, MAX_STREAM_CHUNK_SIZE);
    }

    #[test]
    fn growth_never_decreases_across_a_multi_sentence_stream_issue_28_regression() {
        // Regression guard for issue #28: the old formula derived each sentence's
        // chunk_size from the *previous* sentence's chunk count, which tended to
        // synthesize a whole sentence in ~1 chunk and then reset back near `base`
        // for the next one, alternating small/large/small chunk sizes. The new
        // formula is keyed on how many sentences have been seen, not on chunk
        // counts, so it must never step backwards.
        let sizes: Vec<usize> = (0..20)
            .map(|sentences_seen| RealtimeSpeechStream::next_chunk_size(72, sentences_seen))
            .collect();
        for window in sizes.windows(2) {
            assert!(
                window[1] >= window[0],
                "chunk_size must never decrease between sentences, got {sizes:?}"
            );
        }
    }
}

#[cfg(test)]
mod cancellation_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    struct CountingStreamModel {
        chunks_per_sentence: usize,
        sentences: usize,
        produced: Arc<AtomicUsize>,
        delay: Option<std::time::Duration>,
        chunk_sizes_seen: Arc<Mutex<Vec<usize>>>,
    }

    impl DengjenModel for CountingStreamModel {
        fn audio_output_info(&self) -> DengjenResult<AudioInfo> {
            Ok(AudioInfo { sample_rate: 16000, num_channels: 1, sample_width: 2 })
        }
        fn phonemize_text(&self, _text: &str) -> DengjenResult<Phonemes> {
            Ok(Phonemes::from(vec!["sentence".to_string(); self.sentences]))
        }
        fn speak_batch(&self, _phoneme_batches: Vec<String>) -> DengjenResult<Vec<Audio>> {
            Ok(Vec::new())
        }
        fn speak_one_sentence(&self, _phonemes: String) -> DengjenAudioResult {
            Err(DengjenError::OperationError("not used by this test".to_string()))
        }
        fn get_default_synthesis_config(&self) -> DengjenResult<SynthesisConfig> {
            Ok(SynthesisConfig::None)
        }
        fn get_fallback_synthesis_config(&self) -> DengjenResult<SynthesisConfig> {
            Ok(SynthesisConfig::None)
        }
        fn set_fallback_synthesis_config(&self, _c: &SynthesisConfig) -> DengjenResult<()> {
            Ok(())
        }
        fn supports_streaming_output(&self) -> bool {
            true
        }
        fn stream_synthesis(
            &self,
            _phonemes: String,
            chunk_size: usize,
            _chunk_padding: usize,
            cancel_token: CancellationToken,
        ) -> DengjenResult<AudioStreamIterator<'_>> {
            self.chunk_sizes_seen.lock().unwrap().push(chunk_size);
            let produced = Arc::clone(&self.produced);
            let n = self.chunks_per_sentence;
            let delay = self.delay;
            let iter = (0..n).map_while(move |_| {
                if let Some(delay) = delay {
                    std::thread::sleep(delay);
                }
                if cancel_token.is_cancelled() {
                    None
                } else {
                    produced.fetch_add(1, Ordering::SeqCst);
                    Some(Ok(AudioSamples::from(vec![0.0f32; 4])))
                }
            });
            Ok(Box::new(iter))
        }
    }

    #[test]
    fn cancelling_mid_stream_stops_further_chunks() {
        let produced = Arc::new(AtomicUsize::new(0));
        let model: Arc<dyn DengjenModel + Send + Sync> = Arc::new(CountingStreamModel {
            chunks_per_sentence: 1000,
            sentences: 5,
            produced: Arc::clone(&produced),
            delay: Some(std::time::Duration::from_millis(1)),
            chunk_sizes_seen: Arc::new(Mutex::new(Vec::new())),
        });
        let synth = DengjenSpeechSynthesizer::new(model).unwrap();
        let cancel_token = CancellationToken::new();
        let stream = synth
            .synthesize_streamed("irrelevant".to_string(), None, 10, 0, cancel_token.clone())
            .unwrap();

        let mut received = 0;
        for result in stream {
            let _ = result.unwrap();
            received += 1;
            if received == 3 {
                cancel_token.cancel();
            }
        }

        assert!(
            received < 100,
            "expected cancellation to truncate the stream promptly, got {received} chunks"
        );
        assert!(
            produced.load(Ordering::SeqCst) < 100,
            "expected production to stop promptly, produced {} chunks",
            produced.load(Ordering::SeqCst)
        );
    }

    #[test]
    fn cancelling_stops_delivery_of_already_buffered_chunks() {
        // The producer runs ahead into an unbounded channel; without a consumer-side
        // cancellation check the iterator would keep draining everything already buffered.
        let produced = Arc::new(AtomicUsize::new(0));
        let model: Arc<dyn DengjenModel + Send + Sync> = Arc::new(CountingStreamModel {
            chunks_per_sentence: 500,
            sentences: 3,
            produced: Arc::clone(&produced),
            delay: None,
            chunk_sizes_seen: Arc::new(Mutex::new(Vec::new())),
        });
        let synth = DengjenSpeechSynthesizer::new(model).unwrap();
        let cancel_token = CancellationToken::new();
        let mut stream = synth
            .synthesize_streamed("irrelevant".to_string(), None, 10, 0, cancel_token.clone())
            .unwrap();

        let mut received = 0;
        while received < 3 {
            let _ = stream.next().expect("stream ended before 3 chunks").unwrap();
            received += 1;
        }

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while produced.load(Ordering::SeqCst) < 200 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let buffered_at_cancel = produced.load(Ordering::SeqCst);
        assert!(
            buffered_at_cancel >= 200,
            "test setup failed: producer only made {buffered_at_cancel} chunks, so nothing was buffered"
        );

        cancel_token.cancel();

        assert!(
            stream.next().is_none(),
            "consumer kept delivering buffered chunks after cancellation"
        );
        assert_eq!(received, 3);
    }

    #[test]
    fn chunk_size_growth_stays_bounded_and_never_oscillates_across_many_sentences() {
        // Regression test for both the overflow in issue #24 (growth must stay
        // bounded across a long stream) and the oscillation in issue #28 (growth
        // must ramp up and plateau, never drop back toward `base` mid-stream).
        let produced = Arc::new(AtomicUsize::new(0));
        let chunk_sizes_seen = Arc::new(Mutex::new(Vec::new()));
        let model: Arc<dyn DengjenModel + Send + Sync> = Arc::new(CountingStreamModel {
            chunks_per_sentence: 300,
            sentences: 20,
            produced: Arc::clone(&produced),
            delay: None,
            chunk_sizes_seen: Arc::clone(&chunk_sizes_seen),
        });
        let synth = DengjenSpeechSynthesizer::new(model).unwrap();
        let cancel_token = CancellationToken::new();
        let stream = synth
            .synthesize_streamed("irrelevant".to_string(), None, 72, 0, cancel_token)
            .unwrap();

        let mut received = 0;
        for result in stream {
            let _ = result.expect("stream must not error under bounded chunk_size growth");
            received += 1;
        }

        assert_eq!(
            received,
            300 * 20,
            "expected every produced chunk to be delivered without the stream aborting"
        );

        let seen = chunk_sizes_seen.lock().unwrap();
        assert_eq!(seen.len(), 20, "expected one stream_synthesis call per sentence");
        for &size in seen.iter() {
            assert!(
                size <= MAX_STREAM_CHUNK_SIZE,
                "chunk_size {size} exceeded MAX_STREAM_CHUNK_SIZE, growth is not bounded"
            );
        }
        assert_eq!(
            &seen[0..5],
            &[72, 144, 216, 288, 360],
            "expected the additive ramp for the first 5 sentences"
        );
        for &size in &seen[5..] {
            assert_eq!(
                size, 360,
                "growth must plateau at 5x base after the ramp cap, not continue growing or drop back down"
            );
        }
    }

    #[test]
    fn some_output_config_processes_chunks_and_appends_silence_per_sentence() {
        let chunks_per_sentence = 3;
        let sentences = 2;
        let produced = Arc::new(AtomicUsize::new(0));
        let model: Arc<dyn DengjenModel + Send + Sync> = Arc::new(CountingStreamModel {
            chunks_per_sentence,
            sentences,
            produced: Arc::clone(&produced),
            delay: None,
            chunk_sizes_seen: Arc::new(Mutex::new(Vec::new())),
        });
        let synth = DengjenSpeechSynthesizer::new(model).unwrap();
        let cancel_token = CancellationToken::new();
        let output_config = AudioOutputConfig {
            rate: None,
            volume: None,
            pitch: None,
            appended_silence_ms: Some(1000),
        };
        let stream = synth
            .synthesize_streamed(
                "irrelevant".to_string(),
                Some(output_config),
                10,
                0,
                cancel_token,
            )
            .unwrap();

        let chunks: Vec<AudioSamples> = stream.map(|result| result.unwrap()).collect();

        // Each sentence's stream drains, then gets its own trailing silence chunk
        // appended (process_rt_stream sends it once per sentence, not once overall).
        let group_size = chunks_per_sentence + 1;
        assert_eq!(
            chunks.len(),
            group_size * sentences,
            "expected {chunks_per_sentence} real chunks plus 1 appended-silence chunk per sentence"
        );

        for sentence_index in 0..sentences {
            let silence_position = sentence_index * group_size + chunks_per_sentence;
            let silence_chunk = &chunks[silence_position];
            assert_eq!(
                silence_chunk.len(),
                16000,
                "1000ms of appended silence @ 16000Hz must be 16000 samples"
            );
            let max_abs = silence_chunk
                .as_vec()
                .iter()
                .fold(0f32, |a, &b| a.max(b.abs()));
            assert_eq!(max_abs, 0.0, "appended silence chunk must contain only zeros");
        }
    }
}

#[cfg(test)]
mod audio_output_config_tests {
    use super::*;

    fn sine_samples(n: usize) -> Vec<f32> {
        (0..n).map(|i| (i as f32 * 0.01).sin() * 0.5).collect()
    }

    #[test]
    fn apply_to_raw_samples_on_empty_input_is_a_noop() {
        let config = AudioOutputConfig {
            rate: Some(50),
            volume: Some(50),
            pitch: Some(50),
            appended_silence_ms: None,
        };
        let result = config
            .apply_to_raw_samples(AudioSamples::from(Vec::new()), 16000, 1)
            .unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn apply_to_raw_samples_with_no_config_set_preserves_length_and_signal() {
        let config = AudioOutputConfig {
            rate: None,
            volume: None,
            pitch: None,
            appended_silence_ms: None,
        };
        let input = sine_samples(1000);
        let result = config
            .apply_to_raw_samples(AudioSamples::from(input.clone()), 16000, 1)
            .unwrap();
        assert_eq!(result.len(), input.len());
        let max_diff = result
            .as_vec()
            .iter()
            .zip(input.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0f32, f32::max);
        assert!(
            max_diff < 0.01,
            "expected near-identity passthrough with no config set, max diff was {max_diff}"
        );
    }

    #[test]
    fn apply_to_raw_samples_with_volume_zero_mutes_the_signal() {
        let config = AudioOutputConfig {
            rate: None,
            volume: Some(0),
            pitch: None,
            appended_silence_ms: None,
        };
        let input = sine_samples(1000);
        let result = config
            .apply_to_raw_samples(AudioSamples::from(input), 16000, 1)
            .unwrap();
        let max_abs = result.as_vec().iter().fold(0f32, |a, &b| a.max(b.abs()));
        assert_eq!(max_abs, 0.0, "volume=0 must mute the signal entirely");
    }

    #[test]
    fn apply_to_raw_samples_with_volume_100_preserves_amplitude() {
        let config = AudioOutputConfig {
            rate: None,
            volume: Some(100),
            pitch: None,
            appended_silence_ms: None,
        };
        let input = sine_samples(1000);
        let result = config
            .apply_to_raw_samples(AudioSamples::from(input), 16000, 1)
            .unwrap();
        let max_abs = result.as_vec().iter().fold(0f32, |a, &b| a.max(b.abs()));
        assert!(
            (max_abs - 0.5).abs() < 0.01,
            "expected peak amplitude close to the input's 0.5, got {max_abs}"
        );
    }

    #[test]
    fn generate_silence_produces_the_expected_sample_count_and_is_silent() {
        let config = AudioOutputConfig {
            rate: None,
            volume: Some(50),
            pitch: None,
            appended_silence_ms: None,
        };
        let silence = config.generate_silence(1000, 16000, 1).unwrap();
        assert_eq!(silence.len(), 16000, "1000ms @ 16000Hz must be 16000 samples");
        let max_abs = silence.as_vec().iter().fold(0f32, |a, &b| a.max(b.abs()));
        assert_eq!(max_abs, 0.0, "generated silence must contain only zeros");
    }

    #[test]
    fn apply_appends_generated_silence_when_configured() {
        let config = AudioOutputConfig {
            rate: None,
            volume: None,
            pitch: None,
            appended_silence_ms: Some(500),
        };
        let samples = AudioSamples::from(sine_samples(100));
        let audio = Audio::new(samples, 16000, None);
        let original_len = audio.len();
        let result = config.apply(audio).unwrap();
        assert_eq!(
            result.len(),
            original_len + 8000,
            "500ms @ 16000Hz of silence (8000 samples) must be appended"
        );
    }
}

#[cfg(test)]
mod lazy_parallel_tests {
    use super::*;

    struct CannedSentenceModel {
        sentences: Vec<&'static str>,
        fail_on: Option<&'static str>,
    }

    impl DengjenModel for CannedSentenceModel {
        fn audio_output_info(&self) -> DengjenResult<AudioInfo> {
            Ok(AudioInfo { sample_rate: 16000, num_channels: 1, sample_width: 2 })
        }
        fn phonemize_text(&self, _text: &str) -> DengjenResult<Phonemes> {
            Ok(Phonemes::from(
                self.sentences
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>(),
            ))
        }
        fn speak_batch(&self, phoneme_batches: Vec<String>) -> DengjenResult<Vec<Audio>> {
            phoneme_batches
                .into_iter()
                .map(|ph| self.speak_one_sentence(ph))
                .collect()
        }
        fn speak_one_sentence(&self, phonemes: String) -> DengjenAudioResult {
            if self.fail_on == Some(phonemes.as_str()) {
                return Err(DengjenError::OperationError(format!(
                    "synthesis failed for {phonemes}"
                )));
            }
            let n = phonemes.len();
            let samples = AudioSamples::from(vec![n as f32; n]);
            Ok(Audio::new(samples, 16000, None))
        }
        fn get_default_synthesis_config(&self) -> DengjenResult<SynthesisConfig> {
            Ok(SynthesisConfig::None)
        }
        fn get_fallback_synthesis_config(&self) -> DengjenResult<SynthesisConfig> {
            Ok(SynthesisConfig::None)
        }
        fn set_fallback_synthesis_config(&self, _c: &SynthesisConfig) -> DengjenResult<()> {
            Ok(())
        }
    }

    #[test]
    fn lazy_stream_yields_one_result_per_sentence_in_order() {
        let model: Arc<dyn DengjenModel + Send + Sync> = Arc::new(CannedSentenceModel {
            sentences: vec!["a", "bb", "ccc"],
            fail_on: None,
        });
        let synth = DengjenSpeechSynthesizer::new(model).unwrap();
        let results: Vec<_> = synth
            .synthesize_lazy("irrelevant".to_string(), None)
            .unwrap()
            .collect();
        assert_eq!(results.len(), 3);
        let lens: Vec<usize> = results.into_iter().map(|r| r.unwrap().len()).collect();
        assert_eq!(lens, vec![1, 2, 3], "lazy stream must preserve sentence order");
    }

    #[test]
    fn lazy_stream_propagates_a_sentence_level_error() {
        let model: Arc<dyn DengjenModel + Send + Sync> = Arc::new(CannedSentenceModel {
            sentences: vec!["a", "bb", "ccc"],
            fail_on: Some("bb"),
        });
        let synth = DengjenSpeechSynthesizer::new(model).unwrap();
        let results: Vec<_> = synth
            .synthesize_lazy("irrelevant".to_string(), None)
            .unwrap()
            .collect();
        assert_eq!(results.len(), 3);
        assert!(results[0].is_ok());
        assert!(matches!(results[1], Err(DengjenError::OperationError(_))));
        assert!(results[2].is_ok());
    }

    #[test]
    fn parallel_stream_yields_one_result_per_sentence_in_order() {
        let model: Arc<dyn DengjenModel + Send + Sync> = Arc::new(CannedSentenceModel {
            sentences: vec!["a", "bb", "ccc"],
            fail_on: None,
        });
        let synth = DengjenSpeechSynthesizer::new(model).unwrap();
        let results: Vec<_> = synth
            .synthesize_parallel("irrelevant".to_string(), None)
            .unwrap()
            .collect();
        assert_eq!(results.len(), 3);
        let lens: Vec<usize> = results.into_iter().map(|r| r.unwrap().len()).collect();
        assert_eq!(
            lens,
            vec![1, 2, 3],
            "parallel stream must preserve sentence order in its results"
        );
    }

    #[test]
    fn parallel_stream_propagates_a_sentence_level_error() {
        let model: Arc<dyn DengjenModel + Send + Sync> = Arc::new(CannedSentenceModel {
            sentences: vec!["a", "bb", "ccc"],
            fail_on: Some("ccc"),
        });
        let synth = DengjenSpeechSynthesizer::new(model).unwrap();
        let results: Vec<_> = synth
            .synthesize_parallel("irrelevant".to_string(), None)
            .unwrap()
            .collect();
        assert_eq!(results.len(), 3);
        assert_eq!(results.iter().filter(|r| r.is_err()).count(), 1);
    }
}
