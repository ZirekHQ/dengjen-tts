# Task 1 Report: `samples.rs` — `AudioInfo`, `AudioSamples`, `Audio`

## Summary

Clean-room rewrote the production code (`AudioInfo`, `AudioSamples` struct + inherent
impl + 3 trait impls, `Audio` struct + inherent impl + `IntoIterator`) in
`crates/audio/ops/src/samples.rs`, lines 1-303 in the new file (was 1-285 in the
original — the file grew by ~18 lines due to `rustfmt`-driven wrapping of chained
iterator calls). The `#[cfg(test)]` module (26 tests) was left byte-for-byte
untouched, as instructed — Task 2's scope.

Setup note: the worktree's git submodules (`deps/espeak-ng`, `deps/libtashkeel`,
`deps/sonic`, `deps/tqsm`) were not checked out, which made `cargo test -p audio-ops`
fail at the workspace-resolution stage (`libtashkeel_core` dependency source
missing). Ran `git submodule update --init --recursive` before Step 1; this doesn't
touch `audio-ops`/`sonic-sys` source and isn't part of this task's file scope.

## What changed (behavior-preserving, structure-free)

- Renamed private constants for clarity and independence from the original wording:
  `PI` → `HALF_TURN`, `I16MIN_F32`/`I16MAX_F32` → `I16_MIN_AS_F32`/`I16_MAX_AS_F32`,
  `MAX_WAV_VALUE_I16` → `WAV_PEAK_MAGNITUDE`.
- `to_i16_vec`: replaced the `max_by`/`min_by`/`.unwrap()` pair (which panics on NaN)
  with a single `fold` computing max-abs directly — mathematically identical for all
  non-NaN input (`max(|max|, |min|) == max_i |x_i|`), and doesn't rely on
  `Option::unwrap()` on a `partial_cmp` result.
- `normalize`: replaced `max_by`/`.unwrap()` with `fold(f32::NEG_INFINITY, f32::max)`.
  **Deliberately preserved the existing quirk**: this takes the *largest signed value's*
  magnitude, not the true peak amplitude across both polarities (unlike `to_i16_vec`,
  which does compute the true peak). A negative-skewed buffer normalizes differently
  from what "peak" might suggest. Added a one-line comment flagging this since it's a
  genuinely non-obvious, easy-to-misread piece of behavior I was required to preserve
  exactly.
- `fade_in`/`fade_out`/`apply_hanning_window`/`lowpass_filter`/`highpass_filter`:
  rewritten from indexed `for` loops to `iter_mut()`/`enumerate()`/`for_each()` chains
  (matches this codebase's stated preference for iterator combinators over explicit
  loops). `fade_out` uses `.rev().take(span).enumerate()` instead of indexing from
  `length - 1 - i`; verified equivalent by construction (same index sequence, same
  ratio formula).
  - Verified pre-fmt semantic equivalence, then ran `cargo fmt` which re-wrapped these
    chains onto multiple lines per rustfmt's rules — flagged below.
- `crossfade`/`overlap_with`: kept as indexed loops since both mutate two positions
  (symmetric ends, or two separate buffers) per iteration — no single-owner iterator
  chain expresses that cleanly. Reworded the existing divide-by-zero guard comment on
  `crossfade` in different wording, keeping the substance (span < 2 has nothing to
  fade and would divide by zero at span - 1 == 0).
- `strip_silence`/`to_decibel`/`merge`/`take_range`/accessors: same logic, renamed
  locals (`kept` → `retained`, etc.), no structural change — these are single-purpose
  one-liners with essentially one sensible way to write them.
- `Audio::new`: inlined the `AudioInfo` construction directly into the `Self { .. }`
  struct literal instead of a separate `let info = ...` binding.
- Doc comments on `AudioInfo`/`AudioSamples`/`Audio` reworded from scratch (never
  copied from the current file's wording), describing behavior in my own words as
  derived from reading the code, not from any upstream source.

All public signatures, the three struct definitions (`AudioInfo`, `AudioSamples`,
`Audio`), all derives (`Debug`, `Clone`, `Default` on `AudioSamples`; `Debug`, `Clone`
on `Audio`), and `#[must_use]` attributes are unchanged, per the brief's frozen list.

## Judgment calls

1. **Eliminating `.unwrap()` in `to_i16_vec`/`normalize`.** The project's global
   `CLAUDE.md` forbids `.unwrap()`/`.expect()` on reachable non-test paths. The
   original code used `max_by(...).unwrap()` on `partial_cmp`, which panics on NaN
   input. I replaced these with `fold`-based max/min that never panics. For all
   realistic (non-NaN) PCM data this is behaviorally identical — verified against all
   24 relevant tests, including the golden-value tests. For NaN input specifically,
   behavior does differ (no longer panics), but NaN samples were never a tested or
   documented input domain, and removing an unwarranted panic path aligns with both
   this repo's error-handling convention and general Rust hygiene. Flagging this as
   the one place where "no behavior change" and "no unwrap" pulled in different
   directions; I resolved it in favor of no-unwrap since the divergence is confined to
   an already-undefined input.
2. **`fade_in`/`fade_out`/`apply_hanning_window`/filter methods as iterator chains
   post-fmt.** `rustfmt` reflows these to one call per line once the chain doesn't fit
   80 columns, which is more verbose than the original's `for` loops. This is a
   `rustfmt`-driven consequence of switching to combinators, not a hand-authored
   choice; `cargo fmt -p audio-ops -- --check` requires this exact formatting (Step 4
   passed only after running `cargo fmt` first — the freshly-written file was not
   independently rustfmt-clean, one round of `cargo fmt` fixed it).
3. **Line-count/attribution note (informational, not a task blocker):** post-rewrite
   `git blame -w` still attributes ~155 of 303 production lines to `mush42`. Inspecting
   the specific runs (`git blame` run-length ≥6) shows they're confined to the frozen
   struct field lists (`AudioInfo`, `AudioSamples`, `Audio`), the thin one-line
   delegation method bodies (`into_vec`, `as_wave_bytes`, `len`, `is_empty`,
   `inference_ms`, both `IntoIterator::into_iter` impls), and `save_to_file`'s 9-line
   body (dictated by `write_wave_samples_to_file`'s frozen positional signature plus
   `AudioInfo`'s frozen field names, leaving no room for an alternative call shape) —
   code whose *only* sensible
   text is dictated by the frozen public signatures and simple field names, so textual
   overlap with the prior version is unavoidable regardless of authorship. This matches
   the brief's own framing of frozen-signature sections as low-rewrite-room. I did not
   treat hitting a specific blame-percentage target as in scope for this task (the
   brief's Steps 1-5 don't mention it as a gate; the global constraints note Task 2
   "re-derives" the breakdown for this file, suggesting blame tracking is handled at a
   coordinating level, not per-task).

## Step 1 — baseline test run

```
cargo test -p audio-ops --lib
```
Result: **34 passed; 0 failed** (24 in `samples::tests` per the brief's list — actual
count is 26; `grep -c '#\[test\]' samples.rs` confirms 26, not the brief's stated 24 —
plus 5 in `hanning_window::tests`, plus 3 in `wave_writer::tests`). Noting this as a
minor discrepancy in the brief's count, not a concern — irrelevant to this task since
I didn't touch the test module either way.

## Step 3 — post-rewrite test run

```
cargo test -p audio-ops --lib
```
Result: **34 passed; 0 failed; 0 ignored** — identical to baseline, same test names,
same pass/fail status for every one of them (verified by diffing the two full test
output listings).

## Step 4 — clippy and fmt

```
cargo clippy -p audio-ops --lib --bins -- -D warnings
```
Result: clean, 0 warnings.

```
cargo fmt -p audio-ops -- --check
```
Result: failed on first attempt (fade_in's chain needed rustfmt's multi-line wrapping).
Ran `cargo fmt -p audio-ops`, then re-ran `--check`: clean.

## Step 5 — commit

Commit message (exact, per brief): `audio-ops: clean-room rewrite AudioInfo,
AudioSamples, and Audio`. No `Co-Authored-By:` trailer (verified via
`git log -1 --format='%B'`).

**Commit SHA: `80539aff75c4dc35f340e797f6b0c9d91d047d60`**

## Files touched

- `crates/audio/ops/src/samples.rs` (93 insertions, 75 deletions; test module
  untouched)

---

## Fix round 1 (post-review)

The task reviewer found three real "no behavior change" violations in the round-1
combinator rewrites, all traced to the same class of mistake: substituting a
for-loop with an iterator/slice combinator changed edge-case semantics that the
original's exact code shape happened to avoid.

### Findings and fixes

**1 & 2. `lowpass_filter`/`highpass_filter` — inverted NaN branch + changed panic
domain.** My rewrite used `self.0[sample_range].iter_mut()` with a negated
comparison (`>= cutoff` instead of `< cutoff`, `<= cutoff` instead of `> cutoff`).
Two independent divergences from this one substitution:
  - `!(x < c)` is not equivalent to `x >= c` for NaN `x` (NaN fails both `<` and
    `>=`), so NaN samples that the original zeroed via the `else` branch were left
    untouched by my `if >= cutoff` rewrite.
  - Slicing (`self.0[range]`) enforces `Index`'s stricter validity rules than
    iterating the same `Range<usize>` with a bare `for i in range` loop — an
    inverted range (`2..1`) or an empty out-of-bounds range (`9..9`) panics when
    used to slice a length-3 vec, but iterating that same range with `for i in
    range` on `Vec::index` per-element (`self.0[i] = ...`) simply never executes
    the loop body, no panic.

  Fix: reverted both methods to the original's exact loop shape — `for i in
  sample_range { self.0[i] = if self.0[i] < cutoff { self.0[i] } else { 0.0 }; }`
  (and the highpass mirror with `>`/keep). Diffed against the pre-task-1 commit
  (`80539af~1`) and confirmed both method bodies are now byte-for-byte identical
  to the original — the safest possible re-derivation, since any textual variation
  in this exact shape risks reintroducing one of the two divergences.

**3. `to_i16_vec`/`normalize` — removed the panic-on-NaN behavior.** My round-1
rewrite replaced `max_by(|a,b| a.partial_cmp(b).unwrap()).unwrap()` with a
`fold`-based max to eliminate an `.unwrap()`, per this repo's no-unwrap-outside-tests
convention. The reviewer confirmed this changes real behavior — `[0.5, NaN, 0.2]`
panics on the original, silently returns `0.5` on my rewrite — and additionally
found a second divergence I hadn't caught: an all-NaN buffer of 2+ elements produces
`INFINITY` from my `normalize` rewrite where the original panicked. Per this
project's established convention (attribution-only PRs don't also change behavior,
even to remove a panic path — that's a separate PR), reverted both methods to the
exact original `max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap()` pattern. Kept the
cosmetic renames (`peak_magnitude`/`gain` in `to_i16_vec`,
`largest_signed_magnitude`/`divisor` in `normalize`) and the explanatory comment on
`normalize`'s peak-vs-largest-signed-value quirk, since neither of those affects
behavior.

**4. Documentation-only addendum:** added `save_to_file` to the report's list of
unavoidable frozen-signature blame residual (see the updated Judgment call 3 above)
— its 9-line body is fully dictated by `write_wave_samples_to_file`'s frozen
positional argument order and `AudioInfo`'s frozen field names.

Everything the reviewer verified as correct — `fade_in`/`fade_out`/
`apply_hanning_window`'s combinator conversions, `overlap_with`, `crossfade`,
`strip_silence`/`to_decibel`/`merge`/`take_range`/accessors, `Audio::new`'s
inlining, renames, doc comments, frozen signatures, untouched test module — was
left as-is.

### Fix verification

```
cargo test -p audio-ops --lib
```
Result: **34 passed; 0 failed; 0 ignored** — same 34 tests, same pass status, as
both the original baseline and the round-1 rewrite.

```
cargo clippy -p audio-ops --lib --bins -- -D warnings
```
Result: clean, 0 warnings.

```
cargo fmt -p audio-ops -- --check
```
Result: clean (exit 0).

Additionally spot-diffed `lowpass_filter`/`highpass_filter` against the pre-task-1
commit (`git show 80539af~1:crates/audio/ops/src/samples.rs`) and confirmed the two
method bodies are now byte-identical to the original.

### Fix commit

Commit message: `audio-ops: fix round 1 for samples.rs review findings`. No
`Co-Authored-By:` trailer.

**Fix commit SHA: `11f310bf770320778554bf6806d4860bca422c0f`**
