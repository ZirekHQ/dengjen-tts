mod dev_utils;

use dengjen_synth::DengjenResult;
use std::{io::Write, path::PathBuf, sync::Arc};

#[test]
fn test_lazy_stream() -> DengjenResult<()> {
    let (synthesizer, text, config) = dev_utils::gen_params("std");
    let synthesis_stream = synthesizer
        .synthesize_lazy(text, config)?
        .map(|result| result.map(|chunk| chunk.samples));
    dev_utils::iterate_stream(synthesis_stream)
}

#[test]
fn test_parallel_stream() -> DengjenResult<()> {
    let (synthesizer, text, config) = dev_utils::gen_params("std");
    let synthesis_stream = synthesizer
        .synthesize_parallel(text, config)?
        .map(|result| result.map(|chunk| chunk.samples));
    dev_utils::iterate_stream(synthesis_stream)
}

#[test]
fn test_realtime_stream() -> DengjenResult<()> {
    let (synthesizer, text, config) = dev_utils::gen_params("rt");
    let token = dengjen_core::CancellationToken::new();
    let synthesis_stream = synthesizer.synthesize_streamed(
        text,
        config,
        72,
        3,
        token,
    )?;
    dev_utils::iterate_stream(synthesis_stream)
}

const KOKORO_STYLE_DIM: usize = 256;
const KOKORO_MAX_TOKEN_LEN: usize = 510;

fn write_synthetic_kokoro_voice_file(dir: &std::path::Path, voice_name: &str) {
    let path = dir.join(format!("{voice_name}.bin"));
    let mut bytes = Vec::with_capacity(KOKORO_MAX_TOKEN_LEN * KOKORO_STYLE_DIM * 4);
    for row in 0..KOKORO_MAX_TOKEN_LEN {
        for _ in 0..KOKORO_STYLE_DIM {
            bytes.extend_from_slice(&(row as f32).to_le_bytes());
        }
    }
    std::fs::write(&path, &bytes).unwrap();
}

fn write_minimal_kokoro_vocab(dir: &std::path::Path) -> PathBuf {
    let path = dir.join("tokenizer.json");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(r#"{"model": {"vocab": {"$": 0, "t": 1, "ɛ": 2, "s": 3}}}"#.as_bytes())
        .unwrap();
    path
}

fn load_synthetic_kokoro_model() -> dengjen_kokoro::KokoroModel {
    let dir = std::env::temp_dir().join("dengjen_synth_kokoro_realtime_stream_test");
    std::fs::create_dir_all(&dir).unwrap();
    let voices_dir = dir.join("voices");
    std::fs::create_dir_all(&voices_dir).unwrap();
    write_synthetic_kokoro_voice_file(&voices_dir, "test_voice");
    let vocab_path = write_minimal_kokoro_vocab(&dir);

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let model_path = manifest_dir.join("../models/kokoro/tests/fixtures/synthetic_kokoro.onnx");

    let config = dengjen_kokoro::KokoroVoiceConfig {
        model_path,
        voices_dir,
        vocab_path,
        sample_rate: 24000,
        voices: vec!["test_voice".to_string()],
    };
    let model =
        dengjen_kokoro::KokoroModel::from_config(config).expect("failed to load synthetic Kokoro model");
    std::fs::remove_dir_all(&dir).ok();
    model
}

// Regression guard for the bug fixed alongside this test: Kokoro's `stream_synthesis` used to
// treat `chunk_size` as a raw sample count, even though every real caller tunes it as a
// Piper-style mel-frame count. Against capi's real default of 72, that produced
// ceil(16000 / 72) = 223 tiny ~3ms chunks per sentence. With the fix, Kokoro scales
// `chunk_size` by a nominal hop of 256 internally, so 72 -> 72 * 256 = 18432, which exceeds the
// synthetic fixture's fixed 16000-sample output - landing the whole sentence in one chunk.
#[test]
fn kokoro_realtime_stream_uses_realistic_chunk_duration_for_capi_default_chunk_size() {
    let model = load_synthetic_kokoro_model();
    let model: Arc<dyn dengjen_core::DengjenModel + Send + Sync> = Arc::new(model);
    let synth = dengjen_synth::DengjenSpeechSynthesizer::new(model).unwrap();

    let stream = synth.synthesize_streamed(
        "t\u{025b}st".to_string(),
        None,
        72,
        3,
        dengjen_core::CancellationToken::new(),
    );
    let stream = match stream {
        Ok(stream) => stream,
        Err(dengjen_core::DengjenError::PhonemizationError(msg))
            if msg.contains("Failed to initialize eSpeak-ng") =>
        {
            eprintln!(
                "Skipping kokoro_realtime_stream_uses_realistic_chunk_duration_for_capi_default_chunk_size: \
                 espeak-ng data not available on this machine."
            );
            return;
        }
        Err(e) => panic!("synthesize_streamed failed unexpectedly: {e:?}"),
    };

    let chunks: Vec<Vec<f32>> = stream
        .map(|r| r.expect("chunk synthesis failed").into_vec())
        .collect();

    assert_eq!(chunks.len(), 1, "expected the whole sentence to land in a single chunk");
    assert_eq!(chunks[0].len(), 16000);
}
