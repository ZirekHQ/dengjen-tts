use dengjen_tts_core::{DengjenModel, SynthesisConfig};
use dengjen_tts_kokoro::{KokoroModel, KokoroVoiceConfig};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

const STYLE_DIM: usize = 256;
const MAX_TOKEN_LEN: usize = 510;

fn write_scaled_synthetic_voice_file(dir: &std::path::Path, voice_name: &str, scale: f32) {
    let path = dir.join(format!("{voice_name}.bin"));
    let mut bytes = Vec::with_capacity(MAX_TOKEN_LEN * STYLE_DIM * 4);
    for row in 0..MAX_TOKEN_LEN {
        for _ in 0..STYLE_DIM {
            bytes.extend_from_slice(&(row as f32 * scale).to_le_bytes());
        }
    }
    std::fs::write(&path, &bytes).unwrap();
}

fn write_minimal_vocab(dir: &std::path::Path) -> PathBuf {
    let path = dir.join("tokenizer.json");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(r#"{"model": {"vocab": {"$": 0, "t": 1, "ɛ": 2, "s": 3}}}"#.as_bytes())
        .unwrap();
    path
}

fn build_model_with_voices(test_name: &str, voices: &[&str]) -> (KokoroModel, PathBuf) {
    let scaled_voices: Vec<(&str, f32)> = voices.iter().map(|&v| (v, 1.0)).collect();
    build_model_with_scaled_voices(test_name, &scaled_voices)
}

fn build_model_with_scaled_voices(
    test_name: &str,
    voices: &[(&str, f32)],
) -> (KokoroModel, PathBuf) {
    let dir = std::env::temp_dir().join(format!("dengjen_kokoro_synthetic_inference_{test_name}"));
    std::fs::create_dir_all(&dir).unwrap();
    let voices_dir = dir.join("voices");
    std::fs::create_dir_all(&voices_dir).unwrap();
    for (voice, scale) in voices {
        write_scaled_synthetic_voice_file(&voices_dir, voice, *scale);
    }
    let vocab_path = write_minimal_vocab(&dir);

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let model_path = manifest_dir.join("tests/fixtures/synthetic_kokoro.onnx");

    let config = KokoroVoiceConfig {
        model_path,
        voices_dir,
        vocab_path,
        sample_rate: 24000,
        voices: voices.iter().map(|(v, _)| v.to_string()).collect(),
    };
    let model = KokoroModel::from_config(config).expect("failed to load synthetic Kokoro model");
    (model, dir)
}

#[test]
fn synthesizes_against_synthetic_fixture_without_panicking() {
    let (model, dir) = build_model_with_voices("basic", &["test_voice"]);

    let audio = model
        .speak_one_sentence("t\u{025b}st".to_string())
        .expect("synthesis against synthetic fixture failed");

    assert_eq!(audio.info.sample_rate, 24000);
    let samples = audio.samples.into_vec();
    assert!(!samples.is_empty(), "expected non-empty output samples");

    assert_eq!(samples.len(), 16000);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn get_speakers_returns_configured_voices_indexed_by_position() {
    let (model, dir) = build_model_with_voices("get_speakers", &["voice_a", "voice_b"]);

    let speakers = model
        .get_speakers()
        .expect("get_speakers should not error")
        .expect("expected Some speaker map for a multi-voice config");

    assert_eq!(
        speakers,
        &HashMap::from([(0, "voice_a".to_string()), (1, "voice_b".to_string())])
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn get_default_synthesis_config_selects_speaker_zero() {
    let (model, dir) = build_model_with_voices("default_config", &["voice_a", "voice_b"]);

    let config = model
        .get_default_synthesis_config()
        .expect("get_default_synthesis_config should not error")
        .expect("expected Some default synthesis config");

    assert_eq!(config.speaker, Some(0));
    assert!(config.parameters.is_empty());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn get_fallback_synthesis_config_reflects_a_previously_set_speaker() {
    let (model, dir) = build_model_with_voices("fallback_roundtrip", &["voice_a", "voice_b"]);

    model
        .set_fallback_synthesis_config(&SynthesisConfig {
            speaker: Some(1),
            parameters: HashMap::new(),
        })
        .expect("set_fallback_synthesis_config should not error");

    let config = model
        .get_fallback_synthesis_config()
        .expect("get_fallback_synthesis_config should not error")
        .expect("expected Some fallback synthesis config after it was set");

    assert_eq!(config.speaker, Some(1));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn set_fallback_synthesis_config_rejects_an_unknown_speaker_id() {
    let (model, dir) = build_model_with_voices("unknown_speaker", &["voice_a", "voice_b"]);

    let result = model.set_fallback_synthesis_config(&SynthesisConfig {
        speaker: Some(99),
        parameters: HashMap::new(),
    });

    assert!(
        matches!(
            result,
            Err(dengjen_tts_core::DengjenError::InvalidConfiguration(_))
        ),
        "expected an unknown speaker id to be rejected, got: {result:?}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn synthesis_falls_back_to_the_default_voice_for_an_unset_speaker() {
    let (model, dir) = build_model_with_voices("unset_speaker", &["voice_a", "voice_b"]);

    let audio = model
        .speak_one_sentence("t\u{025b}st".to_string())
        .expect("synthesis with no speaker selected should use the default voice");
    assert!(!audio.samples.into_vec().is_empty());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn switching_the_selected_speaker_changes_the_synthesized_audio() {
    // The synthetic fixture's output is a direct function of the style tensor,
    // so distinguishably-scaled voice files let a real output difference prove
    // synthesis used the *selected* voice, not always the first configured one.
    let (model, dir) =
        build_model_with_scaled_voices("speaker_switch", &[("voice_a", 1.0), ("voice_b", 5.0)]);

    let audio_a = model
        .speak_one_sentence("t\u{025b}st".to_string())
        .expect("synthesis with default (index 0) speaker failed");

    model
        .set_fallback_synthesis_config(&SynthesisConfig {
            speaker: Some(1),
            parameters: HashMap::new(),
        })
        .unwrap();
    let audio_b = model
        .speak_one_sentence("t\u{025b}st".to_string())
        .expect("synthesis with speaker 1 selected failed");

    assert_ne!(
        audio_a.samples.into_vec(),
        audio_b.samples.into_vec(),
        "expected switching the selected speaker to change the synthesized audio"
    );

    std::fs::remove_dir_all(&dir).ok();
}
