use crate::config::KokoroVoiceConfig;
use crate::phonemize::text_to_kokoro_phonemes;
use crate::voice_style::VoiceStyles;
use crate::vocab::Vocab;
use dengjen_core::{
    Audio, AudioInfo, DengjenAudioResult, DengjenError, DengjenModel, DengjenResult, Phonemes,
};
use ndarray::{Array1, Array2};
use ort::session::Session;
use ort::value::Tensor;
use std::any::Any;
use std::collections::HashMap;
use std::sync::Mutex;

pub struct KokoroModel {
    session: Mutex<Session>,
    vocab: Vocab,
    voice_styles: VoiceStyles,
    sample_rate: u32,
    voices: Vec<String>,
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
            voices: config.voices,
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
            .map_err(|e| DengjenError::OperationError(format!("Kokoro inference failed: {}", e)))?;
        let (_, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| DengjenError::OperationError(format!("Failed to extract Kokoro output: {}", e)))?;
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
        let phonemes = text_to_kokoro_phonemes(text, language)?;
        Ok(Phonemes::from(vec![phonemes]))
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

    fn get_default_synthesis_config(&self) -> DengjenResult<Box<dyn Any>> {
        Ok(Box::new(()))
    }

    fn get_fallback_synthesis_config(&self) -> DengjenResult<Box<dyn Any>> {
        Ok(Box::new(()))
    }

    fn set_fallback_synthesis_config(&self, _synthesis_config: &dyn Any) -> DengjenResult<()> {
        Ok(())
    }

    fn get_speakers(&self) -> DengjenResult<Option<&HashMap<i64, String>>> {
        Ok(None)
    }
}
