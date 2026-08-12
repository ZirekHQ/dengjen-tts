use dengjen_core::{DengjenError, DengjenResult, PiperSynthesisConfig};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

#[derive(Deserialize, Default)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub quality: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct ESpeakConfig {
    pub(crate) voice: String,
}

#[derive(Deserialize, Default, Clone)]
pub struct InferenceConfig {
    pub(crate) noise_scale: f32,
    pub(crate) length_scale: f32,
    pub(crate) noise_w: f32,
}

#[derive(Clone, Deserialize, Default)]
pub struct Language {
    pub(crate) code: String,
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
    pub(crate) streaming: Option<bool>,
    pub(crate) espeak: ESpeakConfig,
    pub(crate) inference: InferenceConfig,
    #[allow(dead_code)]
    pub(crate) num_symbols: u32,
    #[allow(dead_code)]
    pub(crate) phoneme_map: HashMap<i64, String>,
    pub(crate) phoneme_id_map: HashMap<String, Vec<i64>>,
    pub(crate) phoneme_type: Option<PhonemeType>,
    pub(crate) default_speaker_id: Option<i64>,
    pub(crate) hop_length: Option<usize>,
}

pub(crate) fn load_model_config(
    config_path: &Path,
) -> DengjenResult<(ModelConfig, PiperSynthesisConfig)> {
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

pub(crate) fn map_phonemes_to_ids(
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

pub(crate) fn resolve_default_speaker_id(config: &ModelConfig) -> i64 {
    config.default_speaker_id.unwrap_or(0)
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
    fn load_model_config_errors_when_file_does_not_exist() {
        let path = std::path::Path::new("/nonexistent-piper-config-xyz.json");
        let result = load_model_config(path);
        assert!(matches!(result, Err(DengjenError::FailedToLoadResource(_))));
    }

    #[test]
    fn load_model_config_errors_on_malformed_json() {
        let mut path = std::env::temp_dir();
        path.push(format!("dengjen-piper-test-malformed-{}.json", std::process::id()));
        std::fs::write(&path, b"{ not valid json").unwrap();
        let result = load_model_config(&path);
        std::fs::remove_file(&path).ok();
        assert!(matches!(result, Err(DengjenError::FailedToLoadResource(_))));
    }
}
