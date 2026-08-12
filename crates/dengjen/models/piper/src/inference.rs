use crate::config::{load_model_config, ModelConfig};
use crate::phonemize::{create_tashkeel_engine, TashkeelEngine};
use crate::VitsModelCommons;
use dengjen_core::{
    Audio, AudioInfo, DengjenAudioResult, DengjenError, DengjenModel, DengjenResult, Phonemes,
    PiperSynthesisConfig, SynthesisConfig,
};
use ndarray::{Array1, Array2};
use ort::session::Session;
use ort::value::Tensor;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};

#[inline(always)]
pub(crate) fn reversed_mapping<K, V>(input: &HashMap<K, V>) -> HashMap<V, K>
where
    K: ToOwned<Owned = K>,
    V: ToOwned<Owned = V> + std::hash::Hash + std::cmp::Eq,
{
    HashMap::from_iter(input.iter().map(|(k, v)| (v.to_owned(), k.to_owned())))
}

pub(crate) fn create_inference_session(model_path: &Path) -> Result<Session, ort::Error> {
    Session::builder()?
        // .with_parallel_execution(true)?
        // .with_inter_threads(16)?
        // .with_optimization_level(ort::GraphOptimizationLevel::Level3)?
        // .with_memory_pattern(false)?
        .commit_from_file(model_path)
}

pub struct VitsModel {
    synth_config: RwLock<PiperSynthesisConfig>,
    config: ModelConfig,
    speaker_map: HashMap<i64, String>,
    session: Mutex<Session>,
    #[cfg_attr(not(all(feature = "tashkeel", feature = "espeak")), allow(dead_code))]
    tashkeel_engine: Option<TashkeelEngine>,
}

impl VitsModel {
    pub fn new(config_path: PathBuf, onnx_path: &Path) -> DengjenResult<Self> {
        match load_model_config(&config_path) {
            Ok((config, synth_config)) => Self::from_config(config, synth_config, onnx_path),
            Err(error) => Err(error),
        }
    }
    pub(crate) fn from_config(
        config: ModelConfig,
        synth_config: PiperSynthesisConfig,
        onnx_path: &Path,
    ) -> DengjenResult<Self> {
        let session = match create_inference_session(onnx_path) {
            Ok(session) => session,
            Err(err) => {
                return Err(DengjenError::InferenceError(format!(
                    "Failed to initialize onnxruntime inference session: `{}`",
                    err
                )))
            }
        };
        let speaker_map = reversed_mapping(&config.speaker_id_map);
        let tashkeel_engine = create_tashkeel_engine(&config)?;
        Ok(Self {
            synth_config: RwLock::new(synth_config),
            config,
            speaker_map,
            session: Mutex::new(session),
            tashkeel_engine,
        })
    }
    fn infer_with_values(&self, input_phonemes: Vec<i64>) -> DengjenAudioResult {
        let synth_config = self.synth_config.read().unwrap();

        let input_len = input_phonemes.len();
        let phoneme_inputs = Array2::<i64>::from_shape_vec((1, input_len), input_phonemes).unwrap();
        let input_lengths = Array1::<i64>::from_iter([input_len as i64]);
        let scales = Array1::<f32>::from_iter([
            synth_config.noise_scale,
            synth_config.length_scale,
            synth_config.noise_w,
        ]);
        let speaker_id = if self.config.num_speakers > 1 {
            let sid = synth_config.speaker.unwrap_or(0);
            Some(Array1::<i64>::from_iter([sid]))
        } else {
            None
        };

        let mut session = self.session.lock().unwrap();
        let timer = std::time::Instant::now();
        let outputs = {
            let outputs = if let Some(sid_tensor) = speaker_id.clone() {
                let inputs = ort::inputs![
                    Tensor::from_array(phoneme_inputs).unwrap(),
                    Tensor::from_array(input_lengths).unwrap(),
                    Tensor::from_array(scales).unwrap(),
                    Tensor::from_array(sid_tensor).unwrap(),
                ];
                session.run(inputs)
            } else {
                let inputs = ort::inputs![
                    Tensor::from_array(phoneme_inputs).unwrap(),
                    Tensor::from_array(input_lengths).unwrap(),
                    Tensor::from_array(scales).unwrap(),
                ];
                session.run(inputs)
            };
            match outputs {
                Ok(out) => out,
                Err(e) => {
                    return Err(DengjenError::InferenceError(format!(
                        "Failed to run model inference. Error: {}",
                        e
                    )))
                }
            }
        };
        let inference_ms = timer.elapsed().as_millis() as f32;

        let (_, outputs) = match outputs[0].try_extract_tensor::<f32>() {
            Ok(out) => out,
            Err(e) => {
                return Err(DengjenError::InferenceError(format!(
                    "Failed to run model inference. Error: {}",
                    e
                )))
            }
        };

        let audio = Vec::from(outputs);

        Ok(Audio::new(
            audio.into(),
            self.config.audio.sample_rate as usize,
            Some(inference_ms),
        ))
    }
    pub fn get_input_output_info(&self) -> DengjenResult<Vec<String>> {
        todo!()
    }
}

impl VitsModelCommons for VitsModel {
    fn get_synth_config(&self) -> &RwLock<PiperSynthesisConfig> {
        &self.synth_config
    }
    fn get_config(&self) -> &ModelConfig {
        &self.config
    }
    fn get_speaker_map(&self) -> &HashMap<i64, String> {
        &self.speaker_map
    }
    fn get_tashkeel_engine(&self) -> Option<&TashkeelEngine> {
        self.tashkeel_engine.as_ref()
    }
}

impl DengjenModel for VitsModel {
    fn phonemize_text(&self, text: &str) -> DengjenResult<Phonemes> {
        self.do_phonemize_text(text)
    }

    fn speak_batch(&self, phoneme_batches: Vec<String>) -> DengjenResult<Vec<Audio>> {
        let (pad_id, bos_id, eos_id) = self.get_meta_ids();
        let phoneme_batches = Vec::from_iter(
            phoneme_batches
                .into_iter()
                .map(|phonemes| self.phonemes_to_input_ids(&phonemes, pad_id, bos_id, eos_id)),
        );
        let mut retval = Vec::new();
        for phonemes in phoneme_batches.into_iter() {
            retval.push(self.infer_with_values(phonemes)?);
        }
        Ok(retval)
    }

    fn speak_one_sentence(&self, phonemes: String) -> DengjenAudioResult {
        let (pad_id, bos_id, eos_id) = self.get_meta_ids();
        let phonemes = self.phonemes_to_input_ids(&phonemes, pad_id, bos_id, eos_id);
        self.infer_with_values(phonemes)
    }
    fn get_default_synthesis_config(&self) -> DengjenResult<SynthesisConfig> {
        Ok(SynthesisConfig::Piper(self.factory_synthesis_config()))
    }
    fn get_fallback_synthesis_config(&self) -> DengjenResult<SynthesisConfig> {
        Ok(SynthesisConfig::Piper(self.synth_config.read().unwrap().clone()))
    }
    fn set_fallback_synthesis_config(&self, synthesis_config: &SynthesisConfig) -> DengjenResult<()> {
        match synthesis_config {
            SynthesisConfig::Piper(new_config) => self._do_set_default_synth_config(new_config),
            SynthesisConfig::None => Err(DengjenError::InvalidConfiguration(
                "Piper models require a PiperSynthesisConfig".to_string(),
            )),
        }
    }
    fn get_language(&self) -> DengjenResult<Option<String>> {
        Ok(self.language())
    }
    fn get_speakers(&self) -> DengjenResult<Option<&HashMap<i64, String>>> {
        Ok(Some(self.get_speaker_map()))
    }
    fn speaker_name_to_id(&self, name: &str) -> DengjenResult<Option<i64>> {
        Ok(self.config.speaker_id_map.get(name).copied())
    }
    fn properties(&self) -> DengjenResult<HashMap<String, String>> {
        Ok(self.get_properties())
    }
    fn audio_output_info(&self) -> DengjenResult<AudioInfo> {
        self.get_audio_output_info()
    }
}
