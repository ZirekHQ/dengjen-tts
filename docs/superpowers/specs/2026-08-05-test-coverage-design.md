# Test Coverage Remediation — Design

**Date:** 2026-08-05

## Goal

Close the test-coverage gap across the `dengjen` workspace. Baseline: 39 `#[test]` functions across ~5,000 lines in 11 crates, concentrated in 3 crates (`piper`, `espeak-phonemizer`, `audio-ops`); `core`, `grpc`, `capi`, `cli` have zero tests; no `tests/` integration directory exists anywhere; CI (`.github/workflows/CI.yml`) never runs `cargo test` — it only builds Python wheels via maturin. Real ONNX inference (`piper`'s `infer_with_values`/`infer_encoder`/`infer_decoder`/`synthesize_chunk`) has zero executed coverage in a clean checkout, since the only tests that would exercise it (`synth`'s `test_lazy_stream`/`test_parallel_stream`/`test_realtime_stream`) depend on gitignored model files not present in the repo.

Scope: all 11 workspace crates, including the FFI/sys crates (`sonic-sys`, `istft-sys`, `capi`).

## Approach: bottom-up, three tiers

The crates form a dependency chain (`audio-ops` / `espeak-phonemizer` / `sonic-sys` → `piper` → `synth` → frontends). Tier 1 needs no fixtures and starts immediately; Tiers 2–3 are blocked on shared test infrastructure and go through `writing-plans` before implementation.

### Tier 1 — write now (no fixtures needed)

Pure-logic unit tests and named error branches:

- **`audio-ops`**: `hanning_window::get_hann_window` (lookup-table + on-the-fly paths, panic on `window_length == 0`), `crossfade` (div-by-zero when `fade_samples == 1`), `to_i16_vec` (empty input, near-silent divide guard), `to_decibel`, `take_range` clamping, `merge`, `apply_hanning_window`, `Audio::real_time_factor` branches, `wave_writer` buffer round-trip (in-memory `Cursor`, no filesystem needed).
- **`espeak-phonemizer`**: empty-string input, `phoneme_separator: None` path, `DENGJEN_ESPEAKNG_DATA_DIRECTORY` override vs default (real espeak-ng lib is already a build dep; no model file needed).
- **`core`**: `DengjenError::Display` per variant, `Phonemes::Display` join behavior, default `stream_synthesis` error path.
- **`piper`**: `load_model_config` file-not-found + malformed-JSON branches, `from_config_path` invalid-filename branch, `_do_set_default_synth_config` unknown-speaker-id branch, `set_fallback_synthesis_config` downcast-failure branch, `do_phonemize_text` espeak-disabled error path, `AdaptiveMelChunker` one-shot/termination branches (pure math, no ONNX).
- **`cli`**: `SynthesisRequest::as_piper_synth_config`/`as_audio_output_config` defaults.
- **`sonic-sys`**: minimal smoke test — `sonicCreateStream`/`sonicDestroyStream` round-trip, non-null check.

### Bug fixes folded into Tier 1 (per user decision — treat flagged risks as fixes, not just documented gaps)

- **`piper`**: `VitsModel::get_input_output_info` is `todo!()` — implement it (query session input/output metadata) instead of leaving a panic; add a test.
- **`capi`**: every `unsafe extern "C"` function taking `*mut DengjenVoice`/`*mut AudioInfo` does `.as_ref().unwrap()`/`.as_mut().unwrap()`, panicking on a null pointer despite the doc contract saying "must be non-null." Replace with an explicit null check that returns a `DengjenFFIError` (there's already an error-code mapping path for this). Add unit tests asserting null input returns the mapped error instead of panicking. (Full lifecycle round-trip test stays in Tier 3.)
- **`cli`**: `SynthesisMode::from(&str)` panics on an unknown mode string instead of a clap-level parse error. Fix to return a `Result` clap can surface properly; add a test.
- **`cli`**: `get_synthesis_request_from_stdin` silently swallows malformed-JSON errors (logs and continues the loop). Change to surface the parse failure to the caller (still non-fatal to the loop, but observable/testable) instead of a silent continue; add a test.
- **`istft-sys`**: exposes no Rust API (`src/lib.rs` is empty) and has zero consumers anywhere in the workspace — it only builds and links `nanosnap` for nothing. Remove it from workspace members rather than carry dead weight. If it's intentional groundwork for future work, this should be called out explicitly before removal — flagging here for confirmation at plan time.

### Tier 2 — shared fixtures, then unlock `synth`/`piper` inference + integration tests

1. **Mock `DengjenModel`** — a test-only fake implementing the trait (test utils in `core`, or a new `dengjen-test-fixtures` dev-dependency), returning canned `Audio`/errors on demand. Unlocks `synth`'s orchestration, error-propagation (all 3 stream types), and `AudioOutputConfig::apply_to_raw_samples` empty-input/error branches — all without ONNX Runtime.
2. **Tiny vendored voice** — the smallest viable real Piper voice (or a minimal synthetic ONNX graph matching the VITS I/O contract), checked into the repo as a fixture. Unlocks `piper`'s real inference path and a cross-crate integration test chaining `espeak-phonemizer` → `map_phonemes_to_ids` → real inference → `synth`'s sonic DSP → valid WAV bytes.
3. New `tests/` integration directories (the first in the repo) for the pipeline test above.

### Tier 3 — e2e per frontend (needs Tier 2 + a fetched real-voice fixture)

- **`cli`**: subprocess test — spawn the binary, feed JSON via stdin/`-f`, assert WAV on stdout/file, assert non-zero exit on bad config/malformed JSON.
- **`grpc`**: `tonic` in-process server, exercise all 7 RPCs including `VoiceNotFound`/`invalid_argument` paths.
- **`python`**: maturin-built wheel + pytest, covering `PiperModel`/`Dengjen`/`WaveSamples` happy path + error paths (bad config, bad speaker name).
- **`capi`**: highest-risk crate — round-trip lifecycle test (load → speak → free), including the `SynthesisEvent` buffer-ownership handoff (`Box::into_raw`/`mem::forget` in `with_speech`, reconstruction in `libdengjenFreeSynthesisEvent`).

## Fixture strategy

Two fixtures, different purposes:

- **Vendored tiny voice** (Tier 2): checked into the repo, deterministic, runs in CI with no network. Used for `piper`/`synth` inference-path unit and integration tests.
- **Fetched real voice** (Tier 3): a known small full Piper voice, downloaded into a cache dir at test time for e2e smoke tests through each frontend. Skips gracefully if unavailable/offline — not a hard CI requirement.

## CI

None of this closes the gap if it doesn't run. `.github/workflows/CI.yml` currently only builds Python wheels — add a job that runs `cargo test --workspace` (respecting the README's per-package `.cargo/config` caveat: if root-level `cargo test --workspace` misbehaves for a package, that package's tests run via `cd` into it, per existing project convention) as part of Tier 1 completion.

## Out of scope

- Full ONNX Runtime fuzzing/property testing of inference numerics — covered only at the level of "does the pipeline run and produce valid output," not "is the audio correct."
- Windows/macOS-specific CI test execution — this design covers what tests exist and where fixtures live; wiring them into the existing per-OS CI matrix is a follow-up.
