fn real_voice_config_path() -> Option<std::path::PathBuf> {
    std::env::var("DENGJEN_KOKORO_TEST_VOICE_CONFIG")
        .ok()
        .map(std::path::PathBuf::from)
}

#[test]
fn synthesizes_real_audio_from_a_real_voice() {
    let Some(config_path) = real_voice_config_path() else {
        eprintln!(
            "Skipping: set DENGJEN_KOKORO_TEST_VOICE_CONFIG to a real Kokoro voice config to run this test"
        );
        return;
    };
    let model =
        dengjen_kokoro::from_config_path(&config_path).expect("failed to load real Kokoro voice");
    let phonemes = model
        .phonemize_text("Hello, world!")
        .expect("phonemization failed");
    let audio = model
        .speak_one_sentence(phonemes.to_string())
        .expect("synthesis failed");
    assert!(
        !audio.samples.into_vec().is_empty(),
        "expected non-empty audio samples"
    );
}
