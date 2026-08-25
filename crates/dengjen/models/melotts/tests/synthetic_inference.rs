use dengjen_tts_melotts::MeloTTSModel;
use std::collections::HashMap;
use std::path::PathBuf;

fn test_config() -> dengjen_tts_melotts::MeloVoiceConfig {
    dengjen_tts_melotts::MeloVoiceConfig {
        audio: dengjen_tts_melotts::AudioConfig { sample_rate: 24000 },
        phonemizer: dengjen_tts_melotts::PhonemizerConfig::Espeak {
            voice: "en-us".to_string(),
        },
        phone_id_map: HashMap::from([
            ("^".to_string(), vec![1]),
            ("$".to_string(), vec![2]),
            ("_".to_string(), vec![3]),
            ("t".to_string(), vec![4]),
        ]),
        tone_id_map: HashMap::from([("_".to_string(), 0)]),
        speaker_id_map: HashMap::new(),
        default_speaker_id: None,
        inference: dengjen_tts_melotts::InferenceConfig {
            noise_scale: 0.667,
            length_scale: 1.0,
            noise_scale_w: 0.8,
        },
    }
}

#[test]
fn synthesizes_against_synthetic_fixture_without_panicking() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let model_path = manifest_dir.join("tests/fixtures/synthetic_melotts.onnx");
    let model = MeloTTSModel::from_config_with_model_path(test_config(), &model_path)
        .expect("failed to load synthetic MeloTTS model");

    let pairs = vec![("t".to_string(), "_".to_string())];
    let audio = model
        .synthesize_phone_tone_pairs(&pairs, 0)
        .expect("synthesis against synthetic fixture failed");

    assert_eq!(audio.info.sample_rate, 24000);
    let samples = audio.samples.into_vec();
    assert_eq!(samples.len(), 16000);
}
