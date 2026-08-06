mod utils;
pub use dengjen_core::*;

use flume::{Receiver, SendError, Sender};
use once_cell::sync::Lazy;
use rayon::prelude::*;
use rayon::{ThreadPool, ThreadPoolBuilder};
use std::any::Any;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

const RATE_RANGE: (f32, f32) = (0.5f32, 5.5f32);
const VOLUME_RANGE: (f32, f32) = (0.0f32, 1.0f32);
const PITCH_RANGE: (f32, f32) = (0.5f32, 1.5f32);

pub static SYNTHESIS_THREAD_POOL: Lazy<ThreadPool> = Lazy::new(|| {
    let num_cpus = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(4);
    ThreadPoolBuilder::new()
        .thread_name(|i| format!("dengjen_synth_{}", i))
        .num_threads(num_cpus * 4)
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
        let mut samples = audio.samples.take();
        if let Some(time_ms) = self.appended_silence_ms {
            let mut silence_samples = self.generate_silence(
                time_ms as usize,
                audio.info.sample_rate,
                audio.info.num_channels,
            )?;
            samples.append(silence_samples.take().as_mut());
        }
        let mut samples = self.apply_to_raw_samples(
            samples.into(),
            audio.info.sample_rate,
            audio.info.num_channels,
        )?;
        audio.samples.as_mut_vec().append(samples.as_mut_vec());
        Ok(audio)
    }
    fn apply_to_raw_samples(
        &self,
        samples: AudioSamples,
        sample_rate: usize,
        num_channels: usize,
    ) -> DengjenResult<AudioSamples> {
        let samples = samples.into_vec();
        let input_len = samples.len();
        if input_len == 0 {
            return Ok(samples.into());
        }
        let mut out_buf: Vec<f32> = Vec::new();
        unsafe {
            let stream = sonic_sys::sonicCreateStream(sample_rate as i32, num_channels as i32);
            if let Some(rate) = self.rate {
                sonic_sys::sonicSetSpeed(
                    stream,
                    utils::percent_to_param(rate, RATE_RANGE.0, RATE_RANGE.1),
                );
            }
            if let Some(volume) = self.volume {
                sonic_sys::sonicSetVolume(
                    stream,
                    utils::percent_to_param(volume, VOLUME_RANGE.0, VOLUME_RANGE.1),
                );
            }
            if let Some(pitch) = self.pitch {
                sonic_sys::sonicSetPitch(
                    stream,
                    utils::percent_to_param(pitch, PITCH_RANGE.0, PITCH_RANGE.1),
                );
            }
            sonic_sys::sonicWriteFloatToStream(stream, samples.as_ptr(), input_len as i32);
            sonic_sys::sonicFlushStream(stream);
            let num_samples = sonic_sys::sonicSamplesAvailable(stream);
            if num_samples <= 0 {
                return Err(
                    DengjenError::OperationError("Sonic Error: failed to apply audio config. Invalid parameter value for rate, volume, or pitch".to_string())
                );
            }
            out_buf.reserve_exact(num_samples as usize);
            sonic_sys::sonicReadFloatFromStream(
                stream,
                out_buf.spare_capacity_mut().as_mut_ptr().cast(),
                num_samples,
            );
            sonic_sys::sonicDestroyStream(stream);
            out_buf.set_len(num_samples as usize);
        }
        Ok(out_buf.into())
    }
    #[inline(always)]
    fn generate_silence(
        &self,
        time_ms: usize,
        sample_rate: usize,
        num_channels: usize,
    ) -> DengjenResult<AudioSamples> {
        let num_samples = (time_ms * sample_rate) / 1000;
        let silence_samples = vec![0f32; num_samples];
        self.apply_to_raw_samples(silence_samples.into(), sample_rate, num_channels)
    }
}

pub struct DengjenSpeechSynthesizer(Arc<dyn DengjenModel + Sync + Send>);

impl DengjenSpeechSynthesizer {
    pub fn new(model: Arc<dyn DengjenModel + Sync + Send>) -> DengjenResult<Self> {
        Ok(Self(model))
    }

    fn create_synthesis_task_provider(
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
        DengjenSpeechStreamLazy::new(self.create_synthesis_task_provider(text, output_config))
    }
    pub fn synthesize_parallel(
        &self,
        text: String,
        output_config: Option<AudioOutputConfig>,
    ) -> DengjenResult<DengjenSpeechStreamParallel> {
        DengjenSpeechStreamParallel::new(self.create_synthesis_task_provider(text, output_config))
    }
    pub fn synthesize_streamed(
        &self,
        text: String,
        output_config: Option<AudioOutputConfig>,
        chunk_size: usize,
        chunk_padding: usize,
        cancel_token: CancellationToken,
    ) -> DengjenResult<RealtimeSpeechStream> {
        let provider = self.create_synthesis_task_provider(text, output_config);
        let wavinfo = self.0.audio_output_info()?;
        RealtimeSpeechStream::new(
            provider,
            chunk_size,
            chunk_padding,
            wavinfo.sample_rate,
            wavinfo.num_channels,
            cancel_token,
        )
    }

    pub fn synthesize_to_file(
        &self,
        filename: &Path,
        text: String,
        output_config: Option<AudioOutputConfig>,
    ) -> DengjenResult<()> {
        let mut samples: Vec<f32> = Vec::new();
        for result in self.synthesize_parallel(text, output_config)? {
            let ws = result?;
            samples.append(&mut ws.into_vec());
        }
        if samples.is_empty() {
            return Err(DengjenError::OperationError(
                "No speech data to write".to_string(),
            ));
        }
        let audio = AudioSamples::from(samples);
        Ok(audio_ops::write_wave_samples_to_file(
            filename,
            audio.to_i16_vec().iter(),
            self.0.audio_output_info()?.sample_rate as u32,
            self.0.audio_output_info()?.num_channels.try_into().unwrap(),
            self.0.audio_output_info()?.sample_width.try_into().unwrap(),
        )?)
    }
    #[inline(always)]
    pub fn clone_model(&self) -> Arc<dyn DengjenModel + Send + Sync> {
        Arc::clone(&self.0)
    }
}

impl DengjenModel for DengjenSpeechSynthesizer {
    fn audio_output_info(&self) -> DengjenResult<AudioInfo> {
        self.0.audio_output_info()
    }
    fn phonemize_text(&self, text: &str) -> DengjenResult<Phonemes> {
        self.0.phonemize_text(text)
    }
    fn speak_batch(&self, phoneme_batches: Vec<String>) -> DengjenResult<Vec<Audio>> {
        self.0.speak_batch(phoneme_batches)
    }
    fn speak_one_sentence(&self, phonemes: String) -> DengjenAudioResult {
        self.0.speak_one_sentence(phonemes)
    }
    fn get_default_synthesis_config(&self) -> DengjenResult<Box<dyn Any>> {
        self.0.get_default_synthesis_config()
    }
    fn get_fallback_synthesis_config(&self) -> DengjenResult<Box<dyn Any>> {
        self.0.get_fallback_synthesis_config()
    }
    fn set_fallback_synthesis_config(&self, synthesis_config: &dyn Any) -> DengjenResult<()> {
        self.0.set_fallback_synthesis_config(synthesis_config)
    }
    fn get_language(&self) -> DengjenResult<Option<String>> {
        self.0.get_language()
    }
    fn get_speakers(&self) -> DengjenResult<Option<&HashMap<i64, String>>> {
        self.0.get_speakers()
    }
    fn properties(&self) -> DengjenResult<HashMap<String, String>> {
        self.0.properties()
    }
    fn supports_streaming_output(&self) -> bool {
        self.0.supports_streaming_output()
    }
    fn stream_synthesis<'a>(
        &'a self,
        phonemes: String,
        chunk_size: usize,
        chunk_padding: usize,
        cancel_token: CancellationToken,
    ) -> DengjenResult<Box<dyn Iterator<Item = DengjenResult<AudioSamples>> + Send + Sync + 'a>> {
        self.0.stream_synthesis(phonemes, chunk_size, chunk_padding, cancel_token)
    }
}

struct SpeechSynthesisTaskProvider {
    model: Arc<dyn DengjenModel + Sync + Send>,
    text: String,
    output_config: Option<AudioOutputConfig>,
}

impl SpeechSynthesisTaskProvider {
    fn get_phonemes(&self) -> DengjenResult<Vec<String>> {
        Ok(self.model.phonemize_text(&self.text)?.to_vec())
    }
    fn process_one_sentence(&self, phonemes: String) -> DengjenAudioResult {
        let wave_samples = self.model.speak_one_sentence(phonemes)?;
        match self.output_config {
            Some(ref config) => config.apply(wave_samples),
            None => Ok(wave_samples),
        }
    }
    #[allow(dead_code)]
    fn process_batches(&self, phonemes: Vec<String>) -> DengjenResult<Vec<Audio>> {
        let wave_samples = self.model.speak_batch(phonemes)?;
        match self.output_config {
            Some(ref config) => {
                let mut processed: Vec<Audio> = Vec::with_capacity(wave_samples.len());
                for samples in wave_samples.into_iter() {
                    processed.push(config.apply(samples)?);
                }
                Ok(processed)
            }
            None => Ok(wave_samples),
        }
    }
}

pub struct DengjenSpeechStreamLazy {
    provider: SpeechSynthesisTaskProvider,
    sentence_phonemes: std::vec::IntoIter<String>,
}

impl DengjenSpeechStreamLazy {
    fn new(provider: SpeechSynthesisTaskProvider) -> DengjenResult<Self> {
        let sentence_phonemes = provider.get_phonemes()?.into_iter();
        Ok(Self {
            provider,
            sentence_phonemes,
        })
    }
}

impl Iterator for DengjenSpeechStreamLazy {
    type Item = DengjenAudioResult;

    fn next(&mut self) -> Option<Self::Item> {
        let phonemes = self.sentence_phonemes.next()?;
        match self.provider.process_one_sentence(phonemes) {
            Ok(ws) => Some(Ok(ws)),
            Err(e) => Some(Err(e)),
        }
    }
}

#[must_use]
pub struct DengjenSpeechStreamParallel {
    precalculated_results: std::vec::IntoIter<DengjenAudioResult>,
}

impl DengjenSpeechStreamParallel {
    fn new(provider: SpeechSynthesisTaskProvider) -> DengjenResult<Self> {
        let calculated_result: Vec<DengjenAudioResult> = provider
            .get_phonemes()?
            .par_iter()
            .map(|ph| provider.process_one_sentence(ph.to_string()))
            .collect();
        Ok(Self {
            precalculated_results: calculated_result.into_iter(),
        })
    }
}

impl Iterator for DengjenSpeechStreamParallel {
    type Item = DengjenAudioResult;

    fn next(&mut self) -> Option<Self::Item> {
        self.precalculated_results.next()
    }
}

pub struct RealtimeSpeechStream(Receiver<DengjenResult<AudioSamples>>);

impl RealtimeSpeechStream {
    fn new(
        provider: SpeechSynthesisTaskProvider,
        chunk_size: usize,
        chunk_padding: usize,
        sample_rate: usize,
        num_channels: usize,
        cancel_token: CancellationToken,
    ) -> DengjenResult<Self> {
        let phonemes = provider.get_phonemes()?.into_iter();
        let (tx, rx) = flume::unbounded();
        SYNTHESIS_THREAD_POOL.spawn(move || {
            let mut chunk_size = chunk_size;
            let chunk_factor = 1;
            let mut num_processed_chunks = 0;
            for ph_sent in phonemes {
                if cancel_token.is_cancelled() {
                    return;
                }
                chunk_size = if num_processed_chunks != 0 {
                    chunk_size  * chunk_factor * num_processed_chunks
                } else {
                    chunk_size
                };
                match provider
                    .model
                    .stream_synthesis(ph_sent, chunk_size, chunk_padding, cancel_token.clone())
                {
                    Ok(stream) => {
                        let send_result = RealtimeSpeechStream::process_rt_stream(
                            stream,
                            &tx,
                            provider.output_config.as_ref(),
                            sample_rate,
                            num_channels,
                            &cancel_token,
                        );
                        match send_result {
                            Ok(num_chunks) => num_processed_chunks += num_chunks,
                            Err(_) => return
                        };
                    }
                    Err(e) => {
                        tx.send(Err(e)).ok();
                        return;
                    }
                };
            }
        });
        Ok(Self(rx))
    }
    #[inline(always)]
    fn process_rt_stream(
        stream: AudioStreamIterator,
        tx: &Sender<DengjenResult<AudioSamples>>,
        audio_output_config: Option<&AudioOutputConfig>,
        sample_rate: usize,
        num_channels: usize,
        cancel_token: &CancellationToken,
    ) -> Result<usize, SendError<DengjenResult<AudioSamples>>> {
        let mut num_chunks = 0;
        if let Some(output_config) = audio_output_config {
            for result in stream {
                if cancel_token.is_cancelled() {
                    return Ok(num_chunks);
                }
                match result {
                    Ok(samples) => {
                        tx.send(output_config.apply_to_raw_samples(
                            samples,
                            sample_rate,
                            num_channels,
                        ))?;
                        num_chunks += 1;
                    }
                    Err(e) => {
                        tx.send(Err(e))?;
                    }
                };
            }
            if !cancel_token.is_cancelled() {
                if let Some(silence_ms) = output_config.appended_silence_ms {
                    let silence_result =
                        output_config.generate_silence(silence_ms as usize, sample_rate, num_channels);
                    tx.send(silence_result)?;
                }
            }
            Ok(num_chunks)
        } else {
            for result in stream {
                if cancel_token.is_cancelled() {
                    return Ok(num_chunks);
                }
                tx.send(result)?;
                num_chunks += 1;
            }
            Ok(num_chunks)
        }
    }
}

impl Iterator for RealtimeSpeechStream {
    type Item = DengjenResult<AudioSamples>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.recv().ok()
    }
}

#[cfg(test)]
mod cancellation_tests {
    use super::*;
    use std::any::Any;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingStreamModel {
        chunks_per_sentence: usize,
        sentences: usize,
        produced: Arc<AtomicUsize>,
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
        fn get_default_synthesis_config(&self) -> DengjenResult<Box<dyn Any>> {
            Ok(Box::new(()))
        }
        fn get_fallback_synthesis_config(&self) -> DengjenResult<Box<dyn Any>> {
            Ok(Box::new(()))
        }
        fn set_fallback_synthesis_config(&self, _c: &dyn Any) -> DengjenResult<()> {
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
            cancel_token: CancellationToken,
        ) -> DengjenResult<AudioStreamIterator<'_>> {
            let produced = Arc::clone(&self.produced);
            let n = self.chunks_per_sentence;
            let iter = (0..n).map_while(move |_| {
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
        });
        let synth = DengjenSpeechSynthesizer::new(model).unwrap();
        let cancel_token = CancellationToken::new();
        let stream = synth
            .synthesize_streamed("irrelevant".to_string(), None, 10, 0, cancel_token.clone())
            .unwrap();

        let mut received = 0;
        for result in stream {
            result.unwrap();
            received += 1;
            if received == 3 {
                cancel_token.cancel();
            }
        }

        let total_possible = 1000 * 5;
        assert!(
            received < total_possible,
            "expected cancellation to truncate the stream, got all {received} chunks"
        );
        assert!(produced.load(Ordering::SeqCst) < total_possible);
    }
}
