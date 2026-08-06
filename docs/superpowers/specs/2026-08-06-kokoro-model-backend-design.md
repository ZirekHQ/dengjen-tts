# Kokoro Model Backend — Design

**Date:** 2026-08-06

## Goal

Add a second `DengjenModel` backend — Kokoro-class, per the Core Engine spec's model-class decision
(`docs/superpowers/specs/2026-08-06-core-engine-rewrite-design.md`, "Model class" section) — proving
the trait-based backend architecture actually generalizes beyond Piper, and wiring it into the CLI
frontend so it's invokable end-to-end, not just a library nobody can call.

## Scope

- New `crates/dengjen/models/kokoro` crate implementing `DengjenModel` (no changes to the trait
  itself — Piper already established it fits this shape).
- CLI wiring: auto-detect Piper vs. Kokoro voice configs and dispatch to the right backend.
- Out of scope: capi/grpc/python wiring (a separate follow-up once the crate proves out via CLI);
  the sample-level streaming chunking discussed below (tracked as issue #20); non-English languages
  beyond what espeak-ng already supports for Piper.

## Backend selection (CLI)

No new CLI flag. The CLI sniffs the voice config's JSON shape — Kokoro configs declare
`"model_type": "kokoro"`; Piper configs (the existing upstream `.onnx.json` format) don't have that
field. Dispatch logic lives in `crates/frontends/cli/src/main.rs` only — this is the one consumer
being wired up now (per Scope, above); no shared abstraction is built for a single caller. capi/grpc/
python keep loading Piper voices directly, unchanged, until their own follow-up wiring work.

## Config manifest

Unlike Piper (whose `.onnx.json` config is a de facto ecosystem standard dengjen already parses),
Kokoro has no single standard config format — reference implementations (onnx-community,
thewh1teagle, hexgrad) each lay out the `.onnx` model, voice style vectors, and vocab/tokenizer
differently. dengjen defines its own minimal manifest, the same role Piper's config plays, just
dengjen-native since no upstream standard exists:

```json
{
  "model_type": "kokoro",
  "model_path": "model.onnx",
  "voices_path": "voices.bin",
  "vocab_path": "tokenizer.json",
  "sample_rate": 24000,
  "voices": ["af_heart", "am_adam", "bf_emma"]
}
```

Paths are relative to the config file's directory, matching Piper's convention.

## Entry point

`dengjen_kokoro::from_config_path(&Path) -> DengjenResult<Arc<dyn DengjenModel + Send + Sync>>`,
mirroring `dengjen_piper::from_config_path`'s exact shape so the CLI's dispatch is a straight
if/else between two otherwise-identical calls.

## Phonemization

Per-voice-config language tag picks the phonemization path — this is backend-owned dispatch, not a
new centralized phonemizer abstraction (matches the Core Engine spec's existing "phonemize is
backend-owned" architecture decision):

- **`en-us` / `en-gb`**: `misaki-rs` (crates.io, `0.3.0`) — a self-contained, POS-aware English G2P
  engine built specifically for Kokoro, no espeak-ng involved. API: `G2P::new(is_british: bool)`,
  `.g2p(text: &str) -> (String, tokens)`.
- **Everything else** (Japanese, Chinese, Spanish, French, Hindi, Italian, Portuguese — Kokoro-82M's
  other supported languages): `espeak-ng`, via the existing in-repo `espeak-phonemizer` crate, kept
  as an optional Cargo feature (default-on, matching Piper's current `espeak` feature) — this is
  about build footprint (not everyone wants the espeak-ng C dependency), not license, since the
  relicense to GPL-3.0-or-later removes any license reason to gate it.

espeak-ng's raw IPA output does not map directly onto Kokoro's phoneme-to-token-ID vocabulary — it
needs a conversion layer first (matching the approach the `kokoroxide` reference implementation
uses: espeak IPA → Kokoro/misaki phoneme notation → token IDs). This is real, necessary
implementation work, not a thin feature-flag passthrough; the implementation plan treats it as its
own task with real test cases (known IPA input → expected Kokoro phoneme output pairs), not a
one-line adapter assumed to just work.

## ONNX inference

Uses the workspace's existing `ort` 2.0.0-rc.13, matching Piper's exact idiom
(`ort::inputs![Tensor::from_array(...).unwrap(), ...]`, positional binding, `session.run(inputs)`,
`outputs[0].try_extract_tensor::<f32>()`). Three inputs, confirmed against a real working Rust+ort
reference implementation (`kokoroxide`, not guessed or reconstructed from memory):

| Input | Shape | Dtype |
|---|---|---|
| `input_ids` | `(1, seq_len)` | i64 |
| `style` | `(1, 256)` | f32 |
| `speed` | `(1,)` | f32 |

Output: a single f32 waveform tensor, sample rate from the config (24kHz for standard Kokoro-82M
exports).

## Voice style vectors

`voices.bin` is not a single 256-float vector per voice — it's length-conditioned (a per-voice table
of style vectors indexed by token count, per every reference implementation surveyed). The exact
byte layout needs confirming against a real downloaded `voices.bin`, not assumed here from
documentation alone; the implementation plan's first task includes that verification as a concrete,
falsifiable step (load a real file, assert the shape matches what's expected) rather than code that
silently assumes a layout that turns out wrong.

## Streaming

Piper's ONNX graph splits into an encoder and a chunkable decoder, so `SYNTH_MODE_REALTIME` can cut
a single long sentence into sub-chunks mid-generation. Kokoro-82M's published ONNX exports run the
whole StyleTTS2 vocoder in one forward pass per sentence — there's no natural mid-sentence cut
point; the entire sentence must finish computing before any audio exists.

**Decision:** Kokoro v1 does not implement `stream_synthesis` — it stays the trait's default (returns
an error), same as Piper's own non-streaming `VitsModel` does today. Kokoro implements only
`speak_one_sentence`/`speak_batch`, used by the `SYNTH_MODE_LAZY`/`SYNTH_MODE_PARALLEL` orchestration
paths, which already chunk at sentence boundaries and support between-sentence cancellation via the
existing orchestration layer (`dengjen-synth`). This isn't a gap — sentence-boundary granularity is
Kokoro's actual minimum unit of work.

A possible future improvement — slicing a completed sentence's waveform into fixed-size chunks and
yielding them progressively through `stream_synthesis` for smoother playback delivery and
finer-grained cancellation checks (without reducing actual synthesis latency, since the whole
sentence is still computed before any of it is available) — is tracked as issue #20, not built now.

## Error handling

Reuses `dengjen_core::DengjenError`'s existing variants (`FailedToLoadResource`,
`PhonemizationError`, `OperationError`) — no new error type. Config-parse failures, missing
model/voices/vocab files, unknown voice names, and phonemization failures (including espeak-ng
conversion failures) all map onto these three variants the same way Piper's errors do today.

## Testing pyramid

Built in from the start, not retrofitted — this reuses the tiered strategy the repo's existing
Piper test-coverage plan (`docs/superpowers/plans/2026-08-05-test-coverage-tier1.md`) established,
applied to a backend that doesn't have years of untested legacy code to catch up on.

**Tier 1 — pure logic, no fixtures:** config parsing (valid/malformed/missing-file), the CLI's
Piper-vs-Kokoro auto-detect dispatch, vocab/tokenizer-map loading, the espeak-IPA→Kokoro-phoneme
conversion (known input/output pairs), voice-style-vector lookup/indexing (once the real
`voices.bin` layout is confirmed), unknown-voice-name and empty-text error paths.

**Tier 2 — real inference, fast, deterministic:** Kokoro-82M's smallest real quantized export is
tens of MB, too large to vendor as a fixture the way a tiny Piper voice can be. Instead, a small
**synthetic ONNX graph** matching Kokoro's exact I/O contract (`input_ids`/`style`/`speed` in,
waveform out) gets checked into the repo — deterministic, network-free in CI, proves the plumbing
(phonemization → token IDs → tensor construction → inference call → audio bytes) without asserting
anything about real speech quality. Same tradeoff the Piper test-coverage plan made for its own
Tier 2, applied consistently here.

**Tier 3 — real voice, e2e per frontend, skippable:** a real downloaded Kokoro voice exercises
actual inference quality through the CLI (subprocess test: spawn the binary, feed text, assert a
valid non-empty WAV) and through `dengjen-synth`'s orchestration (lazy/parallel modes, mirroring
`dengjen-synth`'s existing `test_lazy_stream`/`test_parallel_stream` tests). Skips gracefully if the
voice isn't available offline, matching the repo's existing convention for Piper's own Tier 3 — not
a new pattern introduced here.

## Out of scope

- capi/grpc/python wiring — CLI only, per Scope above; separate follow-up.
- Sample-level streaming chunking — tracked as issue #20, not built now.
- Non-English languages beyond what espeak-ng already supports for Piper.
- Publishing/distributing actual Kokoro voice files — this spec covers loading a voice a user
  already has, not sourcing one.
