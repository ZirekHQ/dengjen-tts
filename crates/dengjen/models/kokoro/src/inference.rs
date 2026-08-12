use crate::config::KokoroVoiceConfig;
use crate::phonemize::text_to_kokoro_phonemes;
use crate::voice_style::VoiceStyles;
use crate::vocab::Vocab;
use dengjen_core::{
    Audio, AudioInfo, AudioSamples, AudioStreamIterator, CancellationToken, DengjenAudioResult,
    DengjenError, DengjenModel, DengjenResult, Phonemes, SynthesisConfig,
};
use ndarray::{Array1, Array2};
use ort::session::Session;
use ort::value::Tensor;
use std::collections::HashMap;
use std::sync::Mutex;

// `chunk_size` on `DengjenModel::stream_synthesis` is tuned by callers for Piper, where it
// counts mel frames that Piper's decoder expands by `hop_length` (~256) into samples. Kokoro
// has no mel/frame stage, so its `chunk_size` is scaled by the same nominal factor to land in
// the same practical chunk-duration range as Piper's callers expect, rather than being
// consumed directly as a far-too-small raw sample count.
const KOKORO_CHUNK_SIZE_SCALE: usize = 256;

pub struct KokoroModel {
    session: Mutex<Session>,
    vocab: Vocab,
    voice_styles: VoiceStyles,
    sample_rate: u32,
    default_voice: String,
}

impl KokoroModel {
    pub fn from_config(config: KokoroVoiceConfig) -> DengjenResult<Self> {
        let session = Session::builder()
            .map_err(|e| DengjenError::FailedToLoadResource(e.to_string()))?
            .commit_from_file(&config.model_path)
            .map_err(|e| {
                DengjenError::FailedToLoadResource(format!(
                    "Failed to load Kokoro ONNX model at `{}`: {}",
                    config.model_path.display(),
                    e
                ))
            })?;
        let vocab = Vocab::load(&config.vocab_path)?;
        let voice_styles = VoiceStyles::load(&config.voices_dir, &config.voices)?;
        let default_voice = config
            .voices
            .first()
            .cloned()
            .ok_or_else(|| DengjenError::FailedToLoadResource("No voices in config".to_string()))?;
        Ok(Self {
            session: Mutex::new(session),
            vocab,
            voice_styles,
            sample_rate: config.sample_rate,
            default_voice,
        })
    }

    fn synthesize_phonemes(&self, phonemes: &str) -> DengjenAudioResult {
        let mut token_ids = vec![self.vocab.bos_id()];
        token_ids.extend(self.vocab.tokenize(phonemes));
        token_ids.push(self.vocab.eos_id());

        let input_ids = Array2::from_shape_vec((1, token_ids.len()), token_ids.clone())
            .map_err(|e| DengjenError::with_message(e.to_string()))?;
        let style = self
            .voice_styles
            .style_for(&self.default_voice, token_ids.len())?;
        let speed = Array1::from_vec(vec![1.0f32]);

        let mut session = self.session.lock().unwrap();
        let outputs = session
            .run(ort::inputs![
                Tensor::from_array(input_ids).map_err(|e| DengjenError::with_message(e.to_string()))?,
                Tensor::from_array(style).map_err(|e| DengjenError::with_message(e.to_string()))?,
                Tensor::from_array(speed).map_err(|e| DengjenError::with_message(e.to_string()))?,
            ])
            .map_err(|e| DengjenError::InferenceError(format!("Kokoro inference failed: {}", e)))?;
        let (_, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| DengjenError::InferenceError(format!("Failed to extract Kokoro output: {}", e)))?;
        Ok(Audio::new(data.to_vec().into(), self.sample_rate as usize, None))
    }
}

impl DengjenModel for KokoroModel {
    fn audio_output_info(&self) -> DengjenResult<AudioInfo> {
        Ok(AudioInfo {
            sample_rate: self.sample_rate as usize,
            num_channels: 1,
            sample_width: 2,
        })
    }

    fn phonemize_text(&self, text: &str) -> DengjenResult<Phonemes> {
        let language = "en-US"; // Task 6 revisits per-voice language selection if needed.
        let sentences = text_to_kokoro_phonemes(text, language)?;
        Ok(Phonemes::from(sentences))
    }

    fn speak_batch(&self, phoneme_batches: Vec<String>) -> DengjenResult<Vec<Audio>> {
        phoneme_batches
            .into_iter()
            .map(|p| self.synthesize_phonemes(&p))
            .collect()
    }

    fn speak_one_sentence(&self, phonemes: String) -> DengjenAudioResult {
        self.synthesize_phonemes(&phonemes)
    }

    fn get_default_synthesis_config(&self) -> DengjenResult<SynthesisConfig> {
        Ok(SynthesisConfig::None)
    }

    fn get_fallback_synthesis_config(&self) -> DengjenResult<SynthesisConfig> {
        Ok(SynthesisConfig::None)
    }

    fn set_fallback_synthesis_config(&self, _synthesis_config: &SynthesisConfig) -> DengjenResult<()> {
        Ok(())
    }

    fn get_speakers(&self) -> DengjenResult<Option<&HashMap<i64, String>>> {
        Ok(None)
    }

    fn supports_streaming_output(&self) -> bool {
        true
    }

    fn stream_synthesis(
        &self,
        phonemes: String,
        chunk_size: usize,
        _chunk_padding: usize,
        cancel_token: CancellationToken,
    ) -> DengjenResult<AudioStreamIterator<'_>> {
        if chunk_size == 0 {
            return Err(DengjenError::UnsupportedOperation(
                "chunk_size must be greater than 0".to_string(),
            ));
        }
        // Skip inference entirely rather than erroring: a cancellation must stay silent.
        if cancel_token.is_cancelled() {
            return Ok(Box::new(std::iter::empty()));
        }
        let audio = self.synthesize_phonemes(&phonemes)?;
        let scaled_chunk_size = chunk_size.saturating_mul(KOKORO_CHUNK_SIZE_SCALE);
        Ok(Box::new(KokoroAudioStreamer::new(
            audio.samples,
            scaled_chunk_size,
            cancel_token,
        )))
    }
}

struct KokoroAudioStreamer {
    samples: AudioSamples,
    cursor: usize,
    chunk_size: usize,
    cancel_token: CancellationToken,
}

impl KokoroAudioStreamer {
    fn new(samples: AudioSamples, chunk_size: usize, cancel_token: CancellationToken) -> Self {
        Self {
            samples,
            cursor: 0,
            chunk_size,
            cancel_token,
        }
    }
}

impl Iterator for KokoroAudioStreamer {
    type Item = DengjenResult<AudioSamples>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cancel_token.is_cancelled() {
            return None;
        }
        if self.chunk_size == 0 || self.cursor >= self.samples.len() {
            return None;
        }
        let end = (self.cursor + self.chunk_size).min(self.samples.len());
        let chunk = self.samples.as_slice()[self.cursor..end].to_vec();
        self.cursor = end;
        Some(Ok(AudioSamples::new(chunk)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yields_equal_size_chunks_when_length_is_an_exact_multiple() {
        let samples = AudioSamples::new(vec![0.0; 9]);
        let mut streamer = KokoroAudioStreamer::new(samples, 3, CancellationToken::new());
        assert_eq!(streamer.next().unwrap().unwrap().len(), 3);
        assert_eq!(streamer.next().unwrap().unwrap().len(), 3);
        assert_eq!(streamer.next().unwrap().unwrap().len(), 3);
        assert!(streamer.next().is_none());
    }

    #[test]
    fn yields_a_shorter_final_chunk_for_a_remainder() {
        let samples = AudioSamples::new(vec![0.0; 7]);
        let mut streamer = KokoroAudioStreamer::new(samples, 3, CancellationToken::new());
        assert_eq!(streamer.next().unwrap().unwrap().len(), 3);
        assert_eq!(streamer.next().unwrap().unwrap().len(), 3);
        assert_eq!(streamer.next().unwrap().unwrap().len(), 1);
        assert!(streamer.next().is_none());
    }

    #[test]
    fn chunk_values_preserve_sample_order() {
        let samples = AudioSamples::new(vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        let mut streamer = KokoroAudioStreamer::new(samples, 2, CancellationToken::new());
        assert_eq!(streamer.next().unwrap().unwrap().as_slice(), &[1.0, 2.0]);
        assert_eq!(streamer.next().unwrap().unwrap().as_slice(), &[3.0, 4.0]);
        assert_eq!(streamer.next().unwrap().unwrap().as_slice(), &[5.0]);
        assert!(streamer.next().is_none());
    }

    #[test]
    fn stops_yielding_once_cancelled_mid_stream() {
        let samples = AudioSamples::new(vec![0.0; 9]);
        let cancel_token = CancellationToken::new();
        let mut streamer = KokoroAudioStreamer::new(samples, 3, cancel_token.clone());
        assert!(streamer.next().is_some());
        cancel_token.cancel();
        assert!(streamer.next().is_none(), "no further chunks should be yielded once cancelled");
    }

    #[test]
    fn empty_input_yields_no_chunks() {
        let samples = AudioSamples::new(Vec::new());
        let mut streamer = KokoroAudioStreamer::new(samples, 3, CancellationToken::new());
        assert!(streamer.next().is_none());
    }

    #[test]
    fn chunk_size_zero_yields_no_chunks() {
        let samples = AudioSamples::new(vec![0.0; 9]);
        let mut streamer = KokoroAudioStreamer::new(samples, 0, CancellationToken::new());
        assert!(streamer.next().is_none());
        assert!(streamer.next().is_none(), "should remain exhausted after first call");
    }
}
