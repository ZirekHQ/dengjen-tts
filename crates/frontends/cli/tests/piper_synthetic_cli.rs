use std::path::PathBuf;
use std::process::{Command, Output};

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
// `AdaptiveMelChunker`. Reading raw PCM bytes off stdout instead is what makes
// this test exercise the multi-chunk decoder path Task 11 is about to rewrite.
// (Unlike the batch test's `-o` output file, which goes through
// `write_wave_samples_to_file` and is a real RIFF/WAVE file, stdout in
// streaming mode carries headerless little-endian i16 PCM from
// `AudioSamples::as_wave_bytes` — the name is misleading but there is no
// RIFF header on this path.)

// Must match ENCODER_NUM_FRAMES / HOP_LENGTH in generate_synthetic_piper.py,
// and HOP_LENGTH must match the "hop_length" field in both config JSONs below.
const ENCODER_NUM_FRAMES: usize = 200;
const HOP_LENGTH: usize = 256;
const EXPECTED_STREAM_PCM_BYTES: usize = ENCODER_NUM_FRAMES * HOP_LENGTH * 2; // i16 = 2 bytes/sample

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../dengjen/models/piper/tests/fixtures")
}

fn minimal_phoneme_id_map() -> &'static str {
    r#"{"^": [1], "$": [2], "_": [3], "t": [4], "ɛ": [5], "s": [6]}"#
}

fn write_batch_config(dir: &std::path::Path) -> PathBuf {
    let model_path = fixtures_dir().join("synthetic_piper_batch.onnx");
    assert!(
        model_path.exists(),
        "missing fixture at {}",
        model_path.display()
    );
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
                "hop_length": {hop_length}
            }}"#,
            phoneme_map = minimal_phoneme_id_map(),
            hop_length = HOP_LENGTH,
        ),
    )
    .unwrap();
    std::fs::copy(&model_path, dir.join("synthetic_piper_batch.onnx")).unwrap();
    config_path
}

fn write_streaming_config(dir: &std::path::Path) -> PathBuf {
    let encoder_path = fixtures_dir().join("synthetic_piper_encoder.onnx");
    let decoder_path = fixtures_dir().join("synthetic_piper_decoder.onnx");
    assert!(
        encoder_path.exists(),
        "missing fixture at {}",
        encoder_path.display()
    );
    assert!(
        decoder_path.exists(),
        "missing fixture at {}",
        decoder_path.display()
    );
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
                "hop_length": {hop_length}
            }}"#,
            phoneme_map = minimal_phoneme_id_map(),
            hop_length = HOP_LENGTH,
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
) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_dengjen"));
    cmd.arg(config_path).arg("-f").arg(input_path);
    if let Some(output_path) = output_path {
        cmd.arg("-o").arg(output_path);
    }
    cmd.args(extra_args);
    cmd.output().expect("failed to spawn the dengjen binary")
}

fn run_streaming(
    config_path: &std::path::Path,
    input_path: &std::path::Path,
    chunk_size: &str,
) -> Output {
    run_cli(
        config_path,
        input_path,
        None,
        &[
            "--mode",
            "realtime",
            "--chunk-size",
            chunk_size,
            "--chunk-padding",
            "3",
        ],
    )
}

fn assert_streaming_success(output: &Output, label: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "CLI exited with failure ({label}): stderr={stderr}"
    );
    assert!(
        !stderr.contains("panicked"),
        "CLI panicked during streaming synthesis ({label}): {stderr}"
    );
    assert_eq!(
        output.stdout.len(),
        EXPECTED_STREAM_PCM_BYTES,
        "unexpected stdout PCM byte count ({label}): expected \
         ENCODER_NUM_FRAMES * HOP_LENGTH * sizeof(i16) = {EXPECTED_STREAM_PCM_BYTES}"
    );
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
    // BATCH_OUTPUT_SAMPLES=8000 in generate_synthetic_piper.py: a real 44-byte
    // RIFF/WAVE header (write_wave_samples_to_file -> riff-wave) plus 8000 mono
    // i16 samples.
    const BATCH_OUTPUT_SAMPLES: usize = 8000;
    let expected_len = 44 + BATCH_OUTPUT_SAMPLES * 2;
    assert_eq!(
        wav_bytes.len(),
        expected_len,
        "expected a {expected_len}-byte WAV file (44-byte header + {BATCH_OUTPUT_SAMPLES} i16 samples)"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn cli_streams_realtime_synthesis_from_a_synthetic_piper_voice() {
    // ENCODER_NUM_FRAMES=200, chunk_padding=3:
    //   --chunk-size 20:  one_shot = 200 <= (20*2 + 3*2)  = 200 <= 46  = false (chunked)
    //   --chunk-size 100: one_shot = 200 <= (100*2 + 3*2) = 200 <= 206 = true  (single decode)
    // The synthetic decoder's output is a position-dependent ramp (0, 1, 2, ...)
    // sized to its z input's time dimension, not a constant: a one-shot decode
    // (single continuous ramp) and a genuinely chunked decode (several shorter
    // ramps, each restarting at 0 for its own chunk, then crossfaded) are
    // therefore byte-distinguishable on stdout. This is deliberately exercised
    // below by running both chunk sizes against the same input and asserting
    // the outputs differ - if Task 11's rewrite ever collapses streaming into
    // one-shot, or the decoder stops reacting to its input's time dimension,
    // these two runs would become byte-identical and this assertion would catch it.
    let dir = std::env::temp_dir().join("dengjen_cli_piper_synthetic_streaming_test");
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let config_path = write_streaming_config(&dir);
    let input_path = dir.join("input.txt");
    std::fs::write(&input_path, "test").unwrap();

    // No `-o`: passing an output file makes the CLI take a code path
    // (`synthesize_to_file` -> `synthesize_parallel`) that always does a single
    // one-shot encoder+decoder pass and ignores --mode/--chunk-size/--chunk-padding
    // entirely. Reading PCM bytes from stdout instead exercises the real
    // `synthesize_streamed` -> `stream_synthesis` -> `SpeechStreamer` path.
    let chunked_output = run_streaming(&config_path, &input_path, "20");
    assert_streaming_success(&chunked_output, "chunk-size=20 (chunked)");

    let one_shot_output = run_streaming(&config_path, &input_path, "100");
    assert_streaming_success(&one_shot_output, "chunk-size=100 (one-shot)");

    assert_ne!(
        chunked_output.stdout, one_shot_output.stdout,
        "chunked (--chunk-size 20) and one-shot (--chunk-size 100) streaming runs produced \
         byte-identical output; the decoder fixture should make these paths distinguishable"
    );

    std::fs::remove_dir_all(&dir).ok();
}
