use std::path::PathBuf;
use std::process::Command;

// Safety net for the Piper clean-room rewrite (Task 11 of the model-backends
// rewrite plan): drives the real `dengjen` binary against synthetic
// batch and streaming ("realtime") Piper voices, so a behavior regression in
// the rewritten inference/chunking internals shows up as a CLI-level failure,
// not just a unit-test failure whose surface area the rewrite might also
// have changed.
//
// The realtime test deliberately does NOT pass `-o`/`--output-file`: the CLI's
// `process_synthesis_request` (crates/frontends/cli/src/main.rs) takes an early
// return whenever an output file is given, always routing through
// `synthesize_to_file` -> `synthesize_parallel` -> `speak_one_sentence`, which
// performs a single one-shot encoder+decoder pass regardless of `--mode`,
// `--chunk-size`, or `--chunk-padding`. Only the no-output-file path reaches
// `synthesize_streamed` -> `stream_synthesis` -> `SpeechStreamer`, which is the
// code that actually invokes the decoder once per chunk via
// `AdaptiveMelChunker`. Reading WAV bytes off stdout instead is what makes this
// test exercise the multi-chunk decoder path Task 11 is about to rewrite.

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../dengjen/models/piper/tests/fixtures")
}

fn minimal_phoneme_id_map() -> &'static str {
    r#"{"^": [1], "$": [2], "_": [3], "t": [4], "ɛ": [5], "s": [6]}"#
}

fn write_batch_config(dir: &std::path::Path) -> PathBuf {
    let model_path = fixtures_dir().join("synthetic_piper_batch.onnx");
    assert!(model_path.exists(), "missing fixture at {}", model_path.display());
    let config_path = dir.join("synthetic_piper_batch.onnx.json");
    std::fs::write(
        &config_path,
        format!(
            r#"{{
                "key": null,
                "language": {{"code": "en-US"}},
                "audio": {{"sample_rate": 22050, "quality": null}},
                "num_speakers": 1,
                "speaker_id_map": {{}},
                "streaming": false,
                "espeak": {{"voice": "en-us"}},
                "inference": {{"noise_scale": 0.667, "length_scale": 1.0, "noise_w": 0.8}},
                "num_symbols": 8,
                "phoneme_map": {{}},
                "phoneme_id_map": {phoneme_map},
                "phoneme_type": "text",
                "hop_length": 256
            }}"#,
            phoneme_map = minimal_phoneme_id_map()
        ),
    )
    .unwrap();
    std::fs::copy(&model_path, dir.join("synthetic_piper_batch.onnx")).unwrap();
    config_path
}

fn write_streaming_config(dir: &std::path::Path) -> PathBuf {
    let encoder_path = fixtures_dir().join("synthetic_piper_encoder.onnx");
    let decoder_path = fixtures_dir().join("synthetic_piper_decoder.onnx");
    assert!(encoder_path.exists(), "missing fixture at {}", encoder_path.display());
    assert!(decoder_path.exists(), "missing fixture at {}", decoder_path.display());
    std::fs::copy(&encoder_path, dir.join("encoder.onnx")).unwrap();
    std::fs::copy(&decoder_path, dir.join("decoder.onnx")).unwrap();
    let config_path = dir.join("synthetic_piper_streaming.onnx.json");
    std::fs::write(
        &config_path,
        format!(
            r#"{{
                "key": null,
                "language": {{"code": "en-US"}},
                "audio": {{"sample_rate": 22050, "quality": null}},
                "num_speakers": 1,
                "speaker_id_map": {{}},
                "streaming": true,
                "espeak": {{"voice": "en-us"}},
                "inference": {{"noise_scale": 0.667, "length_scale": 1.0, "noise_w": 0.8}},
                "num_symbols": 8,
                "phoneme_map": {{}},
                "phoneme_id_map": {phoneme_map},
                "phoneme_type": "text",
                "hop_length": 256
            }}"#,
            phoneme_map = minimal_phoneme_id_map()
        ),
    )
    .unwrap();
    config_path
}

fn run_cli(
    config_path: &std::path::Path,
    input_path: &std::path::Path,
    output_path: Option<&std::path::Path>,
    extra_args: &[&str],
) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_dengjen"));
    cmd.arg(config_path).arg("-f").arg(input_path);
    if let Some(output_path) = output_path {
        cmd.arg("-o").arg(output_path);
    }
    cmd.args(extra_args);
    cmd.output().expect("failed to spawn the dengjen binary")
}

#[test]
fn cli_synthesizes_batch_from_a_synthetic_piper_voice() {
    let dir = std::env::temp_dir().join("dengjen_cli_piper_synthetic_batch_test");
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let config_path = write_batch_config(&dir);
    let input_path = dir.join("input.txt");
    std::fs::write(&input_path, "test").unwrap();
    let output_path = dir.join("output.wav");

    let output = run_cli(&config_path, &input_path, Some(&output_path), &[]);

    assert!(
        output.status.success(),
        "CLI exited with failure: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let wav_bytes = std::fs::read(&output_path).expect("expected the CLI to write an output WAV");
    assert!(!wav_bytes.is_empty(), "expected non-empty WAV bytes");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn cli_streams_realtime_synthesis_from_a_synthetic_piper_voice() {
    // ENCODER_NUM_FRAMES=200 in generate_synthetic_piper.py, chunk_size=20,
    // chunk_padding=3: one_shot = num_frames <= (chunk_size*2 + chunk_padding*2)
    // = 200 <= 46 = false, so SpeechStreamer::next() takes the `synthesize_chunk`
    // branch (not the one_shot shortcut) and AdaptiveMelChunker yields more than
    // one (mel_index, audio_index) pair, meaning the decoder is invoked more than
    // once with differently-sized `z`/`y_mask` slices. This is exactly the path
    // that would panic on a decoder whose output size doesn't scale with its
    // input's time dimension.
    let dir = std::env::temp_dir().join("dengjen_cli_piper_synthetic_streaming_test");
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let config_path = write_streaming_config(&dir);
    let input_path = dir.join("input.txt");
    std::fs::write(&input_path, "test").unwrap();

    // No `-o`: passing an output file makes the CLI take a code path
    // (`synthesize_to_file` -> `synthesize_parallel`) that always does a single
    // one-shot encoder+decoder pass and ignores --mode/--chunk-size/--chunk-padding
    // entirely. Reading WAV bytes from stdout instead exercises the real
    // `synthesize_streamed` -> `stream_synthesis` -> `SpeechStreamer` path.
    let output = run_cli(
        &config_path,
        &input_path,
        None,
        &["--mode", "realtime", "--chunk-size", "20", "--chunk-padding", "3"],
    );

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "CLI exited with failure: stderr={stderr}"
    );
    assert!(
        !stderr.contains("panicked"),
        "CLI panicked during streaming synthesis: {stderr}"
    );
    assert!(!output.stdout.is_empty(), "expected non-empty WAV bytes on stdout");

    std::fs::remove_dir_all(&dir).ok();
}
