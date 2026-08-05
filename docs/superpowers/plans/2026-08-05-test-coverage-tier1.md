# Test Coverage Tier 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the "write now, no fixtures needed" tier of the test-coverage gap identified in `docs/superpowers/specs/2026-08-05-test-coverage-design.md` — pure-logic unit tests and named error branches across 7 crates, plus the three approved bug fixes (piper `todo!()` panic deferred to Tier 2 — see note in Task 8; capi null-pointer unwraps; cli `SynthesisMode` panic), plus wiring `cargo test` into CI for the first time.

**Architecture:** No application architecture changes. Adds `#[cfg(test)] mod tests` blocks (or extends existing ones) to 8 source files, adds two small production-code safety fixes (capi null checks, cli `FromStr` impl), removes one dead workspace member (`istft-sys`), and adds a `test` job to `.github/workflows/CI.yml`.

**Tech Stack:** Rust workspace (resolver 2), `cargo test`, no new dependencies.

## Global Constraints

- Every new test must be runnable with a plain `\cargo test -p <crate>` (or the documented feature-flag variant) — no new fixtures, no network access, no real ONNX model files. Anything that needs those belongs to Tier 2/3, not this plan.
- Toolchain: `source "$HOME/.cargo/env"` and prepend `$HOME/.cargo/bin` to `PATH` before every `cargo` invocation in this shell. Always call the real binary with a leading `\cargo` — the `rtk` hook intercepts plain `cargo` invocations and returns a canned summary instead of real output.
- `espeak-phonemizer` tests require `espeak-ng-data` on disk and are **not** safe to run with the default parallel test runner — the underlying espeak-ng C library is not thread-safe across the process-global `ESPEAKNG_INIT`. Always run this crate's tests with `-- --test-threads=1` and `DENGJEN_ESPEAKNG_DATA_DIRECTORY` set to the parent directory that contains an `espeak-ng-data` folder (verified locally at `/usr/lib/x86_64-linux-gnu`; find yours with `find /usr -maxdepth 4 -type d -name espeak-ng-data`). This is a pre-existing repo characteristic, not something this plan changes.
- Don't add a `tempfile` (or similar) dev-dependency — use `std::env::temp_dir()` + `std::process::id()` for any test that needs a throwaway file, and clean it up in the same test.
- Follow existing test-module conventions: append new `#[test]` fns to a crate's existing `#[cfg(test)] mod tests { use super::*; ... }` block where one exists; add a new one (same shape) where it doesn't.
- Don't touch the `deps/nanosnap` git submodule or `.gitmodules` — Task 13 only removes the `istft-sys` *crate* (workspace member + directory), not the vendored submodule it built against.

## Deviations from the design doc

Reading the actual source surfaced a few spec items that turned out not to be Tier-1-testable (need a real `ort::Session`, i.e. an ONNX file) or not to be real bugs. Rather than force placeholder tests or silently drop them, they're listed here and moved/retracted explicitly:

- **`piper`'s `from_config_path` invalid-filename branch** (`config_path.file_stem() == None`) — dropped entirely. It can only be reached by a `Path` with no file name (e.g. `/` or `..`), which can't also be an openable regular file that `load_model_config` (called first) would successfully parse — the branch is unreachable through any realistic input.
- **`piper`'s `set_fallback_synthesis_config` downcast-failure branch** and **`VitsModel::get_input_output_info` (`todo!()`)** — both require a real `VitsModel` instance, which requires a `Session`, which requires an `.onnx` file. Moved to Tier 2 (the design doc already scoped `get_input_output_info` as a Tier 1 fix, but implementing *and verifying* it needs the vendored fixture — deferred along with the test).
- **`piper`'s `SpeechStreamer` one-shot short-circuit flag** — same reason (`SpeechStreamer::new` takes a `Session`-backed `EncoderOutputs`). Moved to Tier 2; `AdaptiveMelChunker`'s own termination logic (Task 7) is independently Tier-1-testable since it doesn't touch `Session`.
- **`espeak-phonemizer`'s `phoneme_separator: None` path** — already exercised by every existing test that doesn't pass a separator (e.g. `test_basic_en`); adding a dedicated test would be redundant. **The `DENGJEN_ESPEAKNG_DATA_DIRECTORY` override-vs-default branch** — untestable within a shared test binary: `ESPEAKNG_INIT` is a process-global `Lazy` computed once from whatever the env var is at first use, and `cargo test`'s default parallel execution makes "first use" non-deterministic across tests. Both dropped.
- **`cli`'s silently-swallowed stdin JSON-parse error** — on closer reading this isn't a bug: `get_synthesis_request_from_stdin` errors are caught and `log::error!`-reported before the read loop continues, which is reasonable behavior for a long-running stdin request loop (one malformed request shouldn't kill the session). The design doc's "fix" framing was wrong; retracted. No task added for it. (It also isn't unit-testable without refactoring the function to accept an injectable `Read` instead of live stdin, which is out of scope here.)

---

### Task 1: `audio-ops` — Hann window coverage

**Files:**
- Modify: `crates/audio/ops/src/hanning_window.rs`

**Interfaces:** N/A — self-contained, tests only, no production code changes.

- [ ] **Step 1: Add a `#[cfg(test)] mod tests` block at the end of the file**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "Invalid window length: 0")]
    fn get_hann_window_panics_on_zero_length() {
        get_hann_window(0);
    }

    #[test]
    fn get_hann_window_starts_and_ends_at_zero_and_peaks_near_the_center() {
        let window = get_hann_window(64);
        assert_eq!(window.len(), 64);
        assert_eq!(window[0], 0.0);
        assert_eq!(window[63], 0.0);
        let max = window.iter().cloned().fold(f32::MIN, f32::max);
        let max_index = window.iter().position(|&v| v == max).unwrap();
        assert!((28..36).contains(&max_index), "expected peak near center, got index {max_index}");
    }

    #[test]
    fn get_hann_window_lookup_table_matches_direct_computation() {
        // 64 is one of the precomputed lengths; verify the cached table entry
        // wasn't corrupted or stored under the wrong key.
        assert_eq!(get_hann_window(64), calculate_hann_window(64));
    }

    #[test]
    fn get_hann_window_computes_on_the_fly_for_a_length_not_in_the_lookup_table() {
        // 10 is not one of the precomputed lengths (64, 128, 256, 512, 1024, 2048, 4096).
        let window = get_hann_window(10);
        assert_eq!(window.len(), 10);
        assert_eq!(window[0], 0.0);
        assert_eq!(window[9], 0.0);
    }
}
```

- [ ] **Step 2: Run the new tests**

Run: `cd /home/ali/Workspace/lang/dengjen && source "$HOME/.cargo/env" && \cargo test -p audio-ops hanning_window::tests -- --nocapture`
Expected: `test result: ok. 4 passed; 0 failed`

- [ ] **Step 3: Commit**

```bash
cd /home/ali/Workspace/lang/dengjen
git add crates/audio/ops/src/hanning_window.rs
git commit -m "Add test coverage for get_hann_window's panic, peak shape, and lookup-table paths"
```

---

### Task 2: `audio-ops` — `AudioSamples` pure-logic gap coverage

**Files:**
- Modify: `crates/audio/ops/src/samples.rs` (append to the existing `mod tests` block, currently at the end of the file)

**Interfaces:** N/A — self-contained, tests only.

- [ ] **Step 1: Add the following tests inside the existing `#[cfg(test)] mod tests { ... }` block in `samples.rs`, alongside `test_strip_silence`**

```rust
    #[test]
    fn to_i16_vec_returns_empty_for_empty_samples() {
        let samples = AudioSamples::from(Vec::<f32>::new());
        assert_eq!(samples.to_i16_vec(), Vec::<i16>::new());
    }

    #[test]
    fn to_i16_vec_scales_all_zero_samples_without_dividing_by_zero() {
        let samples = AudioSamples::from(vec![0.0, 0.0, 0.0]);
        assert_eq!(samples.to_i16_vec(), vec![0, 0, 0]);
    }

    #[test]
    fn take_range_clamps_end_to_available_length() {
        let mut samples = AudioSamples::from(vec![1.0, 2.0, 3.0]);
        let taken = samples.take_range(1..100);
        assert_eq!(taken, vec![2.0, 3.0]);
        assert_eq!(samples.len(), 1);
    }

    #[test]
    fn merge_appends_other_samples_in_order() {
        let mut a = AudioSamples::from(vec![1.0, 2.0]);
        let b = AudioSamples::from(vec![3.0, 4.0]);
        a.merge(b);
        assert_eq!(a.into_vec(), vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn apply_hanning_window_tapers_first_sample_to_zero() {
        let mut samples = AudioSamples::from(vec![1.0; 10]);
        samples.apply_hanning_window();
        let v = samples.as_vec();
        assert_eq!(v[0], 0.0);
        assert!(v[5] > v[0]);
    }

    #[test]
    fn crossfade_attenuates_both_edges_symmetrically_and_leaves_the_middle_untouched() {
        let mut samples = AudioSamples::from(vec![1.0; 10]);
        samples.crossfade(4);
        let v = samples.as_vec();
        assert_eq!(v[0], v[9]);
        assert_eq!(v[1], v[8]);
        assert!(v[0] < 1.0);
        assert_eq!(v[4], 1.0);
        assert_eq!(v[5], 1.0);
    }

    #[test]
    fn crossfade_clamps_fade_length_to_half_of_total_samples() {
        let mut samples = AudioSamples::from(vec![1.0; 6]);
        samples.crossfade(100);
        let v = samples.as_vec();
        assert_eq!(v.len(), 6);
        assert!(v.iter().all(|f| f.is_finite()));
    }

    // KNOWN GAP (see docs/superpowers/specs/2026-08-05-test-coverage-design.md and this
    // plan's Task 2 commit): crossfade divides by (fade_samples - 1), so fade_samples <= 1
    // produces NaN instead of a defined result. This test characterizes the current
    // behavior; it is not a fix. Flag to the user before changing it, since callers may
    // depend on the current (broken) shape.
    #[test]
    fn crossfade_with_one_fade_sample_currently_produces_nan() {
        let mut samples = AudioSamples::from(vec![1.0, 2.0, 3.0, 4.0]);
        samples.crossfade(1);
        assert!(samples.as_vec()[0].is_nan());
    }

    #[test]
    fn to_decibel_converts_full_scale_amplitude_to_zero_db() {
        let samples = AudioSamples::from(vec![1.0, 0.5]);
        let db = samples.to_decibel();
        assert_eq!(db[0], 0.0);
        assert!(db[1] < 0.0);
    }

    #[test]
    fn to_decibel_of_zero_amplitude_is_negative_infinity() {
        let samples = AudioSamples::from(vec![0.0]);
        assert_eq!(samples.to_decibel()[0], f32::NEG_INFINITY);
    }
```

- [ ] **Step 2: Run the new tests**

Run: `cd /home/ali/Workspace/lang/dengjen && source "$HOME/.cargo/env" && \cargo test -p audio-ops samples::tests -- --nocapture`
Expected: `test result: ok. 17 passed; 0 failed` (7 existing + 10 new)

- [ ] **Step 3: Commit**

```bash
cd /home/ali/Workspace/lang/dengjen
git add crates/audio/ops/src/samples.rs
git commit -m "Add test coverage for AudioSamples empty/zero-input, clamping, and decibel edge cases"
```

---

### Task 3: `audio-ops` — `Audio::real_time_factor` and `wave_writer` coverage

**Files:**
- Modify: `crates/audio/ops/src/samples.rs` (append to existing `mod tests`)
- Modify: `crates/audio/ops/src/wave_writer.rs` (add new `mod tests`)

**Interfaces:** N/A — self-contained, tests only.

- [ ] **Step 1: Add these tests to `samples.rs`'s existing `mod tests` block**

```rust
    #[test]
    fn real_time_factor_returns_none_without_inference_time() {
        let audio = Audio::new(AudioSamples::from(vec![0.0; 100]), 100, None);
        assert_eq!(audio.real_time_factor(), None);
    }

    #[test]
    fn real_time_factor_returns_zero_for_zero_duration_audio() {
        let audio = Audio::new(AudioSamples::from(Vec::new()), 100, Some(5.0));
        assert_eq!(audio.real_time_factor(), Some(0.0));
    }

    #[test]
    fn real_time_factor_divides_inference_ms_by_duration_ms() {
        // 100 samples @ 100Hz = 1000ms duration; 50ms inference => rtf 0.05
        let audio = Audio::new(AudioSamples::from(vec![0.0; 100]), 100, Some(50.0));
        assert_eq!(audio.real_time_factor(), Some(0.05));
    }
```

- [ ] **Step 2: Run those three tests**

Run: `cd /home/ali/Workspace/lang/dengjen && source "$HOME/.cargo/env" && \cargo test -p audio-ops real_time_factor -- --nocapture`
Expected: `test result: ok. 3 passed; 0 failed`

- [ ] **Step 3: Add a `mod tests` block to `wave_writer.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_wave_samples_to_buffer_produces_a_valid_riff_wave_header() {
        let samples: Vec<i16> = vec![0, 100, -100, 32767, -32768];
        let mut buf: Vec<u8> = Vec::new();
        let result = write_wave_samples_to_buffer(
            std::io::Cursor::new(&mut buf),
            samples.iter(),
            22050,
            1,
            2,
        );
        assert!(result.is_ok());
        assert_eq!(&buf[0..4], b"RIFF");
        assert_eq!(&buf[8..12], b"WAVE");
    }

    #[test]
    fn write_wave_samples_to_file_errors_when_parent_directory_does_not_exist() {
        let path = Path::new("/nonexistent-dengjen-test-dir-xyz/out.wav");
        let samples: Vec<i16> = vec![0, 1, 2];
        let result = write_wave_samples_to_file(path, samples.iter(), 22050, 1, 2);
        assert!(result.is_err());
    }
}
```

- [ ] **Step 4: Run the wave_writer tests**

Run: `cd /home/ali/Workspace/lang/dengjen && source "$HOME/.cargo/env" && \cargo test -p audio-ops wave_writer::tests -- --nocapture`
Expected: `test result: ok. 2 passed; 0 failed`

- [ ] **Step 5: Commit**

```bash
cd /home/ali/Workspace/lang/dengjen
git add crates/audio/ops/src/samples.rs crates/audio/ops/src/wave_writer.rs
git commit -m "Add test coverage for Audio::real_time_factor branches and wave_writer round-trip/error path"
```

---

### Task 4: `espeak-phonemizer` — empty-input coverage

**Files:**
- Modify: `crates/text/espeak-phonemizer/src/lib.rs` (append to existing `mod tests`)

**Interfaces:** N/A — self-contained, tests only.

- [ ] **Step 1: Add this test to the existing `mod tests` block**

```rust
    #[test]
    fn test_empty_input_returns_no_phonemes() -> ESpeakResult<()> {
        let phonemes = text_to_phonemes("", "en-US", None, false, false)?;
        assert_eq!(phonemes, Vec::<String>::new());
        Ok(())
    }
```

- [ ] **Step 2: Run the full crate test suite (single-threaded, with the espeak-ng data dir set — see Global Constraints)**

Run: `cd /home/ali/Workspace/lang/dengjen && source "$HOME/.cargo/env" && DENGJEN_ESPEAKNG_DATA_DIRECTORY=/usr/lib/x86_64-linux-gnu \cargo test -p espeak-phonemizer -- --test-threads=1`
Expected: `test result: ok. 9 passed; 0 failed` (8 existing + 1 new). If `/usr/lib/x86_64-linux-gnu` doesn't have `espeak-ng-data` on the machine running this, locate it with `find /usr -maxdepth 4 -type d -name espeak-ng-data` and substitute its parent directory.

- [ ] **Step 3: Commit**

```bash
cd /home/ali/Workspace/lang/dengjen
git add crates/text/espeak-phonemizer/src/lib.rs
git commit -m "Add test coverage for empty-string input to text_to_phonemes"
```

---

### Task 5: `core` — `DengjenError`/`Phonemes` Display and `DengjenModel` default-method coverage

**Files:**
- Modify: `crates/dengjen/core/src/lib.rs` (new `mod tests` — crate currently has none)

**Interfaces:** N/A — self-contained, tests only. The `NullModel` test double defined here is local to this task's test module; it is not the shared Tier 2 mock fixture referenced in the design doc (that one needs configurable return values for `synth`'s tests and will live elsewhere).

- [ ] **Step 1: Add a `#[cfg(test)] mod tests` block at the end of the file**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Minimal stand-in for `DengjenModel` so this crate's default trait-method
    // logic (speaker lookup, stream_synthesis fallback) can be tested without a
    // real ONNX-backed implementor. Not the shared Tier 2 mock fixture.
    struct NullModel;

    impl DengjenModel for NullModel {
        fn audio_output_info(&self) -> DengjenResult<AudioInfo> {
            Ok(AudioInfo { sample_rate: 22050, num_channels: 1, sample_width: 2 })
        }
        fn phonemize_text(&self, _text: &str) -> DengjenResult<Phonemes> {
            Ok(Phonemes::from(Vec::new()))
        }
        fn speak_batch(&self, _phoneme_batches: Vec<String>) -> DengjenResult<Vec<Audio>> {
            Ok(Vec::new())
        }
        fn speak_one_sentence(&self, _phonemes: String) -> DengjenAudioResult {
            Err(DengjenError::OperationError("not implemented".to_string()))
        }
        fn get_default_synthesis_config(&self) -> DengjenResult<Box<dyn Any>> {
            Ok(Box::new(()))
        }
        fn get_fallback_synthesis_config(&self) -> DengjenResult<Box<dyn Any>> {
            Ok(Box::new(()))
        }
        fn set_fallback_synthesis_config(&self, _synthesis_config: &dyn Any) -> DengjenResult<()> {
            Ok(())
        }
    }

    #[test]
    fn error_display_formats_each_variant() {
        assert_eq!(
            DengjenError::FailedToLoadResource("disk full".to_string()).to_string(),
            "Failed to load resource from. Error `disk full`"
        );
        assert_eq!(
            DengjenError::PhonemizationError("bad text".to_string()).to_string(),
            "bad text"
        );
        assert_eq!(
            DengjenError::OperationError("boom".to_string()).to_string(),
            "boom"
        );
    }

    #[test]
    fn phonemes_display_joins_sentences_with_a_space() {
        let phonemes = Phonemes::from(vec!["hh ə l ˈoʊ".to_string(), "w ˈɜːld".to_string()]);
        assert_eq!(phonemes.to_string(), "hh ə l ˈoʊ w ˈɜːld");
    }

    #[test]
    fn phonemes_display_is_empty_string_for_no_sentences() {
        let phonemes = Phonemes::from(Vec::<String>::new());
        assert_eq!(phonemes.to_string(), "");
    }

    #[test]
    fn default_stream_synthesis_returns_operation_error() {
        let result = NullModel.stream_synthesis("phonemes".to_string(), 100, 3);
        assert!(matches!(result, Err(DengjenError::OperationError(_))));
    }

    #[test]
    fn default_speaker_id_to_name_returns_none_without_speakers() {
        assert_eq!(NullModel.speaker_id_to_name(&0).unwrap(), None);
    }

    #[test]
    fn default_speaker_name_to_id_returns_none_without_speakers() {
        assert_eq!(NullModel.speaker_name_to_id("foo").unwrap(), None);
    }
}
```

- [ ] **Step 2: Run the new tests**

Run: `cd /home/ali/Workspace/lang/dengjen && source "$HOME/.cargo/env" && \cargo test -p dengjen-core`
Expected: `test result: ok. 6 passed; 0 failed`

- [ ] **Step 3: Commit**

```bash
cd /home/ali/Workspace/lang/dengjen
git add crates/dengjen/core/src/lib.rs
git commit -m "Add test coverage for DengjenError/Phonemes Display and DengjenModel default methods"
```

---

### Task 6: `piper` — `load_model_config` error-branch coverage

**Files:**
- Modify: `crates/dengjen/models/piper/src/lib.rs` (append to existing `mod tests`, currently starting at line 1057)

**Interfaces:** N/A — self-contained, tests only.

- [ ] **Step 1: Add these tests inside the existing `#[cfg(test)] mod tests { ... }` block**

```rust
    #[test]
    fn load_model_config_errors_when_file_does_not_exist() {
        let path = std::path::Path::new("/nonexistent-piper-config-xyz.json");
        let result = load_model_config(path);
        assert!(matches!(result, Err(DengjenError::FailedToLoadResource(_))));
    }

    #[test]
    fn load_model_config_errors_on_malformed_json() {
        let mut path = std::env::temp_dir();
        path.push(format!("dengjen-piper-test-malformed-{}.json", std::process::id()));
        std::fs::write(&path, b"{ not valid json").unwrap();
        let result = load_model_config(&path);
        std::fs::remove_file(&path).ok();
        assert!(matches!(result, Err(DengjenError::FailedToLoadResource(_))));
    }
```

- [ ] **Step 2: Run the new tests**

Run: `cd /home/ali/Workspace/lang/dengjen && source "$HOME/.cargo/env" && \cargo test -p dengjen-piper load_model_config`
Expected: `test result: ok. 2 passed; 0 failed`

- [ ] **Step 3: Commit**

```bash
cd /home/ali/Workspace/lang/dengjen
git add crates/dengjen/models/piper/src/lib.rs
git commit -m "Add test coverage for load_model_config's file-not-found and malformed-JSON branches"
```

---

### Task 7: `piper` — `AdaptiveMelChunker` termination coverage

**Files:**
- Modify: `crates/dengjen/models/piper/src/lib.rs` (append to existing `mod tests`)

**Interfaces:** N/A — self-contained, tests only.

- [ ] **Step 1: Add this test inside the existing `mod tests` block, next to the two existing `adaptive_mel_chunker_*` tests**

```rust
    #[test]
    fn adaptive_mel_chunker_terminates_when_remaining_frames_fall_below_minimum() {
        // num_frames=50, chunk_size=10, chunk_padding=5, hop_length=10:
        // chunk_end = 0 + 10 + 5 = 15; remaining = 50 - 15 = 35 <= MIN_CHUNK_SIZE (44),
        // so this chunk is terminal (end_index/end_padding = None) and the next
        // call must return None (iterator exhausted).
        let mut chunker = AdaptiveMelChunker::new(50, 10, 5, 10);
        let (mel_index, audio_index) = chunker.next().unwrap();
        assert_eq!(mel_index.end, None);
        assert_eq!(audio_index.end, None);
        assert!(chunker.next().is_none());
    }
```

- [ ] **Step 2: Run the new test**

Run: `cd /home/ali/Workspace/lang/dengjen && source "$HOME/.cargo/env" && \cargo test -p dengjen-piper adaptive_mel_chunker_terminates`
Expected: `test result: ok. 1 passed; 0 failed`

- [ ] **Step 3: Commit**

```bash
cd /home/ali/Workspace/lang/dengjen
git add crates/dengjen/models/piper/src/lib.rs
git commit -m "Add test coverage for AdaptiveMelChunker's termination branch"
```

---

### Task 8: `piper` — `VitsModelCommons` default-method coverage via a test double

**Files:**
- Modify: `crates/dengjen/models/piper/src/lib.rs` (append to existing `mod tests`)

**Interfaces:**
- Produces: `TestVitsCommons` (test-only struct in `mod tests`, implements the crate-private `VitsModelCommons` trait without needing a `Session`/ONNX file). Used by Task 9.

**Note:** `VitsModel::get_input_output_info` (currently `todo!()`) is **not** fixed in this task. Implementing and testing it needs an actual `ort::Session`, which needs a real `.onnx` file — that's Tier 2 (the vendored fixture), not Tier 1. Left as-is here; tracked in the Tier 2 follow-up.

- [ ] **Step 1: Add the test double and tests inside the existing `mod tests` block**

```rust
    struct TestVitsCommons {
        synth_config: RwLock<PiperSynthesisConfig>,
        config: ModelConfig,
        speaker_map: HashMap<i64, String>,
    }

    impl VitsModelCommons for TestVitsCommons {
        fn get_synth_config(&self) -> &RwLock<PiperSynthesisConfig> {
            &self.synth_config
        }
        fn get_config(&self) -> &ModelConfig {
            &self.config
        }
        fn get_speaker_map(&self) -> &HashMap<i64, String> {
            &self.speaker_map
        }
        fn get_tashkeel_engine(&self) -> Option<&TashkeelEngine> {
            None
        }
    }

    #[test]
    fn get_meta_ids_reads_bos_pad_eos_from_phoneme_id_map() {
        let commons = TestVitsCommons {
            synth_config: RwLock::new(PiperSynthesisConfig::default()),
            config: ModelConfig {
                phoneme_id_map: HashMap::from([
                    (PAD.to_string(), vec![3]),
                    (BOS.to_string(), vec![1]),
                    (EOS.to_string(), vec![2]),
                ]),
                ..Default::default()
            },
            speaker_map: HashMap::new(),
        };
        assert_eq!(commons.get_meta_ids(), (3, 1, 2));
    }

    #[test]
    fn do_set_default_synth_config_updates_scales_and_accepts_a_known_speaker() {
        let commons = TestVitsCommons {
            synth_config: RwLock::new(PiperSynthesisConfig::default()),
            config: ModelConfig::default(),
            speaker_map: HashMap::from([(5, "narrator".to_string())]),
        };
        let new_config = PiperSynthesisConfig {
            speaker: Some(5),
            noise_scale: 0.5,
            length_scale: 1.2,
            noise_w: 0.9,
        };
        commons._do_set_default_synth_config(&new_config).unwrap();
        let synth_config = commons.synth_config.read().unwrap();
        assert_eq!(synth_config.speaker, Some(5));
        assert_eq!(synth_config.length_scale, 1.2);
    }

    #[test]
    fn do_set_default_synth_config_errors_for_an_unknown_speaker_id() {
        let commons = TestVitsCommons {
            synth_config: RwLock::new(PiperSynthesisConfig::default()),
            config: ModelConfig::default(),
            speaker_map: HashMap::new(),
        };
        let new_config = PiperSynthesisConfig {
            speaker: Some(99),
            ..Default::default()
        };
        let result = commons._do_set_default_synth_config(&new_config);
        assert!(matches!(result, Err(DengjenError::OperationError(_))));
    }

    #[test]
    fn do_phonemize_text_passes_through_unchanged_for_text_phoneme_type() {
        let commons = TestVitsCommons {
            synth_config: RwLock::new(PiperSynthesisConfig::default()),
            config: ModelConfig {
                phoneme_type: Some(PhonemeType::Text),
                ..Default::default()
            },
            speaker_map: HashMap::new(),
        };
        let result = commons.do_phonemize_text("hello").unwrap();
        assert_eq!(result.to_vec(), vec!["hello".to_string()]);
    }
```

- [ ] **Step 2: Run the new tests**

Run: `cd /home/ali/Workspace/lang/dengjen && source "$HOME/.cargo/env" && \cargo test -p dengjen-piper`
Expected: `test result: ok. 23 passed; 0 failed` (16 existing + 2 from Task 6 + 1 from Task 7 + 4 from this task)

- [ ] **Step 3: Commit**

```bash
cd /home/ali/Workspace/lang/dengjen
git add crates/dengjen/models/piper/src/lib.rs
git commit -m "Add test coverage for VitsModelCommons default methods via a Session-free test double"
```

---

### Task 9: `piper` — `do_phonemize_text` espeak-disabled error branch

**Files:**
- Modify: `crates/dengjen/models/piper/src/lib.rs` (append to existing `mod tests`)

**Interfaces:**
- Consumes: `TestVitsCommons` from Task 8.

- [ ] **Step 1: Add this feature-gated test inside the existing `mod tests` block**

```rust
    #[cfg(not(feature = "espeak"))]
    #[test]
    fn do_phonemize_text_errors_when_espeak_feature_is_disabled() {
        let commons = TestVitsCommons {
            synth_config: RwLock::new(PiperSynthesisConfig::default()),
            config: ModelConfig {
                phoneme_type: Some(PhonemeType::Espeak),
                ..Default::default()
            },
            speaker_map: HashMap::new(),
        };
        let result = commons.do_phonemize_text("hello");
        assert!(matches!(result, Err(DengjenError::PhonemizationError(_))));
    }
```

- [ ] **Step 2: Run the full crate test suite with the `espeak` feature disabled**

Run: `cd /home/ali/Workspace/lang/dengjen && source "$HOME/.cargo/env" && \cargo test -p dengjen-piper --no-default-features --features tashkeel`
Expected: `test result: ok.` with the new test included and none of the `#[cfg(feature = "espeak")]`-only tests running.

- [ ] **Step 3: Run the default-feature suite too, to confirm the new test correctly does *not* compile/run there**

Run: `cd /home/ali/Workspace/lang/dengjen && source "$HOME/.cargo/env" && \cargo test -p dengjen-piper do_phonemize_text_errors_when_espeak_feature_is_disabled`
Expected: `0 tests` matched (the `#[cfg(not(feature = "espeak"))]` test is absent under default features) — this is correct, not a failure.

- [ ] **Step 4: Commit**

```bash
cd /home/ali/Workspace/lang/dengjen
git add crates/dengjen/models/piper/src/lib.rs
git commit -m "Add test coverage for do_phonemize_text's espeak-disabled error path"
```

---

### Task 10: `sonic-sys` — FFI smoke test

**Files:**
- Modify: `crates/audio/sonic-sys/src/lib.rs`

**Interfaces:** N/A — self-contained, tests only.

- [ ] **Step 1: Append a test module after the `include!` line**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sonic_stream_create_and_destroy_round_trip_returns_non_null_and_does_not_crash() {
        unsafe {
            let stream = sonicCreateStream(22050, 1);
            assert!(!stream.is_null());
            sonicDestroyStream(stream);
        }
    }
}
```

- [ ] **Step 2: Run the new test**

Run: `cd /home/ali/Workspace/lang/dengjen && source "$HOME/.cargo/env" && \cargo test -p sonic-sys`
Expected: `test result: ok. 1 passed; 0 failed`

- [ ] **Step 3: Commit**

```bash
cd /home/ali/Workspace/lang/dengjen
git add crates/audio/sonic-sys/src/lib.rs
git commit -m "Add a create/destroy smoke test for the sonic-sys FFI bindings"
```

---

### Task 11: `cli` — fix `SynthesisMode`'s panic-on-invalid-input, add `SynthesisRequest` conversion coverage

**Files:**
- Modify: `crates/frontends/cli/src/main.rs`

**Interfaces:** N/A — self-contained.

- [ ] **Step 1: Write the failing test first (add to a new `mod tests` block at the end of the file)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn synthesis_mode_from_str_parses_known_values_case_insensitively() {
        assert!(matches!(SynthesisMode::from_str("Lazy"), Ok(SynthesisMode::Lazy)));
        assert!(matches!(SynthesisMode::from_str("PARALLEL"), Ok(SynthesisMode::Parallel)));
        assert!(matches!(SynthesisMode::from_str("realtime"), Ok(SynthesisMode::Realtime)));
    }

    #[test]
    fn synthesis_mode_from_str_returns_an_error_instead_of_panicking_on_unknown_value() {
        assert!(SynthesisMode::from_str("bogus").is_err());
    }
}
```

- [ ] **Step 2: Run it to verify it fails (compile error — `FromStr` isn't implemented yet)**

Run: `cd /home/ali/Workspace/lang/dengjen && source "$HOME/.cargo/env" && \cargo test -p dengjen-cli synthesis_mode_from_str`
Expected: compile error, `the trait bound SynthesisMode: FromStr is not satisfied` (or similar)

- [ ] **Step 3: Replace the panicking `From<&str>` impl with a `FromStr` impl, and add `Debug, PartialEq` derives needed by the tests**

Change:
```rust
#[derive(Clone, Default, Deserialize)]
enum SynthesisMode {
    #[default]
    Lazy,
    Parallel,
    Realtime,
}

impl<'s> From<&'s str> for SynthesisMode {
    fn from(other: &'s str) -> Self {
        match other.to_lowercase().as_str() {
            "lazy" => Self::Lazy,
            "parallel" => Self::Parallel,
            "realtime" => Self::Realtime,
            _ => panic!("Unknown synthesis mode: `{}`", other),
        }
    }
}
```
To:
```rust
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
enum SynthesisMode {
    #[default]
    Lazy,
    Parallel,
    Realtime,
}

impl std::str::FromStr for SynthesisMode {
    type Err = String;

    fn from_str(other: &str) -> Result<Self, Self::Err> {
        match other.to_lowercase().as_str() {
            "lazy" => Ok(Self::Lazy),
            "parallel" => Ok(Self::Parallel),
            "realtime" => Ok(Self::Realtime),
            _ => Err(format!("Unknown synthesis mode: `{}`", other)),
        }
    }
}
```

(clap's derive resolves `#[arg(long)] mode: Option<SynthesisMode>` through `FromStr` automatically — this was previously resolved through the `From<&str>` impl instead, which is why invalid `--mode` values used to panic instead of producing a clean clap parse error.)

- [ ] **Step 4: Run the tests again to verify they pass, and confirm the crate still builds clean**

Run: `cd /home/ali/Workspace/lang/dengjen && source "$HOME/.cargo/env" && \cargo test -p dengjen-cli`
Expected: `test result: ok.` including the two new `synthesis_mode_from_str_*` tests, no compile errors elsewhere in the file (the old `From<&str>` impl had no other call sites in this file).

- [ ] **Step 5: Add `SynthesisRequest` conversion coverage to the same `mod tests` block**

```rust
    #[test]
    fn as_piper_synth_config_falls_back_to_defaults_when_fields_are_none() {
        let default_config = PiperSynthesisConfig {
            speaker: Some(0),
            length_scale: 1.0,
            noise_scale: 0.667,
            noise_w: 0.8,
        };
        let req = SynthesisRequest {
            text: "hello".to_string(),
            ..Default::default()
        };
        let result = req.as_piper_synth_config(&default_config);
        assert_eq!(result.speaker, None);
        assert_eq!(result.length_scale, 1.0);
        assert_eq!(result.noise_scale, 0.667);
        assert_eq!(result.noise_w, 0.8);
    }

    #[test]
    fn as_piper_synth_config_overrides_defaults_when_fields_are_set() {
        let default_config = PiperSynthesisConfig::default();
        let req = SynthesisRequest {
            text: "hello".to_string(),
            speaker_id: Some(3),
            length_scale: Some(2.0),
            ..Default::default()
        };
        let result = req.as_piper_synth_config(&default_config);
        assert_eq!(result.speaker, Some(3));
        assert_eq!(result.length_scale, 2.0);
    }

    #[test]
    fn as_audio_output_config_carries_over_all_fields() {
        let req = SynthesisRequest {
            text: "hello".to_string(),
            rate: Some(80),
            pitch: Some(40),
            volume: Some(90),
            appended_silence_ms: Some(200),
            ..Default::default()
        };
        let config = req.as_audio_output_config();
        assert_eq!(config.rate, Some(80));
        assert_eq!(config.pitch, Some(40));
        assert_eq!(config.volume, Some(90));
        assert_eq!(config.appended_silence_ms, Some(200));
    }
```

- [ ] **Step 6: Run the full crate test suite**

Run: `cd /home/ali/Workspace/lang/dengjen && source "$HOME/.cargo/env" && \cargo test -p dengjen-cli`
Expected: `test result: ok. 5 passed; 0 failed`

- [ ] **Step 7: Commit**

```bash
cd /home/ali/Workspace/lang/dengjen
git add crates/frontends/cli/src/main.rs
git commit -m "Replace SynthesisMode's panicking From<&str> with a proper FromStr impl, add conversion coverage"
```

---

### Task 12: `capi` — fix unchecked null-pointer `.unwrap()`s, add safety and error-code coverage

**Files:**
- Modify: `crates/frontends/capi/src/lib.rs`

**Interfaces:** N/A — self-contained.

- [ ] **Step 1: Write the failing tests first (add a `mod tests` block at the end of the file)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ffi_support::ExternError;

    extern "C" fn noop_callback(_event: SynthesisEvent) -> u8 {
        1
    }

    fn synth_params() -> SynthesisParams {
        SynthesisParams {
            mode: synth_mode::SYNTH_MODE_LAZY,
            rate: 50,
            volume: 100,
            pitch: 50,
            appended_silence_ms: 0,
            callback: noop_callback,
            nonblocking: 0,
        }
    }

    #[test]
    fn get_audio_info_null_voice_returns_null_pointer_error_without_panicking() {
        let mut out_error = ExternError::default();
        let mut audio_info = AudioInfo { sample_rate: 0, num_channels: 0, sample_width: 0 };
        unsafe {
            libdengjenGetAudioInfo(std::ptr::null_mut(), &mut audio_info, &mut out_error);
        }
        assert_eq!(out_error.get_code().code(), error_codes::NULL_POINTER);
    }

    #[test]
    fn get_piper_default_synth_config_null_voice_returns_null_pointer_error_without_panicking() {
        let mut out_error = ExternError::default();
        let result =
            unsafe { libdengjenGetPiperDefaultSynthConfig(std::ptr::null_mut(), &mut out_error) };
        assert!(result.is_null());
        assert_eq!(out_error.get_code().code(), error_codes::NULL_POINTER);
    }

    #[test]
    fn set_piper_synth_config_null_voice_returns_null_pointer_error_without_panicking() {
        let mut out_error = ExternError::default();
        let synth_config =
            PiperSynthConfig { speaker: 0, length_scale: 1.0, noise_scale: 1.0, noise_w: 1.0 };
        unsafe {
            libdengjenSetPiperSynthConfig(std::ptr::null_mut(), synth_config, &mut out_error);
        }
        assert_eq!(out_error.get_code().code(), error_codes::NULL_POINTER);
    }

    #[test]
    fn speak_null_voice_returns_null_pointer_error_without_panicking() {
        let mut out_error = ExternError::default();
        let text = std::ffi::CString::new("hello").unwrap();
        unsafe {
            libdengjenSpeak(
                std::ptr::null_mut(),
                FfiStr::from_cstr(&text),
                synth_params(),
                &mut out_error,
            );
        }
        assert_eq!(out_error.get_code().code(), error_codes::NULL_POINTER);
    }

    #[test]
    fn speak_to_file_null_voice_returns_null_pointer_error_without_panicking() {
        let mut out_error = ExternError::default();
        let text = std::ffi::CString::new("hello").unwrap();
        let filename = std::ffi::CString::new("out.wav").unwrap();
        let result = unsafe {
            libdengjenSpeakToFile(
                std::ptr::null_mut(),
                FfiStr::from_cstr(&text),
                synth_params(),
                FfiStr::from_cstr(&filename),
                &mut out_error,
            )
        };
        assert_eq!(result, 0);
        assert_eq!(out_error.get_code().code(), error_codes::NULL_POINTER);
    }

    #[test]
    fn error_codes_round_trip_through_dengjen_ffi_error() {
        let cases = [
            (DengjenError::FailedToLoadResource("x".into()), error_codes::FAILED_TO_LOAD_RESOURCE),
            (DengjenError::PhonemizationError("x".into()), error_codes::PHONEMIZATION_ERROR),
            (DengjenError::OperationError("x".into()), error_codes::OPERATION_ERROR),
        ];
        for (err, expected_code) in cases {
            let ffi_err: DengjenFFIError = err.into();
            assert_eq!(ffi_err.0, expected_code);
        }
    }
}
```

- [ ] **Step 2: Run the tests to verify the null-pointer ones currently panic (not fail cleanly)**

Run: `cd /home/ali/Workspace/lang/dengjen && source "$HOME/.cargo/env" && \cargo test -p libdengjen null_pointer_error`
Expected: test binary aborts/panics on `voice_ptr.as_ref().unwrap()` (e.g. `called Option::unwrap() on a None value`) — confirms the bug described in the design doc.

- [ ] **Step 3: Add the `NULL_POINTER` error code and a constructor**

In `pub mod error_codes { ... }`, add after `UNKNOWN_ERROR`:
```rust
    pub const NULL_POINTER: i32 = 22;
```

In `impl DengjenFFIError { ... }`, add after `invalid_synthesis_mode`:
```rust
    fn null_pointer(param_name: &str) -> Self {
        Self(error_codes::NULL_POINTER, format!("`{}` must not be null", param_name))
    }
```

- [ ] **Step 4: Replace the five unchecked `.unwrap()` call sites with explicit null checks that populate `out_error` instead of panicking**

Change `libdengjenGetAudioInfo`:
```rust
pub unsafe extern "C" fn libdengjenGetAudioInfo(
    voice_ptr: *mut DengjenVoice,
    audio_info_ptr: *mut AudioInfo,
    out_error: &mut ExternError,
) {
    let Some(voice) = voice_ptr.as_ref() else {
        *out_error = DengjenFFIError::null_pointer("voice_ptr").into();
        return;
    };
    let Some(audio_info_mut) = audio_info_ptr.as_mut() else {
        *out_error = DengjenFFIError::null_pointer("audio_info_ptr").into();
        return;
    };
    let mut audio_info = AssertUnwindSafe(audio_info_mut);
    call_with_result(out_error, move || {
        voice
            .audio_output_info()
            .map(|a| {
                audio_info.sample_rate = a.sample_rate as u32;
                audio_info.num_channels = a.num_channels as u32;
                audio_info.sample_width = a.sample_width as u32;
            })
            .map_err(DengjenFFIError::from)
    })
}
```

Change `libdengjenGetPiperDefaultSynthConfig` (only the null-check prologue; the rest of the body is unchanged):
```rust
pub unsafe extern "C" fn libdengjenGetPiperDefaultSynthConfig(
    voice_ptr: *mut DengjenVoice,
    out_error: &mut ExternError,
) -> *mut PiperSynthConfig {
    let Some(voice) = voice_ptr.as_ref() else {
        *out_error = DengjenFFIError::null_pointer("voice_ptr").into();
        return std::ptr::null_mut();
    };
    call_with_result(out_error, move || {
        let synth_config = voice
            .get_default_synthesis_config()
            .map_err(DengjenFFIError::from)?;
        match synth_config.downcast_ref::<dengjen_piper::PiperSynthesisConfig>() {
            Some(config) => Ok(PiperSynthConfig {
                speaker: config.speaker.map(|sid| sid as u32).unwrap_or_default(),
                length_scale: config.length_scale,
                noise_scale: config.noise_scale,
                noise_w: config.noise_w,
            }),
            None => Err(DengjenFFIError(
                error_codes::UNKNOWN_ERROR,
                "Cannot retrieve Piper's default synthesis config".to_string(),
            )),
        }
    })
}
```

Change `libdengjenSetPiperSynthConfig` (null-check prologue only):
```rust
pub unsafe extern "C" fn libdengjenSetPiperSynthConfig(
    voice_ptr: *mut DengjenVoice,
    synth_config: PiperSynthConfig,
    out_error: &mut ExternError,
) {
    let Some(voice) = voice_ptr.as_ref() else {
        *out_error = DengjenFFIError::null_pointer("voice_ptr").into();
        return;
    };
    call_with_result(out_error, move || {
        let piper_synth_config = synth_config.as_piper_synth_config();
        let config = &piper_synth_config as &dyn Any;
        voice
            .set_fallback_synthesis_config(config)
            .map_err(DengjenFFIError::from)
    })
}
```

Change `libdengjenSpeak` (null-check prologue only):
```rust
pub unsafe extern "C" fn libdengjenSpeak(
    voice_ptr: *mut DengjenVoice,
    text_ptr: FfiStr,
    params: SynthesisParams,
    out_error: &mut ExternError,
) {
    let Some(voice) = voice_ptr.as_ref() else {
        *out_error = DengjenFFIError::null_pointer("voice_ptr").into();
        return;
    };
    let synth = AssertUnwindSafe(Arc::clone(&voice.0));
    call_with_result(out_error, move || _synthesize(synth, text_ptr, params))
}
```

Change `libdengjenSpeakToFile` (null-check prologue only):
```rust
pub unsafe extern "C" fn libdengjenSpeakToFile(
    voice_ptr: *mut DengjenVoice,
    text_ptr: FfiStr,
    params: SynthesisParams,
    out_filename_ptr: FfiStr,
    out_error: &mut ExternError,
) -> u8 {
    let Some(voice) = voice_ptr.as_ref() else {
        *out_error = DengjenFFIError::null_pointer("voice_ptr").into();
        return 0;
    };
    let synth = AssertUnwindSafe(Arc::clone(&voice.0));
    call_with_result(out_error, move || {
        Ok::<u8, DengjenFFIError>(
            _synthesize_to_file(synth, text_ptr, params, out_filename_ptr).is_ok() as u8,
        )
    })
}
```

- [ ] **Step 5: Run the full crate test suite to verify all six new tests pass cleanly (no panics)**

Run: `cd /home/ali/Workspace/lang/dengjen && source "$HOME/.cargo/env" && \cargo test -p libdengjen`
Expected: `test result: ok. 6 passed; 0 failed`

- [ ] **Step 6: Commit**

```bash
cd /home/ali/Workspace/lang/dengjen
git add crates/frontends/capi/src/lib.rs
git commit -m "Replace unchecked null-pointer unwraps in the C API with a mapped NULL_POINTER error"
```

---

### Task 13: Remove the dead `istft-sys` workspace member

**Files:**
- Modify: `Cargo.toml` (workspace `members` list)
- Delete: `crates/audio/istft-sys/` (entire directory: `Cargo.toml`, `build.rs`, `src/lib.rs`)

**Interfaces:** N/A.

**Rationale (from the design doc):** `istft-sys` exposes no Rust API (`src/lib.rs` is empty) and has zero consumers anywhere in the workspace (verified via `grep -rl istft` across the repo, excluding its own directory) — it only builds and statically links `nanosnap` for nothing. This does **not** touch the `deps/nanosnap` git submodule or `.gitmodules` — only the crate that (unusedly) built against it.

- [ ] **Step 1: Remove the member entry from the workspace `Cargo.toml`**

In `Cargo.toml`, delete this line from the `members` array:
```toml
    "crates/audio/istft-sys",
```

- [ ] **Step 2: Delete the crate directory**

```bash
cd /home/ali/Workspace/lang/dengjen
git rm -r crates/audio/istft-sys
```

- [ ] **Step 3: Verify the workspace still builds and `Cargo.lock` updates cleanly**

Run: `cd /home/ali/Workspace/lang/dengjen && source "$HOME/.cargo/env" && \cargo check --workspace`
Expected: succeeds, no errors about a missing `istft-sys` dependency anywhere (confirms nothing depended on it).

- [ ] **Step 4: Commit**

```bash
cd /home/ali/Workspace/lang/dengjen
git add Cargo.toml Cargo.lock
git commit -m "Remove istft-sys: it exposes no Rust API and has zero consumers in the workspace"
```

---

### Task 14: Wire `cargo test` into CI

**Files:**
- Modify: `.github/workflows/CI.yml`

**Interfaces:**
- Consumes: all tests added in Tasks 1–12; the workspace-member removal from Task 13.

**Context:** `CI.yml` currently only builds Python wheels via maturin — `cargo test` has never run in CI. This task adds a `test` job on `ubuntu-latest` only (per the design doc's "Out of scope" section — Windows/macOS test execution is a follow-up). It must respect the espeak-ng thread-safety constraint from this plan's Global Constraints: `espeak-phonemizer` runs separately, single-threaded, with `DENGJEN_ESPEAKNG_DATA_DIRECTORY` set.

- [ ] **Step 1: Add a `test` job to `.github/workflows/CI.yml`, alongside the existing `linux`/`windows`/`macos`/`sdist`/`release` jobs**

```yaml
  test:
    runs-on: ubuntu-latest
    steps:
      - name: git-submodule-fix
        run: git config --global protocol.file.allow always
      - name: install libclang and espeak-ng
        run: sudo apt-get update && sudo apt-get install -y llvm llvm-dev llvm-runtime libclang-dev espeak-ng
      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.3.0
        with:
          submodules: recursive
          persist-credentials: false
      - name: cargo test (workspace, excluding espeak-phonemizer)
        run: cargo test --workspace --exclude espeak-phonemizer
      - name: locate espeak-ng-data directory
        run: |
          data_dir="$(dirname "$(find /usr -maxdepth 5 -type d -name espeak-ng-data | head -n1)")"
          if [ -z "$data_dir" ] || [ "$data_dir" = "." ]; then
            echo "Could not locate an espeak-ng-data directory" >&2
            exit 1
          fi
          echo "DENGJEN_ESPEAKNG_DATA_DIRECTORY=$data_dir" >> "$GITHUB_ENV"
      - name: cargo test (espeak-phonemizer, single-threaded)
        run: cargo test -p espeak-phonemizer -- --test-threads=1
```

- [ ] **Step 2: Push this change on a branch and confirm the `test` job passes in GitHub Actions**

This step can't be verified locally (no GitHub Actions runner available in this environment). Push the branch, open a PR, and check the Actions run for the new `test` job before merging. If `sudo apt-get install espeak-ng` places `espeak-ng-data` somewhere the `find` step doesn't catch, widen the `-maxdepth` or hardcode the path once observed from the failed run's logs.

- [ ] **Step 3: Commit**

```bash
cd /home/ali/Workspace/lang/dengjen
git add .github/workflows/CI.yml
git commit -m "Run cargo test in CI for the first time, handling espeak-ng's thread-safety constraint"
```
