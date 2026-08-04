use espeak_phonemizer::text_to_phonemes;
#[cfg(feature = "tashkeel")]
use libtashkeel_core::do_tashkeel;
use ndarray::{Array, Array1, Array2, ArrayView, Axis, Dim, IxDynImpl};
use ort::session::Session;
use ort::session::output::SessionOutputs;
use serde::Deserialize;
use dengjen_core::{
    Audio, AudioInfo, AudioSamples, AudioStreamIterator, Phonemes, DengjenAudioResult, DengjenError,
    DengjenModel, DengjenResult,
};
use std::any::Any;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

const MIN_CHUNK_SIZE: isize = 44;
const MAX_CHUNK_SIZE: usize = 1024;
const BOS: &str = "^";
const EOS: &str = "$";
const PAD: &str = "_";

#[cfg(feature = "tashkeel")]
type TashkeelEngine = libtashkeel_core::DynamicInferenceEngine;
#[cfg(not(feature = "tashkeel"))]
type TashkeelEngine = ();

#[cfg(feature = "tashkeel")]
fn should_diacritize(voice: &str) -> bool {
    voice == "ar"
}
#[cfg(not(feature = "tashkeel"))]
fn should_diacritize(_voice: &str) -> bool {
    false
}

#[inline(always)]
fn reversed_mapping<K, V>(input: &HashMap<K, V>) -> HashMap<V, K>
where
    K: ToOwned<Owned = K>,
    V: ToOwned<Owned = V> + std::hash::Hash + std::cmp::Eq,
{
    HashMap::from_iter(input.iter().map(|(k, v)| (v.to_owned(), k.to_owned())))
}

fn load_model_config(config_path: &Path) -> DengjenResult<(ModelConfig, PiperSynthesisConfig)> {
    let file = match File::open(config_path) {
        Ok(file) => file,
        Err(why) => {
            return Err(DengjenError::FailedToLoadResource(format!(
                "Faild to load model config: `{}`. Caused by: `{}`",
                config_path.display(),
                why
            )))
        }
    };
    let model_config: ModelConfig = match serde_json::from_reader(file) {
        Ok(config) => config,
        Err(why) => {
            return Err(DengjenError::FailedToLoadResource(format!(
                "Faild to parse model config from file: `{}`. Caused by: `{}`",
                config_path.display(),
                why
            )))
        }
    };
    let synth_config = PiperSynthesisConfig {
        speaker: None,
        noise_scale: model_config.inference.noise_scale,
        length_scale: model_config.inference.length_scale,
        noise_w: model_config.inference.noise_w,
    };
    Ok((model_config, synth_config))
}

fn map_phonemes_to_ids(
    phoneme_id_map: &HashMap<String, Vec<i64>>,
    phonemes: &str,
    pad_id: i64,
    bos_id: i64,
    eos_id: i64,
) -> Vec<i64> {
    let max_cluster_len = phoneme_id_map
        .keys()
        .map(|k| k.chars().count())
        .max()
        .unwrap_or(1)
        .max(1);
    let chars: Vec<char> = phonemes.chars().collect();
    let mut phoneme_ids: Vec<i64> = Vec::with_capacity((chars.len() + 1) * 2);
    phoneme_ids.push(bos_id);
    let mut i = 0;
    while i < chars.len() {
        let max_len = max_cluster_len.min(chars.len() - i);
        let matched_len = (1..=max_len).rev().find(|&len| {
            let candidate: String = chars[i..i + len].iter().collect();
            phoneme_id_map.contains_key(&candidate)
        });
        match matched_len {
            Some(len) => {
                let candidate: String = chars[i..i + len].iter().collect();
                let id = *phoneme_id_map.get(&candidate).unwrap().first().unwrap();
                phoneme_ids.push(id);
                phoneme_ids.push(pad_id);
                i += len;
            }
            None => i += 1,
        }
    }
    phoneme_ids.push(eos_id);
    phoneme_ids
}

fn resolve_default_speaker_id(config: &ModelConfig) -> i64 {
    config.default_speaker_id.unwrap_or(0)
}

// Returns `None` when the caller should fall through to the espeak-based phonemization
// path; `Some(_)` when this phoneme_type is fully handled here.
fn phonemize_dispatch(phoneme_type: PhonemeType, text: &str) -> Option<DengjenResult<Phonemes>> {
    match phoneme_type {
        PhonemeType::Espeak => None,
        PhonemeType::Text => Some(Ok(vec![text.to_string()].into())),
        other => Some(Err(DengjenError::PhonemizationError(format!(
            "Phonemization for phoneme_type `{:?}` is not yet supported",
            other
        )))),
    }
}

#[cfg(feature = "tashkeel")]
fn create_tashkeel_engine(config: &ModelConfig) -> DengjenResult<Option<TashkeelEngine>> {
    if should_diacritize(&config.espeak.voice) {
        match libtashkeel_core::create_inference_engine(None) {
            Ok(engine) => Ok(Some(engine)),
            Err(msg) => Err(DengjenError::OperationError(format!(
                "Failed to create inference engine for libtashkeel. {}",
                msg
            ))),
        }
    } else {
        Ok(None)
    }
}
#[cfg(not(feature = "tashkeel"))]
fn create_tashkeel_engine(_config: &ModelConfig) -> DengjenResult<Option<TashkeelEngine>> {
    Ok(None)
}

fn create_inference_session(model_path: &Path) -> Result<Session, ort::Error> {
    Session::builder()?
        // .with_parallel_execution(true)?
        // .with_inter_threads(16)?
        // .with_optimization_level(ort::GraphOptimizationLevel::Level3)?
        // .with_memory_pattern(false)?
        .commit_from_file(model_path)
}

pub fn from_config_path(config_path: &Path) -> DengjenResult<Arc<dyn DengjenModel + Send + Sync>> {
    let (config, synth_config) = load_model_config(config_path)?;
    if config.streaming.unwrap_or_default() {
        Ok(Arc::new(VitsStreamingModel::from_config(
            config,
            synth_config,
            &config_path.with_file_name("encoder.onnx"),
            &config_path.with_file_name("decoder.onnx"),
        )?))
    } else {
        let Some(onnx_filename) = config_path.file_stem() else {
            return Err(DengjenError::OperationError(format!(
                "Invalid config filename format `{}`",
                config_path.display()
            )));
        };
        Ok(Arc::new(VitsModel::from_config(
            config,
            synth_config,
            &config_path.with_file_name(onnx_filename),
        )?))
    }
}

#[derive(Deserialize, Default)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub quality: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct ESpeakConfig {
    voice: String,
}

#[derive(Deserialize, Default, Clone)]
pub struct InferenceConfig {
    noise_scale: f32,
    length_scale: f32,
    noise_w: f32,
}

#[derive(Clone, Deserialize, Default)]
pub struct Language {
    code: String,
    #[allow(dead_code)]
    family: Option<String>,
    #[allow(dead_code)]
    region: Option<String>,
    #[allow(dead_code)]
    name_native: Option<String>,
    #[allow(dead_code)]
    name_english: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PhonemeType {
    #[default]
    Espeak,
    Text,
    Pinyin,
    Hebrew,
}

#[derive(Deserialize, Default)]
pub struct ModelConfig {
    pub key: Option<String>,
    pub language: Option<Language>,
    pub audio: AudioConfig,
    pub num_speakers: u32,
    pub speaker_id_map: HashMap<String, i64>,
    streaming: Option<bool>,
    espeak: ESpeakConfig,
    inference: InferenceConfig,
    #[allow(dead_code)]
    num_symbols: u32,
    #[allow(dead_code)]
    phoneme_map: HashMap<i64, String>,
    phoneme_id_map: HashMap<String, Vec<i64>>,
    phoneme_type: Option<PhonemeType>,
    default_speaker_id: Option<i64>,
    hop_length: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct PiperSynthesisConfig {
    pub speaker: Option<i64>,
    pub noise_scale: f32,
    pub length_scale: f32,
    pub noise_w: f32,
}

trait VitsModelCommons {
    fn get_synth_config(&self) -> &RwLock<PiperSynthesisConfig>;
    fn get_config(&self) -> &ModelConfig;
    fn get_speaker_map(&self) -> &HashMap<i64, String>;
    #[cfg_attr(not(feature = "tashkeel"), allow(dead_code))]
    fn get_tashkeel_engine(&self) -> Option<&TashkeelEngine>;
    fn get_meta_ids(&self) -> (i64, i64, i64) {
        let config = self.get_config();
        let pad_id = *config.phoneme_id_map.get(PAD).unwrap().first().unwrap();
        let bos_id = *config.phoneme_id_map.get(BOS).unwrap().first().unwrap();
        let eos_id = *config.phoneme_id_map.get(EOS).unwrap().first().unwrap();
        (pad_id, bos_id, eos_id)
    }
    fn language(&self) -> Option<String> {
        self.get_config()
            .language
            .as_ref()
            .map(|lang| lang.code.clone())
            .or_else(|| Some(self.get_config().espeak.voice.clone()))
    }
    fn get_properties(&self) -> HashMap<String, String> {
        HashMap::from([(
            "quality".to_string(),
            self.get_config()
                .audio
                .quality
                .clone()
                .unwrap_or("unknown".to_string()),
        )])
    }
    // Unused: get_default_synthesis_config below duplicates this logic inline
    // instead of calling it. See issue #1 (Piper config format drift) before
    // consolidating, since the two aren't quite equivalent (this checks
    // num_speakers > 0 before defaulting to speaker 0; the inline version
    // doesn't).
    #[allow(dead_code)]
    fn factory_synthesis_config(&self) -> PiperSynthesisConfig {
        let config = self.get_config();

        let speaker = if config.num_speakers > 0 {
            Some(resolve_default_speaker_id(config))
        } else {
            None
        };
        PiperSynthesisConfig {
            speaker,
            length_scale: config.inference.length_scale,
            noise_scale: config.inference.noise_scale,
            noise_w: config.inference.noise_w,
        }
    }
    // Unused: get_speakers below duplicates this logic inline (returning a
    // reference instead of a clone). See the note on factory_synthesis_config.
    #[allow(dead_code)]
    fn speakers(&self) -> DengjenResult<HashMap<i64, String>> {
        Ok(self.get_speaker_map().clone())
    }
    fn _do_set_default_synth_config(&self, new_config: &PiperSynthesisConfig) -> DengjenResult<()> {
        let mut synth_config = self.get_synth_config().write().unwrap();
        synth_config.length_scale = new_config.length_scale;
        synth_config.noise_scale = new_config.noise_scale;
        synth_config.noise_w = new_config.noise_w;
        if let Some(sid) = new_config.speaker {
            if self.get_speaker_map().contains_key(&sid) {
                synth_config.speaker = Some(sid);
            } else {
                return Err(DengjenError::OperationError(format!(
                    "No speaker was found with the given id `{}`",
                    sid
                )));
            }
        }
        Ok(())
    }
    fn phonemes_to_input_ids(
        &self,
        phonemes: &str,
        pad_id: i64,
        bos_id: i64,
        eos_id: i64,
    ) -> Vec<i64> {
        map_phonemes_to_ids(&self.get_config().phoneme_id_map, phonemes, pad_id, bos_id, eos_id)
    }
    fn do_phonemize_text(&self, text: &str) -> DengjenResult<Phonemes> {
        let config = self.get_config();
        if let Some(result) = phonemize_dispatch(config.phoneme_type.unwrap_or_default(), text) {
            return result;
        }
        let text = if should_diacritize(&config.espeak.voice) {
            let diacritized = self.diacritize_text(text)?;
            Cow::from(diacritized)
        } else {
            Cow::from(text)
        };
        let phonemes = match text_to_phonemes(&text, &config.espeak.voice, None, true, false) {
            Ok(ph) => ph,
            Err(e) => {
                return Err(DengjenError::PhonemizationError(format!(
                    "Failed to phonemize given text using espeak-ng. Error: {}",
                    e
                )))
            }
        };
        Ok(phonemes.into())
    }
    #[cfg(feature = "tashkeel")]
    fn diacritize_text(&self, text: &str) -> DengjenResult<String> {
        match do_tashkeel(self.get_tashkeel_engine().unwrap(), text, None, false) {
            Ok(diacritized_text) => Ok(diacritized_text),
            Err(msg) => Err(DengjenError::OperationError(format!(
                "Failed to diacritize text using  libtashkeel. {}",
                msg
            ))),
        }
    }
    // should_diacritize() is always false without this feature, so this is unreachable.
    #[cfg(not(feature = "tashkeel"))]
    fn diacritize_text(&self, _text: &str) -> DengjenResult<String> {
        unreachable!("diacritize_text called with the `tashkeel` feature disabled")
    }
    fn get_audio_output_info(&self) -> DengjenResult<AudioInfo> {
        Ok(AudioInfo {
            sample_rate: self.get_config().audio.sample_rate as usize,
            num_channels: 1usize,
            sample_width: 2usize,
        })
    }
}

pub struct VitsModel {
    synth_config: RwLock<PiperSynthesisConfig>,
    config: ModelConfig,
    speaker_map: HashMap<i64, String>,
    session: Session,
    #[cfg_attr(not(feature = "tashkeel"), allow(dead_code))]
    tashkeel_engine: Option<TashkeelEngine>,
}

impl VitsModel {
    pub fn new(config_path: PathBuf, onnx_path: &Path) -> DengjenResult<Self> {
        match load_model_config(&config_path) {
            Ok((config, synth_config)) => Self::from_config(config, synth_config, onnx_path),
            Err(error) => Err(error),
        }
    }
    fn from_config(
        config: ModelConfig,
        synth_config: PiperSynthesisConfig,
        onnx_path: &Path,
    ) -> DengjenResult<Self> {
        let session = match create_inference_session(onnx_path) {
            Ok(session) => session,
            Err(err) => {
                return Err(DengjenError::OperationError(format!(
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
            session,
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

        let session = &self.session;
        let timer = std::time::Instant::now();
        let outputs = {
            let outputs = if let Some(sid_tensor) = speaker_id.clone() {
                let inputs = ort::inputs![phoneme_inputs, input_lengths, scales, sid_tensor].unwrap();
                session.run(inputs)
            } else {
                let inputs = ort::inputs![phoneme_inputs, input_lengths, scales].unwrap();
                session.run(inputs)
            };
            match outputs {
                Ok(out) => out,
                Err(e) => {
                    return Err(DengjenError::OperationError(format!(
                        "Failed to run model inference. Error: {}",
                        e
                    )))
                }
            }
        };
        let inference_ms = timer.elapsed().as_millis() as f32;

        let outputs = match outputs[0].try_extract_tensor::<f32>() {
            Ok(out) => out,
            Err(e) => {
                return Err(DengjenError::OperationError(format!(
                    "Failed to run model inference. Error: {}",
                    e
                )))
            }
        };

        let audio = Vec::from(outputs.view().as_slice().unwrap());

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
    fn get_default_synthesis_config(&self) -> DengjenResult<Box<dyn Any>> {
        Ok(Box::new(PiperSynthesisConfig {
            speaker: Some(resolve_default_speaker_id(&self.config)),
            noise_scale: self.config.inference.noise_scale,
            noise_w: self.config.inference.noise_w,
            length_scale: self.config.inference.length_scale,
        }))
    }
    fn get_fallback_synthesis_config(&self) -> DengjenResult<Box<dyn Any>> {
        Ok(Box::new(self.synth_config.read().unwrap().clone()))
    }
    fn set_fallback_synthesis_config(&self, synthesis_config: &dyn Any) -> DengjenResult<()> {
        match synthesis_config.downcast_ref::<PiperSynthesisConfig>() {
            Some(new_config) => self._do_set_default_synth_config(new_config),
            None => Err(DengjenError::OperationError(
                "Invalid configuration for Vits Model".to_string(),
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

pub struct VitsStreamingModel {
    synth_config: RwLock<PiperSynthesisConfig>,
    config: ModelConfig,
    speaker_map: HashMap<i64, String>,
    encoder_model: Session,
    decoder_model: Arc<Session>,
    #[cfg_attr(not(feature = "tashkeel"), allow(dead_code))]
    tashkeel_engine: Option<TashkeelEngine>,
}

impl VitsStreamingModel {
    fn from_config(
        config: ModelConfig,
        synth_config: PiperSynthesisConfig,
        encoder_path: &Path,
        decoder_path: &Path,
    ) -> DengjenResult<Self> {
        let encoder_model = match create_inference_session(encoder_path) {
            Ok(model) => model,
            Err(err) => {
                return Err(DengjenError::OperationError(format!(
                    "Failed to initialize onnxruntime inference session: `{}`",
                    err
                )))
            }
        };
        let decoder_model = match create_inference_session(decoder_path) {
            Ok(model) => Arc::new(model),
            Err(err) => {
                return Err(DengjenError::OperationError(format!(
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
            encoder_model,
            decoder_model,
            tashkeel_engine,
        })
    }

    fn infer_with_values(&self, input_phonemes: Vec<i64>) -> DengjenAudioResult {
        let timer = std::time::Instant::now();
        let encoder_output = self.infer_encoder(input_phonemes)?;
        let audio = encoder_output.infer_decoder(self.decoder_model.as_ref())?;
        let inference_ms = timer.elapsed().as_millis() as f32;
        Ok(Audio::new(
            audio,
            self.config.audio.sample_rate as usize,
            Some(inference_ms),
        ))
    }
    fn infer_encoder(&self, input_phonemes: Vec<i64>) -> DengjenResult<EncoderOutputs> {
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

        let session = &self.encoder_model;
        {
            let outputs = if let Some(sid_tensor) = speaker_id.clone() {
                let inputs = ort::inputs![phoneme_inputs, input_lengths, scales, sid_tensor].unwrap();
                session.run(inputs)
            } else {
                let inputs = ort::inputs![phoneme_inputs, input_lengths, scales].unwrap();
                session.run(inputs)
            };
            match outputs {
                Ok(ort_values) => EncoderOutputs::from_values(ort_values),
                Err(e) => Err(DengjenError::OperationError(format!(
                    "Failed to run model inference. Error: {}",
                    e
                ))),
            }
        }
    }
}

impl VitsModelCommons for VitsStreamingModel {
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

impl DengjenModel for VitsStreamingModel {
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
    fn get_default_synthesis_config(&self) -> DengjenResult<Box<dyn Any>> {
        Ok(Box::new(PiperSynthesisConfig {
            speaker: Some(resolve_default_speaker_id(&self.config)),
            noise_scale: self.config.inference.noise_scale,
            noise_w: self.config.inference.noise_w,
            length_scale: self.config.inference.length_scale,
        }))
    }
    fn get_fallback_synthesis_config(&self) -> DengjenResult<Box<dyn Any>> {
        Ok(Box::new(self.synth_config.read().unwrap().clone()))
    }
    fn set_fallback_synthesis_config(&self, synthesis_config: &dyn Any) -> DengjenResult<()> {
        match synthesis_config.downcast_ref::<PiperSynthesisConfig>() {
            Some(new_config) => self._do_set_default_synth_config(new_config),
            None => Err(DengjenError::OperationError(
                "Invalid configuration for Vits Model".to_string(),
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
    fn supports_streaming_output(&self) -> bool {
        true
    }
    fn stream_synthesis(
        &self,
        phonemes: String,
        chunk_size: usize,
        chunk_padding: usize,
    ) -> DengjenResult<AudioStreamIterator<'_>> {
        let (pad_id, bos_id, eos_id) = self.get_meta_ids();
        let phonemes = self.phonemes_to_input_ids(&phonemes, pad_id, bos_id, eos_id);
        let encoder_outputs = self.infer_encoder(phonemes)?;
        let streamer = Box::new(SpeechStreamer::new(
            Arc::clone(&self.decoder_model),
            encoder_outputs,
            chunk_size,
            chunk_padding,
            self.config.hop_length.unwrap_or(256),
        ));
        Ok(streamer)
    }
}

struct EncoderOutputs {
    z: Array<f32, Dim<IxDynImpl>>,
    y_mask: Array<f32, Dim<IxDynImpl>>,
    #[allow(dead_code)]
    p_duration: Option<Array<f32, Dim<IxDynImpl>>>,
    g: Array<f32, Dim<IxDynImpl>>,
}

impl EncoderOutputs {
    #[inline(always)]
    fn from_values(values: SessionOutputs) -> DengjenResult<Self> {
        let z = {
            let z_t = match values["z"].try_extract_tensor::<f32>() {
                Ok(out) => out,
                Err(e) => {
                    return Err(DengjenError::OperationError(format!(
                        "Failed to run model inference. Error: {}",
                        e
                    )))
                }
            };
            z_t.view().clone().into_owned()
        };
        let y_mask = {
            let y_mask_t = match values["y_mask"].try_extract_tensor::<f32>() {
                Ok(out) => out,
                Err(e) => {
                    return Err(DengjenError::OperationError(format!(
                        "Failed to run model inference. Error: {}",
                        e
                    )))
                }
            };
            y_mask_t.view().clone().into_owned()
        };
        let p_duration = if values.contains_key("p_duration") {
            let p_duration_t = match values["p_duration"].try_extract_tensor::<f32>() {
                Ok(out) => out,
                Err(e) => {
                    return Err(DengjenError::OperationError(format!(
                        "Failed to run model inference. Error: {}",
                        e
                    )))
                }
            };
            Some(p_duration_t.view().clone().into_owned())
        } else {
            None
        };
        let g = if values.contains_key("g") {
            let g_t = match values["g"].try_extract_tensor::<f32>() {
                Ok(out) => out,
                Err(e) => {
                    return Err(DengjenError::OperationError(format!(
                        "Failed to run model inference. Error: {}",
                        e
                    )))
                }
            };
            g_t.view().clone().into_owned()
        } else {
            Array1::<f32>::from_iter([]).into_dyn()
        };
        Ok(Self { z, y_mask, p_duration, g })
    }
    fn infer_decoder(&self, session: &Session) -> DengjenResult<AudioSamples> {
        let outputs = {
            let session_outputs = if self.g.is_empty() {
                let inputs = ort::inputs![self.z.view(), self.y_mask.view()].unwrap();
                session.run(inputs)
            } else {
                let inputs = ort::inputs![self.z.view(), self.y_mask.view(), self.g.view()].unwrap();
                session.run(inputs)
            };
            match session_outputs {
                Ok(out) => out,
                Err(e) => {
                    return Err(DengjenError::OperationError(format!(
                        "Failed to run model inference. Error: {}",
                        e
                    )))
                }
            }
        };
        match outputs[0].try_extract_tensor::<f32>() {
            Ok(out) => Ok(Vec::from(out.view().as_slice().unwrap()).into()),
            Err(e) => Err(DengjenError::OperationError(format!(
                "Failed to run model inference. Error: {}",
                e
            ))),
        }
    }
}

struct SpeechStreamer {
    decoder_model: Arc<Session>,
    encoder_outputs: EncoderOutputs,
    mel_chunker: AdaptiveMelChunker,
    one_shot: bool,
}

impl SpeechStreamer {
    fn new(
        decoder_model: Arc<Session>,
        encoder_outputs: EncoderOutputs,
        chunk_size: usize,
        chunk_padding: usize,
        hop_length: usize,
    ) -> Self {
        let num_frames = encoder_outputs.z.shape()[2];
        let mel_chunker = AdaptiveMelChunker::new(
            num_frames as isize,
            chunk_size as isize,
            chunk_padding as isize,
            hop_length as isize,
        );
        let one_shot = num_frames <= (chunk_size * 2 + (chunk_padding * 2));
        Self {
            decoder_model,
            encoder_outputs,
            mel_chunker,
            one_shot,
        }
    }
    fn synthesize_chunk(
        &mut self,
        mel_index: ndarray::Slice,
        audio_index: ndarray::Slice,
    ) -> DengjenResult<AudioSamples> {
        // println!("Mel index: {:?}\nAudio Index: {:?}", mel_index, audio_index);
        let audio = {
            let session: Arc<Session> = Arc::clone(&self.decoder_model);
            let z_view = self.encoder_outputs.z.view();
            let y_mask_view = self.encoder_outputs.y_mask.view();
            let z_chunk = z_view.slice_axis(Axis(2), mel_index);
            let y_mask_chunk = y_mask_view.slice_axis(Axis(2), mel_index);
            let outputs = if self.encoder_outputs.g.is_empty() {
                let inputs = ort::inputs![z_chunk, y_mask_chunk].unwrap();
                session.run(inputs)
            } else {
                let inputs = ort::inputs![z_chunk, y_mask_chunk, self.encoder_outputs.g.view()].unwrap();
                session.run(inputs)
            };
            let outputs = outputs
                .map_err(|e| {
                    DengjenError::OperationError(format!(
                        "Failed to run model inference. Error: {}",
                        e
                    ))
                })?;
            let audio_t = outputs[0].try_extract_tensor::<f32>().map_err(|e| {
                DengjenError::OperationError(format!("Failed to run model inference. Error: {}", e))
            })?;
            self.process_chunk_audio(audio_t.view().view(), audio_index)?
        };
        Ok(audio)
    }
    #[inline(always)]
    fn process_chunk_audio(
        &mut self,
        audio_view: ArrayView<f32, Dim<IxDynImpl>>,
        audio_index: ndarray::Slice,
    ) -> DengjenResult<AudioSamples> {
        let mut audio: AudioSamples = audio_view
            .slice_axis(Axis(2), audio_index)
            .as_slice()
            .ok_or_else(|| DengjenError::with_message("Invalid model audio output"))?
            .to_vec()
            .into();
        audio.crossfade(42);
        Ok(audio)
    }
}

impl Iterator for SpeechStreamer {
    type Item = DengjenResult<AudioSamples>;

    fn next(&mut self) -> Option<Self::Item> {
        let (mel_index, audio_index) = self.mel_chunker.next()?;
        if self.one_shot {
            self.mel_chunker.consume();
            Some(
                self.encoder_outputs
                    .infer_decoder(self.decoder_model.as_ref()),
            )
        } else {
            Some(self.synthesize_chunk(mel_index, audio_index))
        }
    }
}

struct AdaptiveMelChunker {
    num_frames: isize,
    chunk_size: usize,
    chunk_padding: isize,
    hop_length: isize,
    last_end_index: Option<isize>,
    step: usize
}

impl AdaptiveMelChunker {
    fn new(num_frames: isize, chunk_size: isize, chunk_padding: isize, hop_length: isize) -> Self {
        Self {
            num_frames,
            chunk_size: chunk_size as usize,
            chunk_padding,
            hop_length,
            last_end_index: Some(0),
            step: 1
        }
    }
    fn consume(&mut self) {
        self.last_end_index = None;
    }
}

impl Iterator for AdaptiveMelChunker {
    type Item = (ndarray::Slice, ndarray::Slice);

    fn next(&mut self) -> Option<Self::Item> {
        let last_index = self.last_end_index?;
        let chunk_size = (self.chunk_size * self.step).min(MAX_CHUNK_SIZE);
        let (start_index, end_index): (isize, Option<isize>);
        let (start_padding, end_padding): (isize, Option<isize>);
        if last_index == 0 {
            start_index = 0;
            start_padding = 0;
        } else {
            start_index = last_index - (self.chunk_padding * 2);
            start_padding = self.chunk_padding;
        }
        let chunk_end = last_index + chunk_size as isize + self.chunk_padding;
        let remaining_frames = self.num_frames - chunk_end;
        if remaining_frames <= MIN_CHUNK_SIZE {
            end_index = None;
            end_padding = None;
        } else {
            end_index = Some(chunk_end);
            end_padding = Some(-self.chunk_padding)
        }
        self.step += 1;
        self.last_end_index = end_index;
        let chunk_index = ndarray::Slice::new(start_index, end_index, 1);
        let audio_index = ndarray::Slice::new(
            start_padding * self.hop_length,
            end_padding.map(|i| i * self.hop_length),
            1,
        );
        Some((chunk_index, audio_index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE_CONFIG_JSON: &str = r#"{
        "key": null,
        "language": null,
        "audio": {"sample_rate": 22050, "quality": null},
        "num_speakers": 1,
        "speaker_id_map": {},
        "streaming": false,
        "espeak": {"voice": "en-us"},
        "inference": {"noise_scale": 0.667, "length_scale": 1.0, "noise_w": 0.8},
        "num_symbols": 256,
        "phoneme_map": {},
        "phoneme_id_map": {"^": [1], "$": [2], "_": [3], "a": [4], "aɪ": [5]}
    }"#;

    #[test]
    fn model_config_parses_multi_char_phoneme_cluster_keys() {
        let config: ModelConfig = serde_json::from_str(BASE_CONFIG_JSON).unwrap();
        assert_eq!(config.phoneme_id_map.get("aɪ"), Some(&vec![5]));
    }

    #[test]
    fn model_config_defaults_phoneme_type_default_speaker_id_and_hop_length_when_absent() {
        let config: ModelConfig = serde_json::from_str(BASE_CONFIG_JSON).unwrap();
        assert_eq!(config.phoneme_type, None);
        assert_eq!(config.default_speaker_id, None);
        assert_eq!(config.hop_length, None);
    }

    #[test]
    fn model_config_parses_phoneme_type_default_speaker_id_and_hop_length_when_present() {
        let json = r#"{
            "key": null,
            "language": null,
            "audio": {"sample_rate": 22050, "quality": null},
            "num_speakers": 2,
            "speaker_id_map": {},
            "streaming": false,
            "espeak": {"voice": "en-us"},
            "inference": {"noise_scale": 0.667, "length_scale": 1.0, "noise_w": 0.8},
            "num_symbols": 256,
            "phoneme_map": {},
            "phoneme_id_map": {"^": [1], "$": [2], "_": [3]},
            "phoneme_type": "hebrew",
            "default_speaker_id": 3,
            "hop_length": 512
        }"#;
        let config: ModelConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.phoneme_type, Some(PhonemeType::Hebrew));
        assert_eq!(config.default_speaker_id, Some(3));
        assert_eq!(config.hop_length, Some(512));
    }

    fn single_char_phoneme_map() -> HashMap<String, Vec<i64>> {
        HashMap::from([
            ("^".to_string(), vec![1]),
            ("$".to_string(), vec![2]),
            ("_".to_string(), vec![3]),
            ("a".to_string(), vec![4]),
            ("b".to_string(), vec![5]),
        ])
    }

    #[test]
    fn map_phonemes_to_ids_wraps_with_bos_pad_and_eos() {
        let map = single_char_phoneme_map();
        let ids = map_phonemes_to_ids(&map, "ab", 3, 1, 2);
        assert_eq!(ids, vec![1, 4, 3, 5, 3, 2]);
    }

    #[test]
    fn map_phonemes_to_ids_skips_unknown_chars() {
        let map = single_char_phoneme_map();
        let ids = map_phonemes_to_ids(&map, "azb", 3, 1, 2);
        assert_eq!(ids, vec![1, 4, 3, 5, 3, 2]);
    }

    #[test]
    fn map_phonemes_to_ids_greedily_matches_multi_char_cluster_over_single_chars() {
        let mut map = single_char_phoneme_map();
        map.insert("aɪ".to_string(), vec![161]);
        map.insert("ɪ".to_string(), vec![99]);
        let ids = map_phonemes_to_ids(&map, "aɪ", 3, 1, 2);
        assert_eq!(ids, vec![1, 161, 3, 2]);
    }

    #[test]
    fn resolve_default_speaker_id_uses_configured_value() {
        let config = ModelConfig {
            default_speaker_id: Some(7),
            ..Default::default()
        };
        assert_eq!(resolve_default_speaker_id(&config), 7);
    }

    #[test]
    fn resolve_default_speaker_id_falls_back_to_zero_when_unset() {
        let config = ModelConfig {
            default_speaker_id: None,
            ..Default::default()
        };
        assert_eq!(resolve_default_speaker_id(&config), 0);
    }

    #[test]
    fn adaptive_mel_chunker_scales_audio_index_by_default_hop_length() {
        let mut chunker = AdaptiveMelChunker::new(5000, 100, 10, 256);
        let _first = chunker.next().unwrap();
        let (_, audio_index) = chunker.next().unwrap();
        assert_eq!(audio_index.start, 10 * 256);
        assert_eq!(audio_index.end, Some(-10 * 256));
    }

    #[test]
    fn adaptive_mel_chunker_scales_audio_index_by_custom_hop_length() {
        let mut chunker = AdaptiveMelChunker::new(5000, 100, 10, 100);
        let _first = chunker.next().unwrap();
        let (_, audio_index) = chunker.next().unwrap();
        assert_eq!(audio_index.start, 10 * 100);
        assert_eq!(audio_index.end, Some(-10 * 100));
    }

    #[test]
    fn phonemize_dispatch_falls_through_to_espeak_for_espeak_phoneme_type() {
        assert!(phonemize_dispatch(PhonemeType::Espeak, "hello").is_none());
    }

    #[test]
    fn phonemize_dispatch_passes_text_through_unchanged_for_text_phoneme_type() {
        let result = phonemize_dispatch(PhonemeType::Text, "hello").unwrap().unwrap();
        assert_eq!(result.sentences(), &vec!["hello".to_string()]);
    }

    #[test]
    fn phonemize_dispatch_errors_on_unsupported_pinyin_phoneme_type() {
        let result = phonemize_dispatch(PhonemeType::Pinyin, "hello").unwrap();
        assert!(result.is_err());
    }

    #[test]
    fn phonemize_dispatch_errors_on_unsupported_hebrew_phoneme_type() {
        let result = phonemize_dispatch(PhonemeType::Hebrew, "hello").unwrap();
        assert!(result.is_err());
    }

    #[cfg(feature = "tashkeel")]
    #[test]
    fn should_diacritize_true_for_arabic_voice_when_tashkeel_enabled() {
        assert!(should_diacritize("ar"));
    }

    #[cfg(feature = "tashkeel")]
    #[test]
    fn should_diacritize_false_for_non_arabic_voice_when_tashkeel_enabled() {
        assert!(!should_diacritize("en-us"));
    }

    #[cfg(not(feature = "tashkeel"))]
    #[test]
    fn should_diacritize_always_false_when_tashkeel_disabled() {
        assert!(!should_diacritize("ar"));
        assert!(!should_diacritize("en-us"));
    }
}
