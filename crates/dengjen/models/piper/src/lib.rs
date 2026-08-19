#![forbid(unsafe_code)]

use dengjen_tts_core::{AudioInfo, DengjenError, DengjenModel, DengjenResult, Phonemes};
#[cfg(feature = "espeak")]
use espeak_phonemizer::text_to_phonemes;
#[cfg(feature = "tashkeel")]
use libtashkeel_core::do_tashkeel;
#[cfg(feature = "espeak")]
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

mod config;
mod inference;
mod phonemize;
mod streaming;

pub use config::*;
use config::{load_model_config, resolve_default_speaker_id};
pub use inference::VitsModel;
#[cfg(feature = "espeak")]
use phonemize::should_diacritize;
use phonemize::{phonemize_dispatch, TashkeelEngine};
pub use streaming::VitsStreamingModel;

const PAD: &str = "_";
const BOS: &str = "^";
const EOS: &str = "$";

pub fn from_config_path(config_path: &Path) -> DengjenResult<Arc<dyn DengjenModel + Send + Sync>> {
    let (config, synth_config) = load_model_config(config_path)?;

    if config.streaming.unwrap_or_default() {
        let encoder_path = config_path.with_file_name("encoder.onnx");
        let decoder_path = config_path.with_file_name("decoder.onnx");
        let model =
            VitsStreamingModel::from_config(config, synth_config, &encoder_path, &decoder_path)?;
        return Ok(Arc::new(model));
    }

    let Some(stem) = config_path.file_stem() else {
        return Err(DengjenError::InvalidConfiguration(format!(
            "Invalid config filename format `{}`",
            config_path.display()
        )));
    };
    let onnx_path = config_path.with_file_name(stem);
    let model = VitsModel::from_config(config, synth_config, &onnx_path)?;
    Ok(Arc::new(model))
}

pub use dengjen_tts_core::PiperSynthesisConfig;

trait VitsModelCommons {
    fn get_synth_config(&self) -> &RwLock<PiperSynthesisConfig>;
    fn get_config(&self) -> &ModelConfig;
    fn get_speaker_map(&self) -> &HashMap<i64, String>;
    #[cfg_attr(not(all(feature = "tashkeel", feature = "espeak")), allow(dead_code))]
    fn get_tashkeel_engine(&self) -> Option<&TashkeelEngine>;

    fn get_meta_ids(&self) -> (i64, i64, i64) {
        let phoneme_id_map = &self.get_config().phoneme_id_map;
        let first_id = |token: &str| *phoneme_id_map.get(token).unwrap().first().unwrap();
        (first_id(PAD), first_id(BOS), first_id(EOS))
    }

    fn language(&self) -> Option<String> {
        let config = self.get_config();
        match &config.language {
            Some(lang) => Some(lang.code.clone()),
            None => Some(config.espeak.voice.clone()),
        }
    }

    fn get_properties(&self) -> HashMap<String, String> {
        let quality = self
            .get_config()
            .audio
            .quality
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        HashMap::from([("quality".to_string(), quality)])
    }

    fn factory_synthesis_config(&self) -> PiperSynthesisConfig {
        let config = self.get_config();
        let speaker = (config.num_speakers > 0).then(|| resolve_default_speaker_id(config));
        PiperSynthesisConfig {
            speaker,
            length_scale: config.inference.length_scale,
            noise_scale: config.inference.noise_scale,
            noise_w: config.inference.noise_w,
        }
    }

    fn _do_set_default_synth_config(&self, new_config: &PiperSynthesisConfig) -> DengjenResult<()> {
        let mut synth_config = self.get_synth_config().write().unwrap();
        synth_config.length_scale = new_config.length_scale;
        synth_config.noise_scale = new_config.noise_scale;
        synth_config.noise_w = new_config.noise_w;
        if let Some(sid) = new_config.speaker {
            if !self.get_speaker_map().contains_key(&sid) {
                return Err(DengjenError::InvalidConfiguration(format!(
                    "No speaker was found with the given id `{}`",
                    sid
                )));
            }
            synth_config.speaker = Some(sid);
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
        map_phonemes_to_ids(
            &self.get_config().phoneme_id_map,
            phonemes,
            pad_id,
            bos_id,
            eos_id,
        )
    }
    #[cfg(feature = "espeak")]
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
    #[cfg(not(feature = "espeak"))]
    fn do_phonemize_text(&self, text: &str) -> DengjenResult<Phonemes> {
        let config = self.get_config();
        if let Some(result) = phonemize_dispatch(config.phoneme_type.unwrap_or_default(), text) {
            return result;
        }
        Err(DengjenError::PhonemizationError(
            "This voice requires espeak-based phonemization, but the `espeak` feature (GPL-3.0-or-later, via espeak-ng) is disabled".to_string(),
        ))
    }
    #[cfg(feature = "tashkeel")]
    #[cfg_attr(not(feature = "espeak"), allow(dead_code))]
    fn diacritize_text(&self, text: &str) -> DengjenResult<String> {
        match do_tashkeel(self.get_tashkeel_engine().unwrap(), text, None, false) {
            Ok(diacritized_text) => Ok(diacritized_text),
            Err(msg) => Err(DengjenError::InferenceError(format!(
                "Failed to diacritize text using  libtashkeel. {}",
                msg
            ))),
        }
    }
    // should_diacritize() is always false without this feature, so this is unreachable.
    #[cfg(not(feature = "tashkeel"))]
    #[cfg_attr(not(feature = "espeak"), allow(dead_code))]
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

#[cfg(test)]
mod tests {
    use super::*;

    struct TestVitsCommons {
        synth_config: RwLock<PiperSynthesisConfig>,
        config: ModelConfig,
        speaker_map: HashMap<i64, String>,
    }

    impl VitsModelCommons for TestVitsCommons {
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
            None
        }
    }

    #[test]
    fn get_meta_ids_reads_bos_pad_eos_from_phoneme_id_map() {
        let commons = TestVitsCommons {
            synth_config: RwLock::new(PiperSynthesisConfig::default()),
            config: ModelConfig {
                phoneme_id_map: HashMap::from([
                    (PAD.to_string(), vec![3]),
                    (BOS.to_string(), vec![1]),
                    (EOS.to_string(), vec![2]),
                ]),
                ..Default::default()
            },
            speaker_map: HashMap::new(),
        };
        assert_eq!(commons.get_meta_ids(), (3, 1, 2));
    }

    #[test]
    fn do_set_default_synth_config_updates_scales_and_accepts_a_known_speaker() {
        let commons = TestVitsCommons {
            synth_config: RwLock::new(PiperSynthesisConfig::default()),
            config: ModelConfig::default(),
            speaker_map: HashMap::from([(5, "narrator".to_string())]),
        };
        let new_config = PiperSynthesisConfig {
            speaker: Some(5),
            noise_scale: 0.5,
            length_scale: 1.2,
            noise_w: 0.9,
        };
        commons._do_set_default_synth_config(&new_config).unwrap();
        let synth_config = commons.synth_config.read().unwrap();
        assert_eq!(synth_config.speaker, Some(5));
        assert_eq!(synth_config.length_scale, 1.2);
    }

    #[test]
    fn do_set_default_synth_config_errors_for_an_unknown_speaker_id() {
        let commons = TestVitsCommons {
            synth_config: RwLock::new(PiperSynthesisConfig::default()),
            config: ModelConfig::default(),
            speaker_map: HashMap::new(),
        };
        let new_config = PiperSynthesisConfig {
            speaker: Some(99),
            ..Default::default()
        };
        let result = commons._do_set_default_synth_config(&new_config);
        assert!(matches!(result, Err(DengjenError::InvalidConfiguration(_))));
    }

    #[test]
    fn do_set_default_synth_config_applies_scales_even_when_the_speaker_id_is_unknown() {
        let commons = TestVitsCommons {
            synth_config: RwLock::new(PiperSynthesisConfig::default()),
            config: ModelConfig::default(),
            speaker_map: HashMap::new(),
        };
        let new_config = PiperSynthesisConfig {
            speaker: Some(99),
            noise_scale: 0.5,
            length_scale: 1.2,
            noise_w: 0.9,
        };
        let result = commons._do_set_default_synth_config(&new_config);
        assert!(matches!(result, Err(DengjenError::InvalidConfiguration(_))));
        let synth_config = commons.synth_config.read().unwrap();
        assert_eq!(synth_config.length_scale, 1.2);
        assert_eq!(synth_config.noise_scale, 0.5);
        assert_eq!(synth_config.noise_w, 0.9);
        assert_eq!(synth_config.speaker, None);
    }

    #[test]
    fn do_phonemize_text_passes_through_unchanged_for_text_phoneme_type() {
        let commons = TestVitsCommons {
            synth_config: RwLock::new(PiperSynthesisConfig::default()),
            config: ModelConfig {
                phoneme_type: Some(PhonemeType::Text),
                ..Default::default()
            },
            speaker_map: HashMap::new(),
        };
        let result = commons.do_phonemize_text("hello").unwrap();
        assert_eq!(result.to_vec(), vec!["hello".to_string()]);
    }

    #[cfg(not(feature = "espeak"))]
    #[test]
    fn do_phonemize_text_errors_when_espeak_feature_is_disabled() {
        let commons = TestVitsCommons {
            synth_config: RwLock::new(PiperSynthesisConfig::default()),
            config: ModelConfig {
                phoneme_type: Some(PhonemeType::Espeak),
                ..Default::default()
            },
            speaker_map: HashMap::new(),
        };
        let result = commons.do_phonemize_text("hello");
        assert!(matches!(result, Err(DengjenError::PhonemizationError(_))));
    }
}
