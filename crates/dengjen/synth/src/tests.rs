mod dev_utils;

use dengjen_tts::DengjenResult;
use std::{path::PathBuf, sync::Arc};

#[test]
fn test_lazy_stream() -> DengjenResult<()> {
    let (synthesizer, text, config) = dev_utils::gen_params("std");
    let stream = synthesizer
        .synthesize_lazy(text, config)?
        .map(|chunk| chunk.map(|c| c.samples));
    dev_utils::iterate_stream(stream)
}

#[test]
fn test_parallel_stream() -> DengjenResult<()> {
    let (synthesizer, text, config) = dev_utils::gen_params("std");
    let stream = synthesizer
        .synthesize_parallel(text, config)?
        .map(|chunk| chunk.map(|c| c.samples));
    dev_utils::iterate_stream(stream)
}

#[test]
fn test_realtime_stream() -> DengjenResult<()> {
    let (synthesizer, text, config) = dev_utils::gen_params("rt");
    let cancel = dengjen_tts_core::CancellationToken::new();
    let stream = synthesizer.synthesize_streamed(text, config, 72, 3, cancel)?;
    dev_utils::iterate_stream(stream)
}

const KOKORO_VOICE_STYLE_ROWS: usize = 510;
const KOKORO_VOICE_STYLE_COLS: usize = 256;

fn synthetic_voice_style_bytes() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(KOKORO_VOICE_STYLE_ROWS * KOKORO_VOICE_STYLE_COLS * 4);
    for row in 0..KOKORO_VOICE_STYLE_ROWS {
        let value = (row as f32).to_le_bytes();
        for _ in 0..KOKORO_VOICE_STYLE_COLS {
            bytes.extend_from_slice(&value);
        }
    }
    bytes
}

fn write_synthetic_voice(voices_dir: &std::path::Path, name: &str) {
    std::fs::write(
        voices_dir.join(format!("{name}.bin")),
        synthetic_voice_style_bytes(),
    )
    .expect("failed to write synthetic voice fixture");
}

fn write_minimal_vocab(dir: &std::path::Path) -> PathBuf {
    let path = dir.join("tokenizer.json");
    let vocab = r#"{"model": {"vocab": {"$": 0, "t": 1, "ɛ": 2, "s": 3}}}"#;
    std::fs::write(&path, vocab).expect("failed to write synthetic vocab fixture");
    path
}

/// Builds a synthetic Kokoro model backed by a unique temp-dir fixture. The returned
/// `TempDir` must be kept alive for as long as the model: `KokoroVoiceConfig` retains
/// `voices_dir`/`vocab_path`, so dropping the guard early (or reusing a fixed path across
/// concurrent test runs) can pull the fixture out from under the model.
fn build_synthetic_kokoro_model() -> (dengjen_tts_kokoro::KokoroModel, tempfile::TempDir) {
    let root = tempfile::tempdir().expect("failed to create fixture temp dir");
    let voices_dir = root.path().join("voices");
    std::fs::create_dir_all(&voices_dir).expect("failed to create fixture voices dir");
    write_synthetic_voice(&voices_dir, "test_voice");
    let vocab_path = write_minimal_vocab(root.path());

    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../models/kokoro/tests/fixtures/synthetic_kokoro.onnx");

    let config = dengjen_tts_kokoro::KokoroVoiceConfig {
        model_path,
        voices_dir,
        vocab_path,
        sample_rate: 24000,
        voices: vec!["test_voice".to_string()],
    };
    let model = dengjen_tts_kokoro::KokoroModel::from_config(config)
        .expect("failed to build synthetic Kokoro model");
    (model, root)
}

// Regression guard: chunk_size must be interpreted as a mel-frame count, not a raw
// sample count, so capi's default of 72 doesn't fragment every sentence into tiny chunks.
#[test]
fn kokoro_realtime_stream_uses_realistic_chunk_duration_for_capi_default_chunk_size() {
    let (model, _fixture_dir) = build_synthetic_kokoro_model();
    let model: Arc<dyn dengjen_tts_core::DengjenModel + Send + Sync> = Arc::new(model);
    let synthesizer = dengjen_tts::DengjenSpeechSynthesizer::new(model).unwrap();

    let stream = synthesizer.synthesize_streamed(
        "t\u{025b}st".to_string(),
        None,
        72,
        3,
        dengjen_tts_core::CancellationToken::new(),
    );
    let stream = match stream {
        Ok(stream) => stream,
        Err(dengjen_tts_core::DengjenError::PhonemizationError(msg))
            if msg.contains("Failed to initialize eSpeak-ng") =>
        {
            eprintln!(
                "skipping kokoro_realtime_stream_uses_realistic_chunk_duration_for_capi_default_chunk_size: espeak-ng data unavailable on this machine"
            );
            return;
        }
        Err(e) => panic!("synthesize_streamed failed unexpectedly: {e:?}"),
    };

    let chunks: Vec<Vec<f32>> = stream
        .map(|chunk| chunk.expect("chunk synthesis failed").into_vec())
        .collect();

    assert_eq!(
        chunks.len(),
        1,
        "expected the whole sentence to land in a single chunk"
    );
    assert_eq!(chunks[0].len(), 16000);
}
