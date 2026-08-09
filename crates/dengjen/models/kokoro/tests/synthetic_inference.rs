use dengjen_core::DengjenModel;
use dengjen_kokoro::{KokoroModel, KokoroVoiceConfig};
use std::io::Write;
use std::path::PathBuf;

// This test exercises the real inference plumbing (tensor construction, session.run,
// output extraction) against the checked-in synthetic fixture from Task 5 Step 1 - it
// does not assert anything about real speech quality, only that the pipeline runs and
// produces the expected shape/type of output.

const STYLE_DIM: usize = 256;
const MAX_TOKEN_LEN: usize = 510;

fn write_synthetic_voice_file(dir: &std::path::Path, voice_name: &str) {
    let path = dir.join(format!("{voice_name}.bin"));
    let mut bytes = Vec::with_capacity(MAX_TOKEN_LEN * STYLE_DIM * 4);
    for row in 0..MAX_TOKEN_LEN {
        for _ in 0..STYLE_DIM {
            bytes.extend_from_slice(&(row as f32).to_le_bytes());
        }
    }
    std::fs::write(&path, &bytes).unwrap();
}

fn write_minimal_vocab(dir: &std::path::Path) -> PathBuf {
    let path = dir.join("tokenizer.json");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(
        r#"{"model": {"vocab": {"$": 0, "t": 1, "ɛ": 2, "s": 3}}}"#.as_bytes(),
    )
    .unwrap();
    path
}

#[test]
fn synthesizes_against_synthetic_fixture_without_panicking() {
    let dir = std::env::temp_dir().join("dengjen_kokoro_synthetic_inference_test");
    std::fs::create_dir_all(&dir).unwrap();
    let voices_dir = dir.join("voices");
    std::fs::create_dir_all(&voices_dir).unwrap();
    write_synthetic_voice_file(&voices_dir, "test_voice");
    let vocab_path = write_minimal_vocab(&dir);

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let model_path = manifest_dir.join("tests/fixtures/synthetic_kokoro.onnx");

    let config = KokoroVoiceConfig {
        model_path,
        voices_dir,
        vocab_path,
        sample_rate: 24000,
        voices: vec!["test_voice".to_string()],
    };
    let model = KokoroModel::from_config(config).expect("failed to load synthetic Kokoro model");

    // "tɛst" phonemes (U+025B is ɛ), tokenizes against the minimal vocab above.
    let audio = model
        .speak_one_sentence("t\u{025b}st".to_string())
        .expect("synthesis against synthetic fixture failed");

    assert_eq!(audio.info.sample_rate, 24000);
    let samples = audio.samples.into_vec();
    assert!(!samples.is_empty(), "expected non-empty output samples");
    // The synthetic graph always outputs exactly 16000 samples (see Task 5 Step 1's
    // generator script) - not asserting sample VALUES, since they're an arbitrary
    // placeholder computation, not real audio.
    assert_eq!(samples.len(), 16000);

    std::fs::remove_dir_all(&dir).ok();
}
