use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

fn synthetic_model_config_json() -> &'static str {
    r#"{
        "audio": {"sample_rate": 24000},
        "phonemizer": {"type": "espeak", "voice": "en-us"},
        "phone_id_map": {"^": [1], "$": [2], "_": [3], "t": [4]},
        "tone_id_map": {"_": 0},
        "inference": {"noise_scale": 0.667, "length_scale": 1.0, "noise_scale_w": 0.8},
        "model_path": "model.onnx"
    }"#
}

fn load_synthetic_model(dir_name: &str) -> Arc<dyn dengjen_tts_core::DengjenModel + Send + Sync> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_path = manifest_dir.join("tests/fixtures/synthetic_melotts.onnx");

    let dir = std::env::temp_dir().join(dir_name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(&fixture_path, dir.join("model.onnx")).unwrap();
    let config_path = dir.join("config.json");
    std::fs::write(&config_path, synthetic_model_config_json()).unwrap();

    let model = dengjen_tts_melotts::from_config_path(&config_path)
        .expect("failed to load synthetic MeloTTS model");
    std::fs::remove_dir_all(&dir).ok();
    model
}

#[test]
fn synthesizes_against_synthetic_fixture_without_panicking() {
    let model = load_synthetic_model("dengjen_melotts_synthetic_inference_test");

    let audio = model
        .speak_one_sentence("t:_".to_string())
        .expect("synthesis against synthetic fixture failed");

    assert_eq!(audio.info.sample_rate, 24000);
    assert_eq!(audio.samples.into_vec().len(), 16000);
}

#[test]
fn set_fallback_synthesis_config_changes_inference_output_values() {
    // A noise_scale set via set_fallback_synthesis_config must actually reach
    // inference, not be silently dropped in favor of the static voice-manifest
    // default. The synthetic fixture's output is deterministically
    // `seq_len * noise_scale` tiled to 16000 samples, so two different noise_scale
    // values must produce two different sample buffers.
    let model = load_synthetic_model("dengjen_melotts_synthetic_inference_fallback_test");

    let default_audio = model
        .speak_one_sentence("t:_".to_string())
        .expect("synthesis against synthetic fixture failed");

    let mut parameters = HashMap::new();
    parameters.insert("noise_scale".to_string(), 1.5f32);
    model
        .set_fallback_synthesis_config(&dengjen_tts_core::SynthesisConfig {
            speaker: None,
            parameters,
        })
        .expect("failed to set fallback synthesis config");

    let overridden_audio = model
        .speak_one_sentence("t:_".to_string())
        .expect("synthesis against synthetic fixture failed");

    assert_ne!(
        default_audio.samples.into_vec(),
        overridden_audio.samples.into_vec(),
        "expected a noise_scale set via set_fallback_synthesis_config to change inference \
         output, but output samples were identical -- this means the live fallback config \
         is being ignored"
    );
}
