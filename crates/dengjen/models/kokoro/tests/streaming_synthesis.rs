use dengjen_core::{CancellationToken, DengjenModel};
use dengjen_kokoro::{KokoroModel, KokoroVoiceConfig};
use std::io::Write;
use std::path::PathBuf;

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
    f.write_all(r#"{"model": {"vocab": {"$": 0, "t": 1, "ɛ": 2, "s": 3}}}"#.as_bytes())
        .unwrap();
    path
}

fn load_synthetic_model(test_name: &str) -> KokoroModel {
    let dir = std::env::temp_dir().join(format!("dengjen_kokoro_{test_name}"));
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
    std::fs::remove_dir_all(&dir).ok();
    model
}

// "tɛst" phonemes (U+025B is ɛ), tokenizes against the minimal vocab above.
const TEST_PHONEMES: &str = "t\u{025b}st";

#[test]
fn supports_streaming_output_is_true() {
    let model = load_synthetic_model("supports_streaming_output_is_true");
    assert!(model.supports_streaming_output());
}

#[test]
fn chunk_size_zero_returns_unsupported_operation() {
    let model = load_synthetic_model("chunk_size_zero_returns_unsupported_operation");
    let result = model.stream_synthesis(
        TEST_PHONEMES.to_string(),
        0,
        3,
        CancellationToken::new(),
    );
    assert!(matches!(result, Err(dengjen_core::DengjenError::UnsupportedOperation(_))));
}

#[test]
fn already_cancelled_yields_no_chunks_and_no_error() {
    let model = load_synthetic_model("already_cancelled_yields_no_chunks_and_no_error");
    let cancel_token = CancellationToken::new();
    cancel_token.cancel();
    let mut stream = model
        .stream_synthesis(TEST_PHONEMES.to_string(), 4096, 3, cancel_token)
        .expect("stream_synthesis should not error when already cancelled");
    assert!(stream.next().is_none());
}

#[test]
fn streamed_chunks_total_matches_speak_one_sentence_total() {
    let model = load_synthetic_model("streamed_chunks_total_matches_speak_one_sentence_total");

    let whole = model
        .speak_one_sentence(TEST_PHONEMES.to_string())
        .expect("speak_one_sentence failed");
    let whole_len = whole.samples.into_vec().len();
    // The synthetic graph always outputs exactly 16000 samples (see synthetic_inference.rs).
    assert_eq!(whole_len, 16000);

    // 16 is scaled to 16 * 256 = 4096 internally by stream_synthesis.
    let stream = model
        .stream_synthesis(TEST_PHONEMES.to_string(), 16, 3, CancellationToken::new())
        .expect("stream_synthesis failed");
    let chunks: Vec<Vec<f32>> = stream
        .map(|r| r.expect("chunk synthesis failed").into_vec())
        .collect();

    assert_eq!(chunks.len(), 4, "16000 samples / 4096 chunk_size = 3 full chunks + 1 remainder");
    for chunk in &chunks[..3] {
        assert_eq!(chunk.len(), 4096);
    }
    assert_eq!(chunks[3].len(), 16000 - 3 * 4096);

    let total: usize = chunks.iter().map(|c| c.len()).sum();
    assert_eq!(total, whole_len);
}
