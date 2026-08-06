# Core Engine Rewrite (NVDA LLM TTS, Subsystem 1) — Design

**Date:** 2026-08-06

## Goal

Rework dengjen's model/inference layer into a pluggable architecture that can serve as the native
Core Engine for an NVDA screen-reader TTS add-on (per `nvda_llm_tts_design_specification.md`),
while remaining a general-purpose, cross-platform TTS library for its existing consumers.

This is the first of three specs. The source spec describes three largely independent subsystems:

1. **Core Engine** (this repo, this spec) — native Rust library + C-ABI.
2. **NVDA add-on** (new, Python `synthDriver`) — consumes the Core Engine's C-ABI. Speced separately,
   after this C-ABI is implemented.
3. **Packaging/build pipeline** (Windows cross-compile + `.nvda-addon` zip) — speced after #2.

Only #1 is in scope here.

## Framing: evolution, not a ground-up rewrite

Most of dengjen's current shape survives: Rust workspace, ONNX Runtime (`ort`) for inference,
multiple frontends (C-ABI, CLI, gRPC, Python bindings). What changes is the model layer (made
backend-pluggable) and the C-ABI contract (streaming + explicit cancellation). The source spec's
own C-ABI sketch (`tts_init_engine` / `tts_synthesize_chunk` / `tts_free_engine`, a bare polling
loop) is treated as illustrative, not literal — dengjen's existing callback-based streaming C-ABI
is already more capable and is evolved rather than replaced.

**Amendment (2026-08-06, post-cancellation-PR):** architecturally this remains evolution, not a
stop-everything rewrite — no standalone rewrite project, and existing working code (including the
cancellation work already shipped) stays as-is. But code *provenance* now has a direction: per the
relicensing decision below, whenever a part of the codebase is touched for a real reason (a new
backend, a bug fix, a feature), it gets rewritten cleanly rather than preserving old patterns, so
code substantially derived from the original MIT-licensed Sonata fork shrinks toward zero over time
as a byproduct of real work — not a dedicated effort.

## Model class

The spec calls this "LLM-powered TTS," but its own non-functional requirements (<150ms TTFA,
<1GB RAM, CPU-first INT8/INT4) match small non-autoregressive models (Piper/VITS, Kokoro-class),
not multi-GB autoregressive LLM-TTS (Orpheus, Dia, CSM), which run in the seconds-per-request,
multi-GB range. Resolution: the engine's model backend interface is architecture-agnostic and can
host either class, but v1 targets only the small/fast class:

- **Piper** (VITS) — existing backend, adopts the new trait.
- **Kokoro-class** — new backend.
- **Genuine autoregressive LLM-TTS** (Orpheus/Dia/CSM-class) — not built in v1. The backend trait's
  streaming/cancellable shape is designed so one can be added later without a redesign, with the
  understanding its latency/memory profile won't meet the <150ms/<1GB targets.

## Architecture

### Model backend trait

A new `ModelBackend` trait, implemented per model family:

- `load(config) -> Self`
- `phonemize(text) -> Phonemes` — backend-owned, not centralized. Piper keeps espeak-ng
  (optional feature, see Licensing below) and libtashkeel for Arabic; Kokoro brings its own
  phonemizer. This lets each backend use the phonemization strategy it actually needs instead of
  forcing every family through one shared interface.
- `synthesize_chunk(phonemes, cancel_token) -> AudioSamples` — streaming and cancellable from the
  ground up.

Dispatch is compile-time (`Box<dyn ModelBackend>`), not runtime plugin loading. Each backend lives
in its own crate under `crates/dengjen/models/` (existing `piper` crate adopts the trait; new
`kokoro` crate added alongside), gated by Cargo features. Runtime dynamic loading (`libloading`,
out-of-tree backends as separate `cdylib`s) was considered and rejected for v1: this engine runs
inside NVDA's process, where a crash in a loaded plugin takes the screen reader down with it, and
there's no concrete need yet for third-party backends. The trait boundary stays clean enough that a
`DynamicBackend: ModelBackend` wrapper could be added later if that need materializes.

### Orchestration layer

`dengjen-synth` keeps its role — chunking, parallelism modes, prosody post-processing via Sonic —
but now drives synthesis against `dyn ModelBackend` instead of being Piper-specific.

### Frontends

C-ABI, CLI, gRPC, and Python bindings are all kept and updated to the new trait boundary. The C-ABI
gets implementation priority since the NVDA add-on is the first concrete consumer of the rewritten
engine, but the others remain first-class — this stays a general-purpose library, not an
NVDA-only engine.

## C-ABI: streaming & cancellation

- Keep the existing `SpeechSynthesisCallback` delivering `SYNTH_EVENT_SPEECH` / `SYNTH_EVENT_FINISHED`
  / `SYNTH_EVENT_ERROR`, callback return value doubling as continue/stop, as today.
- Add an explicit `libdengjenCancel(voice_ptr)` entry point: sets an atomic cancellation flag
  checked between chunks inside `synthesize_chunk`. This is what an NVDA add-on's `cancel()` would
  call — it works even between chunks, not only from inside an active callback, and is symmetric
  with clearing playback buffers on the consumer side.
- **TTFA <150ms** is a chunking-strategy question, not an API one: text is segmented at clause
  boundaries (existing punctuation-based splitting in `dengjen-synth`) so the first chunk's
  synthesis time — not the whole utterance's — determines time-to-first-audio. This mechanism
  already exists via the current parallelism-mode design; the rewrite's job is verifying it against
  the 150ms budget for both Piper and Kokoro backends, not building it from scratch.

## Licensing

**Amendment (2026-08-06, post-cancellation-PR):** issue #10 is resolved — dengjen relicenses from
MIT to **GPL-3.0-or-later**, workspace-wide (every crate's `Cargo.toml` `license` field, plus the
root `LICENSE`). This removes the MIT/GPL tension entirely rather than routing around it: espeak-ng
no longer needs to be feature-gated to avoid a license conflict (the gating machinery from #14/#15
can stay as-is for now — it's no longer load-bearing, so ripping it out is optional low-risk
cleanup, not required), and adopting piper1-gpl's C++ `libpiper` runtime directly becomes
license-unblocked (still not committed to — a separate future decision if ever wanted).

GPL-3.0 permits incorporating MIT-licensed code into a GPL work, but MIT's copyright/license-text
retention requirement still applies to whatever code remains substantially unmodified from the
original Sonata fork (mush42/Musharraf Omer). That's preserved in a new `NOTICE` file rather than
blocking the relicense. Issue #18 tracks removing `NOTICE` once the opportunistic rewrite policy
(see Framing, above) has left nothing substantially derived from the original MIT-licensed code.

## Platform support

Core Engine and all frontends build and run cross-platform — Linux (x86_64/aarch64), macOS
(aarch64), Windows (x86_64/aarch64) — the same target matrix the repo's CI already builds today.
This is unaffected by the rewrite. The C-ABI itself is platform-agnostic; NVDA-on-Windows is one
consumer of it, not a constraint on the engine's design — a Linux/macOS screen reader or any other
C-ABI consumer works the same way. TTFA/memory targets apply per-platform, not just on Windows.
Windows-specific concerns (cross-compiling the `.dll`, bundling `espeak-ng.dll`, `.nvda-addon` zip
layout) belong entirely to subsystem 3 (packaging), not this spec.

## Cleanup folded into the rewrite

Touching the whole model layer makes these existing open issues natural to resolve here rather than
track separately:

- **#3** (broken `benchmarks.rs` import) and **#2** (orphaned `istft-sys`/nanosnap) — dead code the
  rewrite sheds; removed, unless Kokoro's vocoder genuinely needs `istft-sys`, in which case it's
  wired in properly instead.
- **#9** (pinyin/Hebrew phonemization) — becomes a natural extension of the new pluggable-phonemizer
  design. Not committed to for v1, but no longer architecturally blocked.
- **#14/#15/#10** (espeak GPL posture) — resolved by the relicense (see Licensing above); the
  feature-gating machinery is no longer required but can stay as optional cleanup.
- **`piper/src/lib.rs`** (currently 1354 lines) — split along the new trait boundary (backend logic
  vs. shared synth orchestration) as part of adopting `ModelBackend`.

## Testing & success criteria

- **Behavioral parity**: existing CLI/gRPC/Python frontend tests pass against the new
  `ModelBackend`-based Piper path — no regression from the trait refactor.
- **New backend proof**: Kokoro backend loads a real voice and produces audio via the same trait,
  exercised by at least one CLI-driven integration test.
- **Latency**: an automated benchmark measures TTFA (first-chunk-ready, not full-utterance) for both
  backends on CPU against the <150ms target. Missing the target is a result to report, not a silent
  gap.
- **Cancellation**: a test starts synthesis, calls `libdengjenCancel` mid-stream, and asserts
  generation stops within a bounded time with no further `SYNTH_EVENT_SPEECH` callbacks after
  cancellation.
- **Memory**: a smoke check that a loaded voice + one in-flight synthesis stays under the <1GB
  budget (approximate CPU RSS sampling — not a hard CI gate given machine variance).

## Out of scope

- NVDA Python add-on implementation (synthDriver, ctypes bridge, nvwave playback) — separate spec,
  next in sequence.
- Windows cross-compile + `.nvda-addon` packaging pipeline — separate spec, after that.
- Actual LLM-TTS (Orpheus/Dia/CSM-class) backend implementation — trait supports it later; not
  built now.
- crates.io publishing (#6), voice-download SSL issue (#7) — unrelated to the engine rewrite.
- Actually performing the relicense (LICENSE/Cargo.toml/NOTICE file changes) — a separate,
  small implementation plan, not bundled into this spec.
- Removing the `NOTICE` file (#18) — tracked separately, not a near-term action.
