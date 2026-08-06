# Core Engine Cancellation & Streaming Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add explicit, cross-thread cancellation to dengjen's realtime/streaming synthesis path (core trait → synth orchestration → Piper backend → C-ABI), so an external caller (eventually the NVDA add-on) can halt an in-flight utterance via a dedicated `libdengjenCancel` entry point instead of relying only on a callback's return value.

**Architecture:** A new `CancellationToken` (cloneable `Arc<AtomicBool>` wrapper) is added to `dengjen-core` and threaded as a new parameter through `DengjenModel::stream_synthesis`, `dengjen-synth`'s `synthesize_streamed`/`RealtimeSpeechStream`, and Piper's `VitsStreamingModel`/`SpeechStreamer`/`AdaptiveMelChunker`. Each stage checks the token between chunks and stops early once cancelled. The C-ABI (`libdengjen`) stores the token for the in-flight realtime synthesis on `DengjenVoice` and exposes `libdengjenCancel(voice_ptr)` to trigger it from another thread.

**Tech Stack:** Rust workspace, `ort` 2.0.0-rc.13 (ONNX Runtime), `flume` (channels), `rayon`/custom thread pool, `ffi-support` (C-ABI), `cbindgen` (header generation).

## Global Constraints

- This is subsystem 1 of 3 from `docs/superpowers/specs/2026-08-06-core-engine-rewrite-design.md` (Core Engine only — no NVDA add-on or packaging work here).
- The C-ABI evolves the existing callback-based streaming API; it does not replace it with the source spec's bare polling sketch (spec §3, design doc §3).
- Cancellation applies to the realtime/streaming synthesis path (`SYNTH_MODE_REALTIME` / `stream_synthesis`) — lazy and parallel modes are unaffected by this plan (design doc §3).
- Cross-platform: every change must build on Linux/macOS/Windows; no platform-specific code introduced here (design doc §7).
- Split mechanical changes (call-site signature fixups, the benchmarks.rs import fix) from semantic changes (cancellation logic) into separate tasks/commits.
- No new external dependencies — `CancellationToken` uses `std::sync` only.

---

### Task 1: `CancellationToken` type in `dengjen-core`

**Files:**
- Create: `crates/dengjen/core/src/cancellation.rs`
- Modify: `crates/dengjen/core/src/lib.rs` (add `mod cancellation; pub use cancellation::CancellationToken;` near the top, alongside the existing `pub use audio_ops::{...}` block)

**Interfaces:**
- Produces: `pub struct CancellationToken` with `CancellationToken::new() -> Self`, `.cancel(&self)`, `.is_cancelled(&self) -> bool`, and `#[derive(Clone, Default)]` (clones share the same underlying flag).

- [ ] **Step 1: Write the failing test**

```rust
// crates/dengjen/core/src/cancellation.rs
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_token_is_not_cancelled() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn cancel_marks_token_cancelled() {
        let token = CancellationToken::new();
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn clones_share_cancellation_state() {
        let token = CancellationToken::new();
        let clone = token.clone();
        clone.cancel();
        assert!(token.is_cancelled(), "cancelling a clone must be visible on the original");
    }

    #[test]
    fn default_token_is_not_cancelled() {
        assert!(!CancellationToken::default().is_cancelled());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd crates/dengjen/core && cargo test cancellation:: -- --include-ignored`
Expected: FAIL with "cannot find type `CancellationToken` in this scope" (the struct doesn't exist yet).

- [ ] **Step 3: Write minimal implementation**

Add above the `#[cfg(test)]` block in the same file:

```rust
/// A cheaply-cloneable flag used to request early termination of an in-flight
/// streaming synthesis from another thread. Clones observe the same state.
#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd crates/dengjen/core && cargo test cancellation::`
Expected: PASS (4 tests: `new_token_is_not_cancelled`, `cancel_marks_token_cancelled`, `clones_share_cancellation_state`, `default_token_is_not_cancelled`)

- [ ] **Step 5: Commit**

```bash
git add crates/dengjen/core/src/cancellation.rs crates/dengjen/core/src/lib.rs
git commit -m "core: add CancellationToken"
```

---

### Task 2: Thread `cancel_token` through `DengjenModel::stream_synthesis`

**Files:**
- Modify: `crates/dengjen/core/src/lib.rs:121-131` (trait default method), `:194-198` (`NullModel` impl doesn't override `stream_synthesis`, so no change needed there), `:196` (`default_stream_synthesis_returns_operation_error` test call site)

**Interfaces:**
- Consumes: `CancellationToken` from Task 1.
- Produces: `DengjenModel::stream_synthesis(&self, phonemes: String, chunk_size: usize, chunk_padding: usize, cancel_token: CancellationToken) -> DengjenResult<AudioStreamIterator<'_>>` — the new trait signature every implementor and caller (Tasks 3-6) must match.

- [ ] **Step 1: Write the failing test**

Update the existing test at `crates/dengjen/core/src/lib.rs` (in `mod tests`) to call the new signature — this fails to compile until Step 3:

```rust
    #[test]
    fn default_stream_synthesis_returns_operation_error() {
        let result = NullModel.stream_synthesis(
            "phonemes".to_string(),
            100,
            3,
            CancellationToken::new(),
        );
        assert!(matches!(result, Err(DengjenError::OperationError(_))));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd crates/dengjen/core && cargo test default_stream_synthesis`
Expected: FAIL to compile — "this function takes 4 arguments but 3 arguments were supplied" (trait signature still has 3 params).

- [ ] **Step 3: Write minimal implementation**

In `crates/dengjen/core/src/lib.rs`, change the trait default method (currently lines ~121-130):

```rust
    fn stream_synthesis(
        &self,
        #[allow(unused_variables)] phonemes: String,
        #[allow(unused_variables)] chunk_size: usize,
        #[allow(unused_variables)] chunk_padding: usize,
        #[allow(unused_variables)] cancel_token: CancellationToken,
    ) -> DengjenResult<AudioStreamIterator<'_>> {
        Err(DengjenError::OperationError(
                "Streaming synthesis is not supported for this model".to_string(),
            ))
    }
```

`NullModel` in the test module doesn't override `stream_synthesis`, so it picks up the new default signature automatically — no change needed there.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd crates/dengjen/core && cargo test`
Expected: PASS, all `dengjen-core` tests green.

- [ ] **Step 5: Commit**

```bash
git add crates/dengjen/core/src/lib.rs
git commit -m "core: add cancel_token parameter to DengjenModel::stream_synthesis"
```

---

### Task 3: Propagate cancellation through `dengjen-synth`

**Files:**
- Modify: `crates/dengjen/synth/src/lib.rs` — `DengjenModel for DengjenSpeechSynthesizer` impl (`stream_synthesis` passthrough, currently `:235-242`), `synthesize_streamed` (`:152-168`), `RealtimeSpeechStream::new` (`:333-378`), `RealtimeSpeechStream::process_rt_stream` (`:379-417`)
- Test: add a `#[cfg(test)] mod cancellation_tests` block at the end of `crates/dengjen/synth/src/lib.rs`

**Interfaces:**
- Consumes: `CancellationToken` (Task 1), the updated `DengjenModel::stream_synthesis` signature (Task 2).
- Produces: `DengjenSpeechSynthesizer::synthesize_streamed(&self, text: String, output_config: Option<AudioOutputConfig>, chunk_size: usize, chunk_padding: usize, cancel_token: CancellationToken) -> DengjenResult<RealtimeSpeechStream>` — the new signature Tasks 4-6's call sites must match.

- [ ] **Step 1: Write the failing test**

Append to `crates/dengjen/synth/src/lib.rs`:

```rust
#[cfg(test)]
mod cancellation_tests {
    use super::*;
    use std::any::Any;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingStreamModel {
        chunks_per_sentence: usize,
        sentences: usize,
        produced: Arc<AtomicUsize>,
    }

    impl DengjenModel for CountingStreamModel {
        fn audio_output_info(&self) -> DengjenResult<AudioInfo> {
            Ok(AudioInfo { sample_rate: 16000, num_channels: 1, sample_width: 2 })
        }
        fn phonemize_text(&self, _text: &str) -> DengjenResult<Phonemes> {
            Ok(Phonemes::from(vec!["sentence".to_string(); self.sentences]))
        }
        fn speak_batch(&self, _phoneme_batches: Vec<String>) -> DengjenResult<Vec<Audio>> {
            Ok(Vec::new())
        }
        fn speak_one_sentence(&self, _phonemes: String) -> DengjenAudioResult {
            Err(DengjenError::OperationError("not used by this test".to_string()))
        }
        fn get_default_synthesis_config(&self) -> DengjenResult<Box<dyn Any>> {
            Ok(Box::new(()))
        }
        fn get_fallback_synthesis_config(&self) -> DengjenResult<Box<dyn Any>> {
            Ok(Box::new(()))
        }
        fn set_fallback_synthesis_config(&self, _c: &dyn Any) -> DengjenResult<()> {
            Ok(())
        }
        fn supports_streaming_output(&self) -> bool {
            true
        }
        fn stream_synthesis(
            &self,
            _phonemes: String,
            _chunk_size: usize,
            _chunk_padding: usize,
            cancel_token: CancellationToken,
        ) -> DengjenResult<AudioStreamIterator<'_>> {
            let produced = Arc::clone(&self.produced);
            let n = self.chunks_per_sentence;
            let iter = (0..n).map_while(move |_| {
                if cancel_token.is_cancelled() {
                    None
                } else {
                    produced.fetch_add(1, Ordering::SeqCst);
                    Some(Ok(AudioSamples::from(vec![0.0f32; 4])))
                }
            });
            Ok(Box::new(iter))
        }
    }

    #[test]
    fn cancelling_mid_stream_stops_further_chunks() {
        let produced = Arc::new(AtomicUsize::new(0));
        let model: Arc<dyn DengjenModel + Send + Sync> = Arc::new(CountingStreamModel {
            chunks_per_sentence: 1000,
            sentences: 5,
            produced: Arc::clone(&produced),
        });
        let synth = DengjenSpeechSynthesizer::new(model).unwrap();
        let cancel_token = CancellationToken::new();
        let stream = synth
            .synthesize_streamed("irrelevant".to_string(), None, 10, 0, cancel_token.clone())
            .unwrap();

        let mut received = 0;
        for result in stream {
            result.unwrap();
            received += 1;
            if received == 3 {
                cancel_token.cancel();
            }
        }

        let total_possible = 1000 * 5;
        assert!(
            received < total_possible,
            "expected cancellation to truncate the stream, got all {received} chunks"
        );
        assert!(produced.load(Ordering::SeqCst) < total_possible);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd crates/dengjen/synth && cargo test cancelling_mid_stream`
Expected: FAIL to compile — `synthesize_streamed` takes 4 arguments, 5 were supplied.

- [ ] **Step 3: Write minimal implementation**

In `crates/dengjen/synth/src/lib.rs`:

1. `DengjenModel for DengjenSpeechSynthesizer`'s `stream_synthesis` passthrough (currently `:235-242`):

```rust
    fn stream_synthesis<'a>(
        &'a self,
        phonemes: String,
        chunk_size: usize,
        chunk_padding: usize,
        cancel_token: CancellationToken,
    ) -> DengjenResult<Box<dyn Iterator<Item = DengjenResult<AudioSamples>> + Send + Sync + 'a>> {
        self.0.stream_synthesis(phonemes, chunk_size, chunk_padding, cancel_token)
    }
```

2. `synthesize_streamed` (currently `:152-168`):

```rust
    pub fn synthesize_streamed(
        &self,
        text: String,
        output_config: Option<AudioOutputConfig>,
        chunk_size: usize,
        chunk_padding: usize,
        cancel_token: CancellationToken,
    ) -> DengjenResult<RealtimeSpeechStream> {
        let provider = self.create_synthesis_task_provider(text, output_config);
        let wavinfo = self.0.audio_output_info()?;
        RealtimeSpeechStream::new(
            provider,
            chunk_size,
            chunk_padding,
            wavinfo.sample_rate,
            wavinfo.num_channels,
            cancel_token,
        )
    }
```

3. `RealtimeSpeechStream::new` (currently `:333-378`) — add the parameter, check it before starting each sentence, and pass it into `stream_synthesis`/`process_rt_stream`:

```rust
    fn new(
        provider: SpeechSynthesisTaskProvider,
        chunk_size: usize,
        chunk_padding: usize,
        sample_rate: usize,
        num_channels: usize,
        cancel_token: CancellationToken,
    ) -> DengjenResult<Self> {
        let phonemes = provider.get_phonemes()?.into_iter();
        let (tx, rx) = flume::unbounded();
        SYNTHESIS_THREAD_POOL.spawn(move || {
            let mut chunk_size = chunk_size;
            let chunk_factor = 1;
            let mut num_processed_chunks = 0;
            for ph_sent in phonemes {
                if cancel_token.is_cancelled() {
                    return;
                }
                chunk_size = if num_processed_chunks != 0 {
                    chunk_size  * chunk_factor * num_processed_chunks
                } else {
                    chunk_size
                };
                match provider
                    .model
                    .stream_synthesis(ph_sent, chunk_size, chunk_padding, cancel_token.clone())
                {
                    Ok(stream) => {
                        let send_result = RealtimeSpeechStream::process_rt_stream(
                            stream,
                            &tx,
                            provider.output_config.as_ref(),
                            sample_rate,
                            num_channels,
                            &cancel_token,
                        );
                        match send_result {
                            Ok(num_chunks) => num_processed_chunks += num_chunks,
                            Err(_) => return
                        };
                    }
                    Err(e) => {
                        tx.send(Err(e)).ok();
                        return;
                    }
                };
            }
        });
        Ok(Self(rx))
    }
```

4. `process_rt_stream` (currently `:379-417`) — add the parameter and check it inside both loops, and skip trailing silence once cancelled:

```rust
    #[inline(always)]
    fn process_rt_stream(
        stream: AudioStreamIterator,
        tx: &Sender<DengjenResult<AudioSamples>>,
        audio_output_config: Option<&AudioOutputConfig>,
        sample_rate: usize,
        num_channels: usize,
        cancel_token: &CancellationToken,
    ) -> Result<usize, SendError<DengjenResult<AudioSamples>>> {
        let mut num_chunks = 0;
        if let Some(output_config) = audio_output_config {
            for result in stream {
                if cancel_token.is_cancelled() {
                    return Ok(num_chunks);
                }
                match result {
                    Ok(samples) => {
                        tx.send(output_config.apply_to_raw_samples(
                            samples,
                            sample_rate,
                            num_channels,
                        ))?;
                        num_chunks += 1;
                    }
                    Err(e) => {
                        tx.send(Err(e))?;
                    }
                };
            }
            if !cancel_token.is_cancelled() {
                if let Some(silence_ms) = output_config.appended_silence_ms {
                    let silence_result =
                        output_config.generate_silence(silence_ms as usize, sample_rate, num_channels);
                    tx.send(silence_result)?;
                }
            }
            Ok(num_chunks)
        } else {
            for result in stream {
                if cancel_token.is_cancelled() {
                    return Ok(num_chunks);
                }
                tx.send(result)?;
                num_chunks += 1;
            }
            Ok(num_chunks)
        }
    }
```

This will not compile yet — `crates/dengjen/synth/src/tests.rs` and `crates/dengjen/synth/src/benchmarks.rs` still call the old 4-argument `synthesize_streamed`. That's expected; Task 5 fixes those call sites. For this task, verify with the unit test target only (Step 4 below), not `cargo test --workspace` or the crate's other integration test binary.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd crates/dengjen/synth && cargo test --lib cancelling_mid_stream`
Expected: PASS (`--lib` runs only `src/lib.rs`'s inline tests, skipping `src/tests.rs`, which won't compile until Task 5).

- [ ] **Step 5: Commit**

```bash
git add crates/dengjen/synth/src/lib.rs
git commit -m "synth: propagate cancel_token through streamed synthesis"
```

---

### Task 4: Cancel-aware streaming in the Piper backend

**Files:**
- Modify: `crates/dengjen/models/piper/src/lib.rs` — `VitsStreamingModel::stream_synthesis` (currently `:769-786`), `SpeechStreamer` (struct `:889-894`, `new` `:896-918`, `Iterator::next` `:979-993`)

**Interfaces:**
- Consumes: `CancellationToken` (Task 1), updated `DengjenModel::stream_synthesis` signature (Task 2).
- Produces: no new public interface — `VitsStreamingModel` now satisfies the Task 2 trait signature.

- [ ] **Step 1: Confirm the compile failure this task fixes**

Run: `cd crates/dengjen/models/piper && cargo check 2>&1 | grep "stream_synthesis"`
Expected: an error that `VitsStreamingModel`'s `stream_synthesis` doesn't match the trait (wrong number of arguments) — this crate has no mock-model unit test for this path (real inference needs an ONNX model file not present in a clean checkout, per the README's testing caveat), so this task is verified by compilation and the existing gitignored-fixture-gated integration test in `dengjen-synth`, not a new unit test.

- [ ] **Step 2: Update `VitsStreamingModel::stream_synthesis`**

Currently `:769-786`:

```rust
    fn stream_synthesis(
        &self,
        phonemes: String,
        chunk_size: usize,
        chunk_padding: usize,
        cancel_token: CancellationToken,
    ) -> DengjenResult<AudioStreamIterator<'_>> {
        let (pad_id, bos_id, eos_id) = self.get_meta_ids();
        let phonemes = self.phonemes_to_input_ids(&phonemes, pad_id, bos_id, eos_id);
        let encoder_outputs = self.infer_encoder(phonemes)?;
        let streamer = Box::new(SpeechStreamer::new(
            Arc::clone(&self.decoder_model),
            encoder_outputs,
            chunk_size,
            chunk_padding,
            self.config.hop_length.unwrap_or(256),
            cancel_token,
        ));
        Ok(streamer)
    }
```

Add the import at the top of the file: `use dengjen_core::CancellationToken;` (alongside the existing `use dengjen_core::{...}` line).

- [ ] **Step 3: Thread the token through `SpeechStreamer`**

`SpeechStreamer` struct (currently `:889-894`) gains a field:

```rust
struct SpeechStreamer {
    decoder_model: Arc<Mutex<Session>>,
    encoder_outputs: EncoderOutputs,
    mel_chunker: AdaptiveMelChunker,
    one_shot: bool,
    cancel_token: CancellationToken,
}
```

`SpeechStreamer::new` (currently `:896-918`) takes and stores it:

```rust
impl SpeechStreamer {
    fn new(
        decoder_model: Arc<Mutex<Session>>,
        encoder_outputs: EncoderOutputs,
        chunk_size: usize,
        chunk_padding: usize,
        hop_length: usize,
        cancel_token: CancellationToken,
    ) -> Self {
        let num_frames = encoder_outputs.z.shape()[2];
        let mel_chunker = AdaptiveMelChunker::new(
            num_frames as isize,
            chunk_size as isize,
            chunk_padding as isize,
            hop_length as isize,
        );
        let one_shot = num_frames <= (chunk_size * 2 + (chunk_padding * 2));
        Self {
            decoder_model,
            encoder_outputs,
            mel_chunker,
            one_shot,
            cancel_token,
        }
    }
```

(`synthesize_chunk` is unchanged.)

- [ ] **Step 4: Check the token at the top of `Iterator::next`**

Currently `:979-993`:

```rust
impl Iterator for SpeechStreamer {
    type Item = DengjenResult<AudioSamples>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cancel_token.is_cancelled() {
            return None;
        }
        let (mel_index, audio_index) = self.mel_chunker.next()?;
        if self.one_shot {
            self.mel_chunker.consume();
            Some(
                self.encoder_outputs
                    .infer_decoder(self.decoder_model.as_ref()),
            )
        } else {
            Some(self.synthesize_chunk(mel_index, audio_index))
        }
    }
}
```

This is the check that skips the next ONNX decoder call entirely once cancelled — the actual latency win from cancellation on this backend, since each remaining chunk would otherwise still run inference before `dengjen-synth`'s own check (Task 3) discards it.

- [ ] **Step 5: Verify the crate compiles**

Run: `cd crates/dengjen/models/piper && cargo check`
Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add crates/dengjen/models/piper/src/lib.rs
git commit -m "piper: stop streaming decode early when cancelled"
```

---

### Task 5: Fix remaining `synthesize_streamed`/`stream_synthesis` call sites

**Files:**
- Modify: `crates/frontends/cli/src/main.rs:157`, `crates/frontends/grpc/src/main.rs:383`, `crates/frontends/python/src/lib.rs:381-392`, `crates/dengjen/synth/src/tests.rs:26`, `crates/dengjen/synth/src/benchmarks.rs:51,91`

**Interfaces:**
- Consumes: `DengjenSpeechSynthesizer::synthesize_streamed(..., cancel_token: CancellationToken)` from Task 3.

None of these frontends have an external cancellation signal wired up yet (that arrives with the NVDA add-on's own spec later) — each call site passes a fresh, never-cancelled `dengjen_core::CancellationToken::new()` so the workspace compiles and behaves exactly as before.

- [ ] **Step 1: Confirm the compile failures this task fixes**

Run: `cargo build --workspace 2>&1 | grep -A2 "synthesize_streamed"`
Expected: "this function takes 5 arguments but 4 arguments were supplied" at each of the five call sites listed above.

- [ ] **Step 2: Fix `crates/frontends/cli/src/main.rs`**

`dengjen_synth` re-exports `dengjen_core::*` (see `crates/dengjen/synth/src/lib.rs:2`), so `CancellationToken` is already reachable through the existing import. Change the top-of-file import (currently):

```rust
use dengjen_synth::{
    AudioOutputConfig, AudioSamples, DengjenModel, DengjenResult, DengjenSpeechSynthesizer,
};
```

to:

```rust
use dengjen_synth::{
    AudioOutputConfig, AudioSamples, CancellationToken, DengjenModel, DengjenResult,
    DengjenSpeechSynthesizer,
};
```

and change the `SynthesisMode::Realtime` arm (currently):

```rust
        SynthesisMode::Realtime => {
            let stream = synth.synthesize_streamed(
                req.text,
                output_config,
                req.chunk_size.unwrap_or(100),
                req.chunk_padding.unwrap_or(3),
            )?;
            consume_stream(stream)?
        }
```

to:

```rust
        SynthesisMode::Realtime => {
            let stream = synth.synthesize_streamed(
                req.text,
                output_config,
                req.chunk_size.unwrap_or(100),
                req.chunk_padding.unwrap_or(3),
                CancellationToken::new(),
            )?;
            consume_stream(stream)?
        }
```

- [ ] **Step 3: Fix `crates/frontends/grpc/src/main.rs`**

Change the top-of-file import (currently):

```rust
use dengjen_core::{DengjenError, DengjenModel, DengjenResult};
```

to:

```rust
use dengjen_core::{CancellationToken, DengjenError, DengjenModel, DengjenResult};
```

and change the call (currently):

```rust
            let stream_result = synth.synthesize_streamed(req.text, output_config, 55, 3);
```

to:

```rust
            let stream_result =
                synth.synthesize_streamed(req.text, output_config, 55, 3, CancellationToken::new());
```

- [ ] **Step 4: Fix `crates/frontends/python/src/lib.rs`**

Change the top-of-file import (currently):

```rust
use dengjen_core::{DengjenError, DengjenModel, Audio, AudioInfo};
```

to:

```rust
use dengjen_core::{Audio, AudioInfo, CancellationToken, DengjenError, DengjenModel};
```

and change the `synthesize_streamed` pymethod body (currently):

```rust
    fn synthesize_streamed(
        &self,
        text: String,
        audio_output_config: Option<PyAudioOutputConfig>,
        chunk_size: Option<usize>,
        chunk_padding: Option<usize>,
    ) -> PyDengjenResult<PyRealtimeSpeechStream> {
        let stream = self.0.synthesize_streamed(
            text,
            audio_output_config.map(|o| o.into()),
            chunk_size.unwrap_or(45),
            chunk_padding.unwrap_or(3),
        )?;
        Ok(PyRealtimeSpeechStream(stream))
    }
```

to:

```rust
    fn synthesize_streamed(
        &self,
        text: String,
        audio_output_config: Option<PyAudioOutputConfig>,
        chunk_size: Option<usize>,
        chunk_padding: Option<usize>,
    ) -> PyDengjenResult<PyRealtimeSpeechStream> {
        let stream = self.0.synthesize_streamed(
            text,
            audio_output_config.map(|o| o.into()),
            chunk_size.unwrap_or(45),
            chunk_padding.unwrap_or(3),
            CancellationToken::new(),
        )?;
        Ok(PyRealtimeSpeechStream(stream))
    }
```

This keeps the Python-facing method signature unchanged — cancellation isn't exposed to Python callers by this plan, only a fresh never-cancelled token is passed through internally so the crate compiles against the new Rust API.

- [ ] **Step 5: Fix `crates/dengjen/synth/src/tests.rs:26`**

```rust
    let stream = synth.synthesize_streamed(text, output_config, 72, 3, dengjen_core::CancellationToken::new())?;
```

- [ ] **Step 6: Fix `crates/dengjen/synth/src/benchmarks.rs:51,91`**

Both occurrences currently read `.synthesize_streamed(text.clone(), output_config.clone(), 72, 3)`. Change each to:

```rust
                    .synthesize_streamed(text.clone(), output_config.clone(), 72, 3, dengjen_core::CancellationToken::new())
```

(keep each call's existing surrounding indentation and chained `?`/`.unwrap()` — only the argument list changes).

- [ ] **Step 7: Run the full workspace build**

Run: `cargo build --workspace`
Expected: builds cleanly (per the README's caveat, `cargo test --workspace` from the root may not pick up per-package `.cargo/config` — that's addressed per-crate in Steps 8-9, not here).

- [ ] **Step 8: Run each touched crate's own test suite**

```bash
(cd crates/frontends/cli && cargo test)
(cd crates/frontends/grpc && cargo test)
(cd crates/frontends/python && cargo test)
(cd crates/dengjen/synth && cargo test --lib)
```
Expected: all PASS. (`synth`'s `src/tests.rs`/`src/benchmarks.rs` integration targets still require the gitignored model fixtures per the README — they're allowed to report "no model found" rather than a compile error; a compile error here would indicate this task's fix is wrong.)

- [ ] **Step 9: Commit**

```bash
git add crates/frontends/cli/src/main.rs crates/frontends/grpc/src/main.rs crates/frontends/python/src/lib.rs crates/dengjen/synth/src/tests.rs crates/dengjen/synth/src/benchmarks.rs
git commit -m "frontends: pass a no-op CancellationToken through updated synthesize_streamed call sites"
```

---

### Task 6: `libdengjenCancel` C-ABI entry point

**Files:**
- Modify: `crates/frontends/capi/src/lib.rs` — imports (`:2`), `DengjenVoice` struct/impls (`:38-63`), `_do_synthesize`'s `SYNTH_MODE_REALTIME` arm (`:434-437`), add new export near the other `libdengjen*` functions (after `:339`, alongside `libdengjenSpeak`)

**Interfaces:**
- Consumes: `CancellationToken` (Task 1), `DengjenSpeechSynthesizer::synthesize_streamed(..., cancel_token)` (Task 3).
- Produces: `pub unsafe extern "C" fn libdengjenCancel(voice_ptr: *mut DengjenVoice, out_error: &mut ExternError)` — callable from another thread than the one running `libdengjenSpeak`.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block at the end of `crates/frontends/capi/src/lib.rs`, following the existing null-pointer-safety pattern used by `speak_null_voice_returns_null_pointer_error_without_panicking`:

```rust
    #[test]
    fn cancel_null_voice_returns_null_pointer_error_without_panicking() {
        let mut out_error = ExternError::default();
        unsafe {
            libdengjenCancel(std::ptr::null_mut(), &mut out_error);
        }
        assert_eq!(out_error.get_code().code(), error_codes::NULL_POINTER);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd crates/frontends/capi && cargo test cancel_null_voice`
Expected: FAIL to compile — "cannot find function `libdengjenCancel`".

- [ ] **Step 3: Restructure `DengjenVoice` to hold an active-cancel-token slot**

Change the import line near the top of the file:

```rust
use dengjen_core::{AudioSamples, CancellationToken, DengjenError, DengjenModel, DengjenResult};
```

and add `Mutex` to the existing `std::sync` import:

```rust
use std::sync::{Arc, Mutex, Once};
```

Replace the `DengjenVoice` definition and its trait impls (currently `:38-63`):

```rust
pub struct DengjenVoice {
    synth: AssertUnwindSafe<Arc<DengjenSpeechSynthesizer>>,
    active_cancel_token: Arc<Mutex<Option<CancellationToken>>>,
}

impl From<DengjenSpeechSynthesizer> for DengjenVoice {
    fn from(other: DengjenSpeechSynthesizer) -> Self {
        Self {
            synth: AssertUnwindSafe(Arc::new(other)),
            active_cancel_token: Arc::new(Mutex::new(None)),
        }
    }
}

impl Deref for DengjenVoice {
    type Target = DengjenSpeechSynthesizer;

    fn deref(&self) -> &Self::Target {
        &self.synth
    }
}

impl<T> AsRef<T> for DengjenVoice
where
    T: ?Sized,
    <DengjenVoice as Deref>::Target: AsRef<T>,
{
    fn as_ref(&self) -> &T {
        self.deref().as_ref()
    }
}
```

- [ ] **Step 4: Add `libdengjenCancel`**

Add after `libdengjenSpeak` (currently ending at `:339`):

```rust
/// # Safety
/// If non-null, the pointer must be well-aligned and point to a valid `DengjenVoice`. A null
/// pointer is handled gracefully (returns a NULL_POINTER error via `out_error`). Safe to call
/// from a different thread than the one that called `libdengjenSpeak` — that's the point.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn libdengjenCancel(voice_ptr: *mut DengjenVoice, out_error: &mut ExternError) {
    let Some(voice) = voice_ptr.as_ref() else {
        *out_error = DengjenFFIError::null_pointer("voice_ptr").into();
        return;
    };
    if let Some(token) = voice.active_cancel_token.lock().unwrap().as_ref() {
        token.cancel();
    }
}
```

- [ ] **Step 5: Wire the token into the realtime synthesis path**

`libdengjenSpeak` currently does `let synth = AssertUnwindSafe(Arc::clone(&voice.0));` — change `&voice.0` to `&voice.synth`, and also clone the cancel-token slot to pass down:

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
    let synth = AssertUnwindSafe(Arc::clone(&voice.synth));
    let cancel_slot = Arc::clone(&voice.active_cancel_token);
    call_with_result(out_error, move || _synthesize(synth, cancel_slot, text_ptr, params))
}
```

Update `_synthesize` and `_do_synthesize` signatures to accept and use `cancel_slot: Arc<Mutex<Option<CancellationToken>>>`:

```rust
fn _synthesize(
    synth: AssertUnwindSafe<Arc<DengjenSpeechSynthesizer>>,
    cancel_slot: Arc<Mutex<Option<CancellationToken>>>,
    text_ptr: FfiStr,
    params: SynthesisParams,
) -> DengjenFFIResult<()> {
    let text = text_ptr
        .into_opt_string()
        .ok_or_else(DengjenFFIError::invalid_utf8)?;
    if params.nonblocking != 0 {
        SYNTHESIS_THREAD_POOL.spawn(move || {
            let callback = params.callback;
            if let Err(e) = _do_synthesize(synth, cancel_slot, text, params) {
                let event = SynthesisEvent::with_error(e);
                callback(event);
            }
        });
    } else {
        _do_synthesize(synth, cancel_slot, text, params)?;
    }
    Ok(())
}

fn _do_synthesize(
    synth: AssertUnwindSafe<Arc<DengjenSpeechSynthesizer>>,
    cancel_slot: Arc<Mutex<Option<CancellationToken>>>,
    text: String,
    params: SynthesisParams,
) -> DengjenFFIResult<()> {
    let audio_output_config = Some(params.as_synth_output_config());
    match params.mode {
        synth_mode::SYNTH_MODE_LAZY => {
            let stream = synth
                .synthesize_lazy(text, audio_output_config)?
                .map(|wr| wr.map(|aud| aud.samples));
            iterate_stream(stream, params.callback)
        }
        synth_mode::SYNTH_MODE_PARALLEL => {
            let stream = synth
                .synthesize_parallel(text, audio_output_config)?
                .map(|wr| wr.map(|aud| aud.samples));
            iterate_stream(stream, params.callback)
        }
        synth_mode::SYNTH_MODE_REALTIME => {
            let cancel_token = CancellationToken::new();
            *cancel_slot.lock().unwrap() = Some(cancel_token.clone());
            let stream = synth.synthesize_streamed(text, audio_output_config, 72, 3, cancel_token)?;
            let result = iterate_stream(stream, params.callback);
            *cancel_slot.lock().unwrap() = None;
            result
        }
        _ => Err(DengjenFFIError::invalid_synthesis_mode())
    }
}
```

Also update `_synthesize_to_file` and `libdengjenSpeakToFile`'s use of `&voice.0` to `&voice.synth` (it doesn't need `cancel_slot` — file synthesis always uses `synthesize_to_file`, which goes through `synthesize_parallel`, not the realtime path).

Lazy/parallel modes intentionally leave `cancel_slot` untouched (stays `None`) — `libdengjenCancel` is a no-op during those modes, matching the design doc's documented scope boundary (cancellation targets the realtime/streaming path only).

- [ ] **Step 6: Run test to verify it passes**

Run: `cd crates/frontends/capi && cargo test`
Expected: PASS, including the new `cancel_null_voice_returns_null_pointer_error_without_panicking` and all pre-existing null-pointer-safety tests (unaffected by the `.0` → `.synth` rename since they only exercise the null-pointer branch).

- [ ] **Step 7: Regenerate the C header**

Run: `cd crates/frontends/capi && cargo build`
Expected: `libdengjen.h` is rewritten by the `cbindgen` build script (see `build.rs`) to include the new `libdengjenCancel` declaration — no manual header editing.

- [ ] **Step 8: Commit**

```bash
git add crates/frontends/capi/src/lib.rs crates/frontends/capi/libdengjen.h
git commit -m "capi: add libdengjenCancel entry point for the realtime synthesis path"
```

---

### Task 7: Fix broken `benchmarks.rs` import (issue #3)

**Files:**
- Modify: `crates/audio/ops/benches/benchmarks.rs`

**Interfaces:** None — self-contained fix, no dependency on Tasks 1-6.

- [ ] **Step 1: Confirm the current failure**

Run: `cd crates/audio/ops && cargo check --benches`
Expected: `error[E0432]: unresolved import audio_ops::RawAudioSamples: no RawAudioSamples in the root`.

- [ ] **Step 2: Fix the import**

`crates/audio/ops/benches/benchmarks.rs` currently imports a type that was renamed/never existed at the crate root (`audio_ops::lib.rs` exports `AudioSamples`, not `RawAudioSamples`). Replace both occurrences:

```rust
use audio_ops::AudioSamples;
use divan::Bencher;

fn main() {
    divan::main();
}

pub fn samples_generator() -> impl Fn() -> (AudioSamples, AudioSamples) {
    let data = Vec::from_iter((0..441000).map(|i| i as f32));
    move || (data.clone().into(), data.clone().into())
}

#[divan::bench]
fn bench_overlap_with(bencher: Bencher) {
    bencher
        .with_inputs(samples_generator())
        .bench_refs(|(s1, s2)| s1.overlap_with(s2));
}
```

- [ ] **Step 3: Run to verify it compiles and runs**

Run: `cd crates/audio/ops && cargo bench --bench benchmarks -- --sample-count 10`
Expected: compiles and runs (a short divan benchmark report for `bench_overlap_with`, no errors).

- [ ] **Step 4: Commit**

```bash
git add crates/audio/ops/benches/benchmarks.rs
git commit -m "audio-ops: fix broken RawAudioSamples import in benchmarks.rs (#3)"
```

---

## Final check

- [ ] Run `cargo build --workspace` — clean build.
- [ ] Run each crate's tests individually per the README's per-package caveat: `for c in crates/dengjen/core crates/dengjen/synth crates/dengjen/models/piper crates/frontends/capi crates/frontends/cli crates/frontends/grpc crates/audio/ops; do (cd "$c" && cargo test); done` — all green (module tests only; the two gitignored-fixture-gated integration targets in `dengjen-synth` are expected to skip/fail on missing model files exactly as they do today, unrelated to this plan).
- [ ] Close GitHub issue #3 (fixed by Task 7).

## Out of scope (tracked separately)

- Kokoro-class model backend (design doc's model-class decision) — needs its own plan; phonemizer (`misaki-rs`) and ONNX I/O contract (`input_ids`/`style`/`speed` tensors, confirmed against the `kokoroxide` reference implementation) are already researched and ready to turn into a plan next.
- Splitting `crates/dengjen/models/piper/src/lib.rs` (1354 lines) along backend/orchestration lines — a mechanical refactor, deliberately kept out of this plan per "split mechanical from semantic changes."
- Automated TTFA (<150ms) and memory (<1GB) benchmarks against the design spec's targets (design doc §5) — needs the real-voice model fixtures this repo doesn't currently ship (per the README's testing caveat) and a benchmark harness; this plan only makes cancellation itself correct and tested, it doesn't measure the latency budget.
- NVDA add-on and packaging subsystems (specs 2 and 3) — not started.
