#![allow(dead_code)]

use dengjen_tts_core::{DengjenError, DengjenResult};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
pub struct AudioConfig {
    pub sample_rate: u32,
}

#[derive(Deserialize)]
pub struct InferenceConfig {
    pub noise_scale: f32,
    pub length_scale: f32,
    pub noise_scale_w: f32,
}

#[derive(Deserialize, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PhonemizerConfig {
    Espeak { voice: String },
    Pinyin { model_dir: PathBuf },
}

#[derive(Deserialize)]
pub struct MeloVoiceConfig {
    pub audio: AudioConfig,
    pub phonemizer: PhonemizerConfig,
    pub phone_id_map: HashMap<String, Vec<i64>>,
    #[serde(default)]
    pub tone_id_map: HashMap<String, i64>,
    #[serde(default)]
    pub speaker_id_map: HashMap<String, i64>,
    #[serde(default)]
    pub default_speaker_id: Option<i64>,
    pub inference: InferenceConfig,
}

pub fn load_config(path: &Path) -> DengjenResult<MeloVoiceConfig> {
    let raw = std::fs::read_to_string(path).map_err(|e| {
        DengjenError::FailedToLoadResource(format!(
            "Failed to read MeloTTS voice config at `{}`: {e}",
            path.display()
        ))
    })?;
    serde_json::from_str(&raw).map_err(|e| {
        DengjenError::InvalidConfiguration(format!(
            "Failed to parse MeloTTS voice config at `{}`: {e}",
            path.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE_CONFIG_JSON: &str = r#"{
        "audio": {"sample_rate": 44100},
        "phonemizer": {"type": "espeak", "voice": "en-us"},
        "phone_id_map": {"^": [1], "$": [2], "_": [3], "a": [4]},
        "inference": {"noise_scale": 0.667, "length_scale": 1.0, "noise_scale_w": 0.8}
    }"#;

    #[test]
    fn parses_a_minimal_espeak_voice_config() {
        let config: MeloVoiceConfig = serde_json::from_str(BASE_CONFIG_JSON).unwrap();
        assert_eq!(config.audio.sample_rate, 44100);
        assert_eq!(
            config.phonemizer,
            PhonemizerConfig::Espeak {
                voice: "en-us".to_string()
            }
        );
        assert_eq!(config.phone_id_map.get("a"), Some(&vec![4]));
        assert!(config.tone_id_map.is_empty());
        assert!(config.speaker_id_map.is_empty());
        assert_eq!(config.default_speaker_id, None);
    }

    #[test]
    fn parses_a_pinyin_voice_config_with_tone_id_map_and_speakers() {
        let json = r#"{
            "audio": {"sample_rate": 44100},
            "phonemizer": {"type": "pinyin", "model_dir": "/models/g2pw"},
            "phone_id_map": {"^": [1], "$": [2], "_": [3], "zh": [4], "ang": [5]},
            "tone_id_map": {"_": 0, "1": 1, "2": 2, "3": 3, "4": 4},
            "speaker_id_map": {"default": 0},
            "default_speaker_id": 0,
            "inference": {"noise_scale": 0.667, "length_scale": 1.0, "noise_scale_w": 0.8}
        }"#;
        let config: MeloVoiceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(
            config.phonemizer,
            PhonemizerConfig::Pinyin {
                model_dir: PathBuf::from("/models/g2pw")
            }
        );
        assert_eq!(config.tone_id_map.get("2"), Some(&2));
        assert_eq!(config.speaker_id_map.get("default"), Some(&0));
        assert_eq!(config.default_speaker_id, Some(0));
    }

    #[test]
    fn rejects_a_config_with_an_unknown_phonemizer_type() {
        let json = r#"{
            "audio": {"sample_rate": 44100},
            "phonemizer": {"type": "festival", "voice": "en"},
            "phone_id_map": {},
            "inference": {"noise_scale": 0.667, "length_scale": 1.0, "noise_scale_w": 0.8}
        }"#;
        assert!(serde_json::from_str::<MeloVoiceConfig>(json).is_err());
    }

    #[test]
    fn load_config_errors_when_the_file_does_not_exist() {
        let result = load_config(Path::new("/nonexistent/config.json"));
        assert!(matches!(result, Err(DengjenError::FailedToLoadResource(_))));
    }
}
