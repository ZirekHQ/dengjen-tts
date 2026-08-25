#[test]
fn synthesizes_against_a_real_downloaded_melotts_voice() {
    let Ok(config_path) = std::env::var("DENGJEN_MELOTTS_TEST_VOICE_CONFIG") else {
        eprintln!("skipping: DENGJEN_MELOTTS_TEST_VOICE_CONFIG not set");
        return;
    };
    let model = dengjen_tts_melotts::from_config_path(std::path::Path::new(&config_path)).unwrap();
    let audio = model
        .speak_one_sentence("Hello, this is a test.".to_string())
        .unwrap();
    assert!(!audio.samples.into_vec().is_empty());
}
