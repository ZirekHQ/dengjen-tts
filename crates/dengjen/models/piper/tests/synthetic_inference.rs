use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

fn phoneme_id_map_json() -> &'static str {
    r#"{"^": [1], "$": [2], "_": [3], "t": [4], "ɛ": [5], "s": [6]}"#
}

fn synthetic_model_config_json() -> String {
    format!(
        r#"{{
            "key": null,
            "language": {{"code": "en-US"}},
            "audio": {{"sample_rate": 22050, "quality": null}},
            "num_speakers": 1,
            "speaker_id_map": {{"default": 0}},
            "streaming": false,
            "espeak": {{"voice": "en-us"}},
            "inference": {{"noise_scale": 0.667, "length_scale": 1.0, "noise_w": 0.8}},
            "num_symbols": 8,
            "phoneme_map": {{}},
            "phoneme_id_map": {phoneme_map},
            "phoneme_type": "text",
            "hop_length": 256
        }}"#,
        phoneme_map = phoneme_id_map_json(),
    )
}

/// Loads a real (synthetic-fixture) `VitsModel` -- exercises the same config/session
/// wiring as `dengjen_tts_piper::from_config_path` without needing a real trained voice.
fn load_synthetic_model(dir_name: &str) -> Arc<dyn dengjen_tts_core::DengjenModel + Send + Sync> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_path = manifest_dir.join("tests/fixtures/synthetic_piper_batch.onnx");

    let dir = std::env::temp_dir().join(dir_name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(&fixture_path, dir.join("model.onnx")).unwrap();
    let config_path = dir.join("model.onnx.json");
    std::fs::write(&config_path, synthetic_model_config_json()).unwrap();

    let model = dengjen_tts_piper::from_config_path(&config_path)
        .expect("failed to load synthetic Piper model");
    std::fs::remove_dir_all(&dir).ok();
    model
}

#[test]
fn speak_one_sentence_synthesizes_against_the_synthetic_fixture() {
    let model = load_synthetic_model("dengjen_piper_synthetic_speak_one_sentence_test");
    let audio = model
        .speak_one_sentence("t".to_string())
        .expect("synthesis against synthetic fixture failed");
    assert_eq!(audio.info.sample_rate, 22050);
    assert!(!audio.samples.into_vec().is_empty());
}

#[test]
fn speak_batch_synthesizes_each_sentence_independently() {
    let model = load_synthetic_model("dengjen_piper_synthetic_speak_batch_test");
    let audios = model
        .speak_batch(vec!["t".to_string(), "s".to_string()])
        .expect("batch synthesis against synthetic fixture failed");
    assert_eq!(audios.len(), 2);
}

#[test]
fn phonemize_text_passes_through_unchanged_for_the_text_phoneme_type() {
    let model = load_synthetic_model("dengjen_piper_synthetic_phonemize_text_test");
    let phonemes = model.phonemize_text("ts").unwrap();
    assert_eq!(phonemes.num_sentences(), 1);
    assert_eq!(phonemes.sentences()[0], "ts");
}

#[test]
fn audio_output_info_reflects_the_configured_sample_rate() {
    let model = load_synthetic_model("dengjen_piper_synthetic_audio_output_info_test");
    let info = model.audio_output_info().unwrap();
    assert_eq!(info.sample_rate, 22050);
}

#[test]
fn get_default_synthesis_config_reflects_the_config_file_inference_settings() {
    let model = load_synthetic_model("dengjen_piper_synthetic_default_synth_config_test");
    let default = model
        .get_default_synthesis_config()
        .unwrap()
        .expect("Piper models always report a default synthesis config");
    assert_eq!(default.speaker, Some(0));
}

#[test]
fn fallback_synthesis_config_starts_at_the_factory_default_then_updates_on_set() {
    let model = load_synthetic_model("dengjen_piper_synthetic_fallback_config_roundtrip_test");
    let initial = model.get_fallback_synthesis_config().unwrap().unwrap();
    // Unlike get_default_synthesis_config (which resolves a factory default speaker), the
    // live fallback config starts with no speaker override until one is explicitly set.
    assert_eq!(initial.speaker, None);

    let mut parameters = HashMap::new();
    parameters.insert("noise_scale".to_string(), 1.5f32);
    parameters.insert("length_scale".to_string(), 1.0f32);
    parameters.insert("noise_w".to_string(), 0.8f32);
    model
        .set_fallback_synthesis_config(&dengjen_tts_core::SynthesisConfig {
            speaker: Some(0),
            parameters,
        })
        .expect("failed to set fallback synthesis config");

    let updated = model.get_fallback_synthesis_config().unwrap().unwrap();
    assert_eq!(updated.parameters.get("noise_scale"), Some(&1.5));
}

#[test]
fn set_fallback_synthesis_config_rejects_an_unknown_speaker_id() {
    let model = load_synthetic_model("dengjen_piper_synthetic_unknown_speaker_test");
    let result = model.set_fallback_synthesis_config(&dengjen_tts_core::SynthesisConfig {
        speaker: Some(99),
        parameters: HashMap::new(),
    });
    assert!(result.is_err());
}

#[test]
fn get_speakers_and_speaker_name_to_id_reflect_the_configured_speaker_map() {
    let model = load_synthetic_model("dengjen_piper_synthetic_get_speakers_test");
    let speakers = model.get_speakers().unwrap().unwrap();
    assert_eq!(speakers.get(&0), Some(&"default".to_string()));
    assert_eq!(model.speaker_name_to_id("default").unwrap(), Some(0));
    assert_eq!(model.speaker_name_to_id("nobody").unwrap(), None);
}

#[test]
fn get_language_reads_the_configured_language_code() {
    let model = load_synthetic_model("dengjen_piper_synthetic_get_language_test");
    assert_eq!(model.get_language().unwrap(), Some("en-US".to_string()));
}

#[test]
fn properties_reports_an_unknown_quality_when_the_config_omits_it() {
    let model = load_synthetic_model("dengjen_piper_synthetic_properties_test");
    let properties = model.properties().unwrap();
    assert_eq!(properties.get("quality"), Some(&"unknown".to_string()));
}
