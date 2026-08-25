#[test]
fn synthesizes_against_a_real_downloaded_melotts_voice() {
    let Ok(config_path) = std::env::var("DENGJEN_MELOTTS_TEST_VOICE_CONFIG") else {
        eprintln!("skipping: DENGJEN_MELOTTS_TEST_VOICE_CONFIG not set");
        return;
    };
    let model = dengjen_tts_melotts::from_config_path(std::path::Path::new(&config_path)).unwrap();
    let phonemes = model
        .phonemize_text("Hello, this is a test.")
        .expect("phonemization failed");
    for sentence in phonemes.sentences() {
        let audio = model
            .speak_one_sentence(sentence.clone())
            .expect("synthesis failed");
        assert!(!audio.samples.into_vec().is_empty());
    }
}
