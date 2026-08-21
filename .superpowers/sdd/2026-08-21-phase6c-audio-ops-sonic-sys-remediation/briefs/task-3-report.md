# Task 3 Report: `crates/audio/ops/src/wave_writer.rs` clean-room rewrite

## Summary

Whole-file clean-room rewrite of `WaveWriterError`, `write_wave_samples_to_buffer`,
`write_wave_samples_to_file`, and the entire `#[cfg(test)] mod tests` block. Behavior
unchanged; both PR #37 (`write_all`, no silent short-write truncation) and the
`WaveWriterError`'s three-branch distinguishability requirement are preserved.

## Test counts

- Actual test count before starting (brief's cited count was stale, per task instructions
  to confirm independently): `grep -c '#\[test\]' crates/audio/ops/src/wave_writer.rs` → **3**,
  not the brief's earlier reference. Confirmed by baseline run:
  `write_wave_samples_to_buffer_produces_a_valid_riff_wave_header`,
  `write_wave_samples_to_file_errors_when_parent_directory_does_not_exist`,
  `write_all_loops_past_a_short_write_until_the_full_buffer_lands` — all `ok`, 3 passed.
- After rewrite: **7 tests**, all `ok` (`cargo test -p audio-ops --lib wave_writer::tests`
  → `test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 31 filtered out`).
- Full crate suite (`cargo test -p audio-ops --lib`): `test result: ok. 38 passed; 0 failed`
  (24 `samples::tests` + 5 `hanning_window::tests`, both untouched, + 7 `wave_writer::tests`).

## Why the count grew from 3 to 7 (Step 4 coverage gap)

Reading the pre-existing test module showed a real coverage gap, not just a naming refresh:

- `write_wave_samples_to_buffer` has three distinct failure branches (`WaveWriter::new`,
  `write_sample_i16`, `sync_header`). **None were exercised** — the only test for this
  function was its success path.
- `write_wave_samples_to_file` has two failure branches (file-creation, file-write).
  Only the file-creation branch was covered (nonexistent parent directory). The third
  pre-existing test, `write_all_loops_past_a_short_write...`, did **not** call either
  production function at all — it drove a mock `Write` impl directly to demonstrate that
  `std::io::Write::write_all` itself loops past short writes. It verified stdlib behavior,
  not this file's code, so it did not actually cover the write-failure branch of
  `write_wave_samples_to_file`.

Per Step 4's instruction to close a real gap using "the rewritten code's actual failure
surface," I added:
- `ThresholdFailingWriter`, a `Seek + Write` test double that fails writes once a byte
  threshold is reached, or fails all seeks — used to hit each of `write_wave_samples_to_buffer`'s
  three branches individually (`to_buffer_errors_when_the_stream_cannot_be_opened`,
  `to_buffer_errors_when_a_sample_write_fails`, `to_buffer_errors_when_the_header_sync_fails`),
  plus one test asserting all three branches' `Display` strings are pairwise distinct
  (`to_buffer_failure_messages_stay_distinguishable_across_all_three_branches`), directly
  testing the "keep branches distinguishable" requirement from the brief.
- `to_file_errors_and_discards_the_partial_file_when_the_write_fails` (`#[cfg(unix)]`),
  using `/dev/full` as the target path — a real file that opens successfully but fails
  every `write()` with `ENOSPC`, reaching `write_wave_samples_to_file`'s actual write-failure
  branch (including the `remove_file` cleanup call) through the real filesystem rather than
  a mock.

The old short-write demonstration test was dropped as redundant: it tested `std::io::Write`
itself, not this crate's code, and its scenario (a short write later completing) is now
subsumed by exercising the real write-failure branch via `/dev/full`, which is a stronger,
more representative test of this file's actual behavior.

## Judgment calls / concerns

1. **Threshold-based failure injection (44-byte header boundary).** Rather than counting
   the `riff_wave` crate's internal `write()` call count (fragile — an implementation detail
   of a dependency), I gated `ThresholdFailingWriter` on cumulative bytes written, using the
   fact that a standard PCM WAV header (no extra chunks) is always exactly 44 bytes
   (12-byte RIFF chunk + 24-byte fmt subchunk + 8-byte data subchunk header). This is a
   format-level invariant, not an implementation detail, so it should stay stable across
   `riff-wave` version bumps. Verified by inspecting `riff-wave-0.1.3`'s `WaveWriter::new`
   source directly (registry cache) rather than assuming.
2. **`/dev/full` for the write-failure branch.** Confirmed before use that this is safe in
   this sandbox: process runs as uid 1000 (non-root), `/dev` is `root:root` mode `755`
   (`os.access('/dev', os.W_OK)` → `False`), so the `remove_file("/dev/full")` cleanup call
   inside the production write-failure path cannot actually unlink the device node — it
   fails with a permission error, which the production code already ignores by design
   (`let _ = std::fs::remove_file(path);`). Gated the test behind `#[cfg(unix)]` since
   `/dev/full` doesn't exist on Windows; the rest of the module has no such gate needed.
3. **Distinguishing the three `WaveWriterError` branches.** Kept each of the three
   `write_wave_samples_to_buffer` branches' wording distinct by describing the specific
   RIFF/WAVE operation that failed ("open the RIFF/WAVE stream for writing" /
   "append a PCM sample to the WAVE stream" / "finalize the RIFF/WAVE chunk sizes") rather
   than a shared generic message, and added a test that asserts this directly
   (`to_buffer_failure_messages_stay_distinguishable_across_all_three_branches`) rather than
   only implying it through separate branch tests.
4. **No new dependencies.** `ThresholdFailingWriter` is hand-rolled using only
   `std::io`/`std::fmt` already imported by the file — no new `Cargo.toml` entries, per the
   Global Constraints.

## Verification gates

- `cargo clippy -p audio-ops --lib --bins -- -D warnings` → clean (no output beyond
  `Checking`/`Finished`).
- `cargo fmt -p audio-ops -- --check` → clean (no output, no diff).

## Commit

`5b8ca7ff419f66dca5ebe362cd9c3598c684ece6` — `audio-ops: clean-room rewrite wave_writer.rs`
(verified via `git log -1` after committing). No `Co-Authored-By:` trailer. Exactly one file
changed: `crates/audio/ops/src/wave_writer.rs` (189 insertions, 88 deletions).

---

## Fix round 1 (reviewer findings)

Three Important + three Minor findings from task review, all addressed.

### Important 1 — destructive `/dev/full` test under a root runner

Added a root-detection guard, `this_process_can_write_into_dev()`, run at the top of the
test: it attempts `OpenOptions::new().write(true).create_new(true).open("/dev/.wave_writer_root_probe")`.
`create_new` makes the probe atomic and non-clobbering. If the open succeeds, the process
can write into `/dev` (the condition under which the production `remove_file("/dev/full")`
cleanup call would actually delete the device node) — the probe file is immediately removed
and the test returns early with an `eprintln!` explaining the skip, instead of running
against a live device. This needed no new dependency, matching every other test double in
this file. Chosen over checking `/dev`'s permission bits directly (`readonly()`/uid checks)
because it exercises the exact operation that matters — "can this process write into
`/dev`" — rather than inferring it from metadata that may not perfectly track the running
user's effective privileges (e.g. capabilities, ACLs).

### Important 2 — test name overclaimed

Renamed `to_file_errors_and_discards_the_partial_file_when_the_write_fails` to
`to_file_errors_when_the_write_fails`, matching this file's plain `..._errors_when_...`
naming style used by the other `to_file_*`/`to_buffer_*` tests. The test only asserts
`result.is_err()`; it does not and structurally cannot verify file removal against a device
node, so the name no longer implies it does.

### Important 3 — blame/attribution check

Re-ran the blame check against the working tree with this fix round's edits already applied
(291 lines, up from 269 at commit `5b8ca7f` — the added lines are the root-detection helper
and the two new single-line comments, none of them mush42-attributed since they're this
session's uncommitted edits):

```
$ git blame --line-porcelain crates/audio/ops/src/wave_writer.rs | awk '...' # run-length report
crates/audio/ops/src/wave_writer.rs: 1-1 (1 lines)
crates/audio/ops/src/wave_writer.rs: 3-4 (2 lines)
crates/audio/ops/src/wave_writer.rs: 6-10 (5 lines)
crates/audio/ops/src/wave_writer.rs: 18-20 (3 lines)
crates/audio/ops/src/wave_writer.rs: 22-25 (4 lines)
crates/audio/ops/src/wave_writer.rs: 27-35 (9 lines)
crates/audio/ops/src/wave_writer.rs: 44-44 (1 lines)
crates/audio/ops/src/wave_writer.rs: 49-51 (3 lines)
crates/audio/ops/src/wave_writer.rs: 53-60 (8 lines)
crates/audio/ops/src/wave_writer.rs: 62-62 (1 lines)
crates/audio/ops/src/wave_writer.rs: 64-68 (5 lines)
crates/audio/ops/src/wave_writer.rs: 85-85 (1 lines)

$ git blame --line-porcelain crates/audio/ops/src/wave_writer.rs | grep -c '^author '
291
$ git blame --line-porcelain crates/audio/ops/src/wave_writer.rs | grep '^author ' | grep -c 'mush42\|Musharraf'
43
```

43/291 mush42-attributed lines. Two runs reach the reviewer's ≥6-line threshold:
`27-35` (9 lines) and `53-60` (8 lines); `6-10` (5 lines) falls just short of it. Checked
what each run actually is: `27-35` is exactly `write_wave_samples_to_buffer`'s parameter
list, `where` clause, and opening brace; `53-60` is the same for
`write_wave_samples_to_file`. Both are inside the two function signatures that Global
Constraints freeze byte-for-byte (`pub fn write_wave_samples_to_buffer(...)` /
`pub fn write_wave_samples_to_file<'a, I>(path: &Path, ...) -> Result<(), WaveWriterError>
where I: Iterator<Item = &'a i16>` — called by name from `dengjen-tts-synth` and re-exported
by `dengjen-tts-core`), so their wording, line breaks, and generic bounds cannot change
without breaking a frozen public API — the same low-rewrite-room category every prior
sub-phase has documented for signature/`use`-block boilerplate. Confirmed all 43
mush42-attributed line numbers individually: every one falls in the range 1-85 (production
code); the test module, which now starts at line 88, is fully clean (0 attributed lines).
This task closes with no unavoidable run outside the two frozen signatures.

### Minors 4-6

- **Minor 4**: condensed the 3-line comment above the old `/dev/full` test to the single
  inline comment `// /dev/full always accepts open() but fails every write() with ENOSPC.`
  directly above the (renamed) test body.
- **Minor 5**: changed the test's gate from `#[cfg(unix)]` to `#[cfg(target_os = "linux")]`
  — macOS has no `/dev/full`; there, `File::create` would fail instead (wrong branch,
  silently testing file-creation-failure instead of write-failure).
- **Minor 6**: added `// write_all (not write) avoids silently truncating on a short write —
  see #34/#37` directly above the `file.write_all(&encoded)` call in
  `write_wave_samples_to_file`, restoring the "why this matters" marker the removed
  short-write test used to carry, in single-line form per this codebase's comment
  convention.

### Re-verification after fixes

- `cargo test -p audio-ops --lib wave_writer::tests` → `test result: ok. 7 passed; 0 failed;
  0 ignored; 0 measured; 31 filtered out` — same 7 tests, same pass count as before the fix
  round (one renamed, none added/removed).
- `cargo test -p audio-ops --lib` (full crate) → `test result: ok. 38 passed; 0 failed`.
- `cargo clippy -p audio-ops --lib --bins -- -D warnings` → clean.
- `cargo fmt -p audio-ops -- --check` → clean.

### Fix-round commit

Commit message: `audio-ops: fix round 1 for wave_writer.rs review findings`. No
`Co-Authored-By:` trailer. Two files changed: `crates/audio/ops/src/wave_writer.rs` and this
report. (The commit's own SHA is necessarily reported outside this file — a commit cannot
embed its own post-commit hash — see the task status reply for the exact SHA, confirmed via
`git log -1` after the final commit.)
