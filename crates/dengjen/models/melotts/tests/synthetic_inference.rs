use std::path::PathBuf;

#[test]
fn synthesizes_against_synthetic_fixture_without_panicking() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_path = manifest_dir.join("tests/fixtures/synthetic_melotts.onnx");

    let dir = std::env::temp_dir().join("dengjen_melotts_synthetic_inference_test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(&fixture_path, dir.join("model.onnx")).unwrap();
    let config_json = r#"{
        "audio": {"sample_rate": 24000},
        "phonemizer": {"type": "espeak", "voice": "en-us"},
        "phone_id_map": {"^": [1], "$": [2], "_": [3], "t": [4]},
        "tone_id_map": {"_": 0},
        "inference": {"noise_scale": 0.667, "length_scale": 1.0, "noise_scale_w": 0.8},
        "model_path": "model.onnx"
    }"#;
    let config_path = dir.join("config.json");
    std::fs::write(&config_path, config_json).unwrap();

    let model = dengjen_tts_melotts::from_config_path(&config_path)
        .expect("failed to load synthetic MeloTTS model");

    let audio = model
        .speak_one_sentence("t:_".to_string())
        .expect("synthesis against synthetic fixture failed");

    assert_eq!(audio.info.sample_rate, 24000);
    assert_eq!(audio.samples.into_vec().len(), 16000);

    std::fs::remove_dir_all(&dir).ok();
}
