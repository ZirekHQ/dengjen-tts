use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use flume::{Receiver, SendError, Sender};
use once_cell::sync::Lazy;
use rayon::{prelude::*, ThreadPool, ThreadPoolBuilder};

mod utils;
pub use dengjen_tts_core::*;

/// A closed interval that one of Sonic's post-processing knobs (speed,
/// volume, pitch) is scaled onto from a `0..=100` percentage.
struct ParamRange {
    min: f32,
    max: f32,
}

const SPEED_PARAM_RANGE: ParamRange = ParamRange { min: 0.5, max: 5.5 };
const VOLUME_PARAM_RANGE: ParamRange = ParamRange { min: 0.0, max: 1.0 };
const PITCH_PARAM_RANGE: ParamRange = ParamRange { min: 0.5, max: 1.5 };

/// Rayon pool that synthesis work runs on, kept off whatever thread calls
/// into this crate. Sized to a multiple of the core count so more
/// synthesis requests can run concurrently than there are cores.
pub static SYNTHESIS_THREAD_POOL: Lazy<ThreadPool> = Lazy::new(|| {
    let core_count = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(4);
    ThreadPoolBuilder::new()
        .num_threads(core_count * 4)
        .thread_name(|index| format!("dengjen_synth_{index}"))
        .build()
        .expect("thread pool construction only fails on invalid config, never at runtime")
});

/// Optional post-processing to run on synthesized audio before it reaches
/// the caller: speed/volume/pitch adjustments (each a `0..=100` percentage
/// of its own fixed range) and/or trailing silence to pad the clip with.
#[derive(Clone)]
pub struct AudioOutputConfig {
    pub rate: Option<u8>,
    pub volume: Option<u8>,
    pub pitch: Option<u8>,
    pub appended_silence_ms: Option<u32>,
}

impl AudioOutputConfig {
    fn apply(&self, mut audio: Audio) -> DengjenAudioResult {
        let mut samples = audio.samples.take();
        if let Some(silence_ms) = self.appended_silence_ms {
            let silence = self.generate_silence(
                silence_ms as usize,
                audio.info.sample_rate,
                audio.info.num_channels,
            )?;
            samples.extend(silence.into_vec());
        }
        let processed = self.apply_to_raw_samples(
            samples.into(),
            audio.info.sample_rate,
            audio.info.num_channels,
        )?;
        audio.samples.merge(processed);
        Ok(audio)
    }

    /// Runs `samples` through a fresh Sonic stream configured with whichever
    /// of `rate`/`volume`/`pitch` are set, returning the transformed audio.
    /// A no-op on empty input (Sonic has nothing meaningful to do with it).
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

        // SAFETY: every libsonic call below targets the `stream` handle this
        // block itself creates, and every exit from the block — the early
        // error return on a zero-sample result included — destroys that
        // same handle first, so no stream ever outlives this block.
        unsafe {
            let stream = sonic_sys::sonicCreateStream(sample_rate as i32, num_channels as i32);

            if let Some(pct) = self.rate {
                let speed =
                    utils::percent_to_param(pct, SPEED_PARAM_RANGE.min, SPEED_PARAM_RANGE.max);
                sonic_sys::sonicSetSpeed(stream, speed);
            }
            // volume == 0 (exact mute) is handled below, after Sonic has run, rather than by
            // calling sonicSetVolume(0.0): Sonic clamps its volume parameter to a minimum of
            // 0.01 internally, so asking it for silence doesn't actually produce silence.
            if let Some(pct) = self.volume.filter(|&pct| pct != 0) {
                let volume =
                    utils::percent_to_param(pct, VOLUME_PARAM_RANGE.min, VOLUME_PARAM_RANGE.max);
                sonic_sys::sonicSetVolume(stream, volume);
            }
            if let Some(pct) = self.pitch {
                let pitch =
                    utils::percent_to_param(pct, PITCH_PARAM_RANGE.min, PITCH_PARAM_RANGE.max);
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

            if self.volume == Some(0) {
                output.fill(0.0);
            }

            Ok(output.into())
        }
    }

    /// Builds `time_ms` worth of silence at the given rate/channel count and
    /// routes it through [`Self::apply_to_raw_samples`], so appended silence
    /// picks up the same rate/volume/pitch treatment as the real audio.
    fn generate_silence(
        &self,
        time_ms: usize,
        sample_rate: usize,
        num_channels: usize,
    ) -> DengjenResult<AudioSamples> {
        let sample_count = (time_ms * sample_rate) / 1000;
        let silence = vec![0f32; sample_count];
        self.apply_to_raw_samples(silence.into(), sample_rate, num_channels)
    }
}

/// Wraps a backend model behind the higher-level synthesis entry points
/// (`synthesize_lazy`/`synthesize_parallel`/`synthesize_streamed`/`synthesize_to_file`).
pub struct DengjenSpeechSynthesizer {
    backend: Arc<dyn DengjenModel + Sync + Send>,
}

impl DengjenSpeechSynthesizer {
    pub fn new(model: Arc<dyn DengjenModel + Sync + Send>) -> DengjenResult<Self> {
        Ok(Self { backend: model })
    }

    #[inline(always)]
    pub fn clone_model(&self) -> Arc<dyn DengjenModel + Send + Sync> {
        Arc::clone(&self.backend)
    }

    fn task_provider(
        &self,
        text: String,
        output_config: Option<AudioOutputConfig>,
    ) -> SpeechSynthesisTaskProvider {
        SpeechSynthesisTaskProvider {
            model: self.clone_model(),
            text,
            output_config,
        }
    }

    pub fn synthesize_lazy(
        &self,
        text: String,
        output_config: Option<AudioOutputConfig>,
    ) -> DengjenResult<DengjenSpeechStreamLazy> {
        DengjenSpeechStreamLazy::new(self.task_provider(text, output_config))
    }

    pub fn synthesize_parallel(
        &self,
        text: String,
        output_config: Option<AudioOutputConfig>,
    ) -> DengjenResult<DengjenSpeechStreamParallel> {
        DengjenSpeechStreamParallel::new(self.task_provider(text, output_config))
    }

    pub fn synthesize_streamed(
        &self,
        text: String,
        output_config: Option<AudioOutputConfig>,
        chunk_size: usize,
        chunk_padding: usize,
        cancel_token: CancellationToken,
    ) -> DengjenResult<RealtimeSpeechStream> {
        let info = self.backend.audio_output_info()?;
        RealtimeSpeechStream::new(
            self.task_provider(text, output_config),
            chunk_size,
            chunk_padding,
            info.sample_rate,
            info.num_channels,
            cancel_token,
        )
    }

    pub fn synthesize_to_file(
        &self,
        filename: &Path,
        text: String,
        output_config: Option<AudioOutputConfig>,
    ) -> DengjenResult<()> {
        let mut collected: Vec<f32> = Vec::new();
        for chunk in self.synthesize_parallel(text, output_config)? {
            collected.extend(chunk?.into_vec());
        }
        if collected.is_empty() {
            return Err(DengjenError::OperationError(
                "No speech data to write".to_string(),
            ));
        }

        let info = self.backend.audio_output_info()?;
        let samples = AudioSamples::from(collected);
        audio_ops::write_wave_samples_to_file(
            filename,
            samples.to_i16_vec().iter(),
            info.sample_rate as u32,
            info.num_channels.try_into().unwrap(),
            info.sample_width.try_into().unwrap(),
        )?;
        Ok(())
    }
}

impl DengjenModel for DengjenSpeechSynthesizer {
    fn audio_output_info(&self) -> DengjenResult<AudioInfo> {
        self.backend.audio_output_info()
    }
    fn phonemize_text(&self, text: &str) -> DengjenResult<Phonemes> {
        self.backend.phonemize_text(text)
    }
    fn speak_batch(&self, phoneme_batches: Vec<String>) -> DengjenResult<Vec<Audio>> {
        self.backend.speak_batch(phoneme_batches)
    }
    fn speak_one_sentence(&self, phonemes: String) -> DengjenAudioResult {
        self.backend.speak_one_sentence(phonemes)
    }
    fn get_default_synthesis_config(&self) -> DengjenResult<Option<SynthesisConfig>> {
        self.backend.get_default_synthesis_config()
    }
    fn get_fallback_synthesis_config(&self) -> DengjenResult<Option<SynthesisConfig>> {
        self.backend.get_fallback_synthesis_config()
    }
    fn set_fallback_synthesis_config(
        &self,
        synthesis_config: &SynthesisConfig,
    ) -> DengjenResult<()> {
        self.backend.set_fallback_synthesis_config(synthesis_config)
    }
    fn get_language(&self) -> DengjenResult<Option<String>> {
        self.backend.get_language()
    }
    fn get_speakers(&self) -> DengjenResult<Option<&HashMap<i64, String>>> {
        self.backend.get_speakers()
    }
    fn properties(&self) -> DengjenResult<HashMap<String, String>> {
        self.backend.properties()
    }
    fn supports_streaming_output(&self) -> bool {
        self.backend.supports_streaming_output()
    }
    fn stream_synthesis<'a>(
        &'a self,
        phonemes: String,
        chunk_size: usize,
        chunk_padding: usize,
        cancel_token: CancellationToken,
    ) -> DengjenResult<Box<dyn Iterator<Item = DengjenResult<AudioSamples>> + Send + Sync + 'a>>
    {
        self.backend
            .stream_synthesis(phonemes, chunk_size, chunk_padding, cancel_token)
    }
}

/// Groups everything a stream constructor needs to turn input text into
/// audio — the model to synthesize with, the text itself, and the optional
/// post-processing config — so callers pass one value instead of three.
struct SpeechSynthesisTaskProvider {
    model: Arc<dyn DengjenModel + Sync + Send>,
    text: String,
    output_config: Option<AudioOutputConfig>,
}

impl SpeechSynthesisTaskProvider {
    fn get_phonemes(&self) -> DengjenResult<Vec<String>> {
        Ok(self.model.phonemize_text(&self.text)?.to_vec())
    }

    fn shape_output(&self, audio: Audio) -> DengjenAudioResult {
        match &self.output_config {
            Some(config) => config.apply(audio),
            None => Ok(audio),
        }
    }

    fn process_one_sentence(&self, sentence: String) -> DengjenAudioResult {
        let audio = self.model.speak_one_sentence(sentence)?;
        self.shape_output(audio)
    }

    #[allow(dead_code)]
    fn process_batches(&self, sentences: Vec<String>) -> DengjenResult<Vec<Audio>> {
        self.model
            .speak_batch(sentences)?
            .into_iter()
            .map(|audio| self.shape_output(audio))
            .collect()
    }
}

/// Pulls one sentence's audio out of the underlying model on each
/// `Iterator::next` call, rather than synthesizing everything up front.
pub struct DengjenSpeechStreamLazy {
    provider: SpeechSynthesisTaskProvider,
    pending_sentences: std::vec::IntoIter<String>,
}

impl DengjenSpeechStreamLazy {
    fn new(provider: SpeechSynthesisTaskProvider) -> DengjenResult<Self> {
        let pending_sentences = provider.get_phonemes()?.into_iter();
        Ok(Self {
            provider,
            pending_sentences,
        })
    }
}

impl Iterator for DengjenSpeechStreamLazy {
    type Item = DengjenAudioResult;

    fn next(&mut self) -> Option<Self::Item> {
        self.pending_sentences
            .next()
            .map(|sentence| self.provider.process_one_sentence(sentence))
    }
}

/// Synthesizes every sentence up front via rayon's parallel iterator, then
/// replays the finished results one at a time. `par_iter().map().collect()`
/// preserves input order in its output `Vec`, so callers see results in the
/// same order as the source sentences even though synthesis itself ran
/// concurrently and completed in whatever order the thread pool finished.
#[must_use]
pub struct DengjenSpeechStreamParallel {
    finished: std::vec::IntoIter<DengjenAudioResult>,
}

impl DengjenSpeechStreamParallel {
    fn new(provider: SpeechSynthesisTaskProvider) -> DengjenResult<Self> {
        let sentences = provider.get_phonemes()?;
        let finished: Vec<DengjenAudioResult> = sentences
            .par_iter()
            .map(|sentence| provider.process_one_sentence(sentence.clone()))
            .collect();
        Ok(Self {
            finished: finished.into_iter(),
        })
    }
}

impl Iterator for DengjenSpeechStreamParallel {
    type Item = DengjenAudioResult;

    fn next(&mut self) -> Option<Self::Item> {
        self.finished.next()
    }
}

/// Upper bound on the value `next_chunk_size` can return, in whatever unit
/// the active backend's `chunk_size` parameter uses (mel frames for Piper,
/// samples for Kokoro). A backstop against pathological growth, not a
/// value any normal stream is expected to reach.
const MAX_STREAM_CHUNK_SIZE: usize = 1_000_000;

/// Delivers synthesized audio chunks as a background producer thread
/// generates them, so a caller can start playback before the whole text
/// finishes synthesizing.
pub struct RealtimeSpeechStream {
    rx: Receiver<DengjenResult<AudioSamples>>,
    cancel_token: CancellationToken,
}

impl RealtimeSpeechStream {
    /// Grows the chunk size by one extra multiple of `base_chunk_size` per
    /// sentence already seen, capped at 4 extra multiples (5x base total),
    /// then clamped to `MAX_STREAM_CHUNK_SIZE`. The growth is always relative
    /// to the fixed `base_chunk_size`, never to a value this function
    /// previously returned, so calling it repeatedly across a stream can
    /// only ramp up and plateau — never compound past the cap and never
    /// fall back down between sentences. Issue #28 was a regression where an
    /// earlier version violated that and let the size oscillate; keep this
    /// additive, non-resetting shape when touching this function.
    fn next_chunk_size(base_chunk_size: usize, sentences_seen: usize) -> usize {
        let extra_multiples = sentences_seen.min(4);
        let growth = base_chunk_size.saturating_mul(extra_multiples);
        base_chunk_size
            .saturating_add(growth)
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
            for (sentence_index, phonemes) in sentences.into_iter().enumerate() {
                if cancel_token.is_cancelled() {
                    return;
                }

                let this_chunk_size = Self::next_chunk_size(chunk_size, sentence_index);
                let stream = match provider.model.stream_synthesis(
                    phonemes,
                    this_chunk_size,
                    chunk_padding,
                    cancel_token.clone(),
                ) {
                    Ok(stream) => stream,
                    Err(err) => {
                        let _ = tx.send(Err(err));
                        return;
                    }
                };

                let stream_result = Self::process_rt_stream(
                    stream,
                    &tx,
                    provider.output_config.as_ref(),
                    sample_rate,
                    num_channels,
                    &cancel_token,
                );
                if stream_result.is_err() {
                    return;
                }
            }
        });

        Ok(Self { rx, cancel_token })
    }

    /// Drains one sentence's model stream into `tx`, applying the output
    /// config to each successful chunk and forwarding both successes and
    /// errors — a chunk is never silently dropped. Checks `cancel_token`
    /// before pulling each item so a mid-sentence cancellation stops the
    /// drain promptly. Appends one silence chunk after the sentence's own
    /// chunks if the config asks for it and the stream wasn't cancelled.
    fn process_rt_stream(
        stream: AudioStreamIterator,
        tx: &Sender<DengjenResult<AudioSamples>>,
        output_config: Option<&AudioOutputConfig>,
        sample_rate: usize,
        num_channels: usize,
        cancel_token: &CancellationToken,
    ) -> Result<(), SendError<DengjenResult<AudioSamples>>> {
        for chunk in stream {
            if cancel_token.is_cancelled() {
                return Ok(());
            }
            let shaped = match (chunk, output_config) {
                (Ok(samples), Some(config)) => {
                    config.apply_to_raw_samples(samples, sample_rate, num_channels)
                }
                (Ok(samples), None) => Ok(samples),
                (Err(err), _) => Err(err),
            };
            tx.send(shaped)?;
        }

        if cancel_token.is_cancelled() {
            return Ok(());
        }
        let Some(config) = output_config else {
            return Ok(());
        };
        let Some(silence_ms) = config.appended_silence_ms else {
            return Ok(());
        };
        let silence = config.generate_silence(silence_ms as usize, sample_rate, num_channels);
        tx.send(silence)
    }
}

impl Iterator for RealtimeSpeechStream {
    type Item = DengjenResult<AudioSamples>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cancel_token.is_cancelled() {
            return None;
        }
        self.rx.recv().ok()
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
            Ok(AudioInfo {
                sample_rate: 16000,
                num_channels: 1,
                sample_width: 2,
            })
        }
        fn phonemize_text(&self, _text: &str) -> DengjenResult<Phonemes> {
            Ok(Phonemes::from(vec!["sentence".to_string(); self.sentences]))
        }
        fn speak_batch(&self, _phoneme_batches: Vec<String>) -> DengjenResult<Vec<Audio>> {
            Ok(Vec::new())
        }
        fn speak_one_sentence(&self, _phonemes: String) -> DengjenAudioResult {
            Err(DengjenError::OperationError(
                "not used by this test".to_string(),
            ))
        }
        fn get_default_synthesis_config(&self) -> DengjenResult<Option<SynthesisConfig>> {
            Ok(None)
        }
        fn get_fallback_synthesis_config(&self) -> DengjenResult<Option<SynthesisConfig>> {
            Ok(None)
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
            let _ = stream
                .next()
                .expect("stream ended before 3 chunks")
                .unwrap();
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
        assert_eq!(
            seen.len(),
            20,
            "expected one stream_synthesis call per sentence"
        );
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
            assert_eq!(
                max_abs, 0.0,
                "appended silence chunk must contain only zeros"
            );
        }
    }
}

#[cfg(test)]
mod realtime_stream_error_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    struct FailingStreamModel {
        sentences: usize,
        fail_on_call: usize,
        calls: Arc<AtomicUsize>,
    }

    impl DengjenModel for FailingStreamModel {
        fn audio_output_info(&self) -> DengjenResult<AudioInfo> {
            Ok(AudioInfo {
                sample_rate: 16000,
                num_channels: 1,
                sample_width: 2,
            })
        }
        fn phonemize_text(&self, _text: &str) -> DengjenResult<Phonemes> {
            Ok(Phonemes::from(vec!["sentence".to_string(); self.sentences]))
        }
        fn speak_batch(&self, _phoneme_batches: Vec<String>) -> DengjenResult<Vec<Audio>> {
            Ok(Vec::new())
        }
        fn speak_one_sentence(&self, _phonemes: String) -> DengjenAudioResult {
            Err(DengjenError::OperationError(
                "not used by this test".to_string(),
            ))
        }
        fn get_default_synthesis_config(&self) -> DengjenResult<Option<SynthesisConfig>> {
            Ok(None)
        }
        fn get_fallback_synthesis_config(&self) -> DengjenResult<Option<SynthesisConfig>> {
            Ok(None)
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
            _chunk_size: usize,
            _chunk_padding: usize,
            _cancel_token: CancellationToken,
        ) -> DengjenResult<AudioStreamIterator<'_>> {
            let call_index = self.calls.fetch_add(1, Ordering::SeqCst);
            if call_index == self.fail_on_call {
                return Err(DengjenError::InferenceError(
                    "synthetic stream_synthesis failure".to_string(),
                ));
            }
            Ok(Box::new(std::iter::once(Ok(AudioSamples::from(vec![
                0.0f32; 4
            ])))))
        }
    }

    #[test]
    fn stream_synthesis_failure_on_a_later_sentence_surfaces_and_stops_further_production() {
        let calls = Arc::new(AtomicUsize::new(0));
        let model: Arc<dyn DengjenModel + Send + Sync> = Arc::new(FailingStreamModel {
            sentences: 3,
            fail_on_call: 1,
            calls: Arc::clone(&calls),
        });
        let synth = DengjenSpeechSynthesizer::new(model).unwrap();
        let cancel_token = CancellationToken::new();
        let stream = synth
            .synthesize_streamed("irrelevant".to_string(), None, 10, 0, cancel_token)
            .unwrap();

        let items: Vec<DengjenResult<AudioSamples>> = stream.collect();

        assert_eq!(
            items.len(),
            2,
            "expected the first sentence's one chunk plus the second sentence's error, nothing after"
        );
        assert!(items[0].is_ok(), "first sentence's chunk should succeed");
        match &items[1] {
            Err(DengjenError::InferenceError(msg)) => {
                assert_eq!(msg, "synthetic stream_synthesis failure");
            }
            other => panic!("expected the injected InferenceError, got {other:?}"),
        }
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "producer must stop after the failing sentence, never reaching the third"
        );
    }

    struct MidStreamErrorModel {
        sentences: usize,
        chunks: Mutex<Option<Vec<DengjenResult<AudioSamples>>>>,
    }

    impl DengjenModel for MidStreamErrorModel {
        fn audio_output_info(&self) -> DengjenResult<AudioInfo> {
            Ok(AudioInfo {
                sample_rate: 16000,
                num_channels: 1,
                sample_width: 2,
            })
        }
        fn phonemize_text(&self, _text: &str) -> DengjenResult<Phonemes> {
            Ok(Phonemes::from(vec!["sentence".to_string(); self.sentences]))
        }
        fn speak_batch(&self, _phoneme_batches: Vec<String>) -> DengjenResult<Vec<Audio>> {
            Ok(Vec::new())
        }
        fn speak_one_sentence(&self, _phonemes: String) -> DengjenAudioResult {
            Err(DengjenError::OperationError(
                "not used by this test".to_string(),
            ))
        }
        fn get_default_synthesis_config(&self) -> DengjenResult<Option<SynthesisConfig>> {
            Ok(None)
        }
        fn get_fallback_synthesis_config(&self) -> DengjenResult<Option<SynthesisConfig>> {
            Ok(None)
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
            _chunk_size: usize,
            _chunk_padding: usize,
            _cancel_token: CancellationToken,
        ) -> DengjenResult<AudioStreamIterator<'_>> {
            let chunks = self
                .chunks
                .lock()
                .unwrap()
                .take()
                .expect("stream_synthesis called more than once in this test");
            Ok(Box::new(chunks.into_iter()))
        }
    }

    #[test]
    fn mid_stream_chunk_error_is_forwarded_to_the_consumer_not_dropped() {
        let chunks = vec![
            Ok(AudioSamples::from(vec![0.0f32; 4])),
            Err(DengjenError::InferenceError(
                "synthetic mid-stream failure".to_string(),
            )),
            Ok(AudioSamples::from(vec![0.0f32; 4])),
        ];
        let model: Arc<dyn DengjenModel + Send + Sync> = Arc::new(MidStreamErrorModel {
            sentences: 1,
            chunks: Mutex::new(Some(chunks)),
        });
        let synth = DengjenSpeechSynthesizer::new(model).unwrap();
        let cancel_token = CancellationToken::new();
        let stream = synth
            .synthesize_streamed("irrelevant".to_string(), None, 10, 0, cancel_token)
            .unwrap();

        let items: Vec<DengjenResult<AudioSamples>> = stream.collect();

        assert_eq!(
            items.len(),
            3,
            "expected all 3 chunks including the mid-stream error, not truncated"
        );
        assert!(items[0].is_ok());
        match &items[1] {
            Err(DengjenError::InferenceError(msg)) => {
                assert_eq!(msg, "synthetic mid-stream failure");
            }
            other => panic!("expected the injected InferenceError, got {other:?}"),
        }
        assert!(
            items[2].is_ok(),
            "chunks after a mid-stream error must still be forwarded, not dropped"
        );
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
        assert_eq!(
            silence.len(),
            16000,
            "1000ms @ 16000Hz must be 16000 samples"
        );
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
            Ok(AudioInfo {
                sample_rate: 16000,
                num_channels: 1,
                sample_width: 2,
            })
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
        fn get_default_synthesis_config(&self) -> DengjenResult<Option<SynthesisConfig>> {
            Ok(None)
        }
        fn get_fallback_synthesis_config(&self) -> DengjenResult<Option<SynthesisConfig>> {
            Ok(None)
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
        assert_eq!(
            lens,
            vec![1, 2, 3],
            "lazy stream must preserve sentence order"
        );
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
