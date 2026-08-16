# dengjen

Rust speech-synthesis workspace (Piper/Kokoro ONNX backends, espeak-ng/tashkeel phonemization,
sonic-based prosody). The maintainer does not read Rust — quality has to come from binary
pass/fail gates and written conventions below, not from manual code review. Every rule here is
enforced by a CI job (`.github/workflows/rust-lint.yml`); if you change the rule, change the job
in the same commit.

## Error handling

- Fallible operations return `DengjenResult<T>` / the crate's own `Result` type. No panics on
  reachable error paths outside `#[cfg(test)]`.
- `dengjen_core::DengjenError` is the shared error enum; FFI crates (`libdengjen`, `dengjen-grpc`,
  `dengjen-python`) convert it at the boundary (see `DengjenFFIError` in
  `crates/frontends/capi/src/lib.rs` for the pattern).

## `unsafe` policy

- `#![forbid(unsafe_code)]` is set on every crate that doesn't need FFI: `dengjen-core`,
  `audio-ops`, `dengjen-piper`, `dengjen-cli`, `dengjen-grpc`, `dengjen-python`. Don't add
  `unsafe` to these — if a dependency forces it (see `dengjen-kokoro` below), forbid can't be
  used and that has to be a deliberate, documented exception, not a silent drop.
- `dengjen-kokoro` cannot use `forbid(unsafe_code)`: `ndarray::s!` (used in `voice_style.rs`)
  expands to code containing `#[allow(unsafe_code)]`, which conflicts with `forbid` even though
  the crate's own source has zero `unsafe`. This is a rustc/ndarray interaction, not a gap in the
  crate — don't "fix" it by adding real unsafe code there.
- The genuinely FFI-heavy crates (`libdengjen`, `sonic-sys`, `espeak-phonemizer`, and
  `dengjen-synth`'s one call into `sonic-sys`) use `unsafe`, and it's enforced narrowly:
  - `unsafe_op_in_unsafe_fn = "deny"` (workspace-wide) — inside an `unsafe fn`, each raw
    operation still needs its own `unsafe { }` block. Don't wrap an entire function body in one
    blanket block just to satisfy this (`cargo clippy --fix` will do exactly that — rewrite the
    fix by hand to scope the block to the actual unsafe operation).
  - `clippy::missing_safety_doc = "deny"` — every `unsafe fn` needs a `# Safety` doc comment
    stating the caller's obligation.
  - `clippy::undocumented_unsafe_blocks = "deny"` — every `unsafe { }` needs a `// SAFETY:`
    comment immediately above it explaining why the operation is sound *at this call site*.

## Lints enabled vs. deferred

`[workspace.lints]` in the root `Cargo.toml` applies to every crate via `[lints] workspace = true`
in its own `Cargo.toml`. CI runs `cargo clippy --workspace --lib --bins -- -D warnings`, which
escalates every enabled lint to a hard failure — so anything added there must already be clean
across the whole workspace before enabling it.

Deliberately **not** enabled yet (verified counts as of 2026-08-16, `cargo clippy -- -W <lint>`):
`clippy::unwrap_used` (~60 sites), `clippy::expect_used` (~5), `clippy::indexing_slicing` (~27,
mostly audio-sample math), `clippy::multiple_unsafe_ops_per_block` (2), full `clippy::pedantic`
(310). These need a real audit, not a blind mechanical fix — changing `.unwrap()` to `Result`
propagation or `[i]` indexing to `.get()` can silently change behavior (panic → wrong output)
if done without understanding each call site. Fold them in incrementally when touching that code
for a real reason, not as a bulk sweep.

## CI gates (`.github/workflows/rust-lint.yml`)

- `clippy` — `cargo clippy --workspace --lib --bins -- -D warnings`.
- `fmt` — `cargo fmt --all -- --check`.
- `audit` — `rustsec/audit-check` (known-CVE advisories).
- `deny` — `cargo deny check licenses bans sources` (`deny.toml`: license allowlist covers the
  actual dependency tree as of 2026-08-16 plus this workspace's own `GPL-3.0-or-later`; a new
  dependency with an unlisted license fails the build on purpose — add it to `deny.toml`'s
  `allow` list only after checking it's actually acceptable, don't rubber-stamp).
- `asan` — AddressSanitizer over `libdengjen`, `dengjen-synth`, and `espeak-phonemizer` (nightly
  toolchain, `-Z sanitizer=address -Z build-std`). **Miri cannot be used here**: every `unsafe`
  block in this workspace calls into a real linked C library (onnxruntime, libsonic, espeak-ng),
  and Miri doesn't support arbitrary FFI. ASan does, and is validated to run clean against real
  memory (it caught and this session fixed a test-fixture leak in `libdengjen`'s unit tests —
  `ExternError`'s message is intentionally not freed by `Drop`, per `ffi_support`'s contract;
  tests must call `out_error.manually_release()` after asserting on it).

- `fuzz-build` — build-only smoke check for `crates/dengjen/models/piper/fuzz` (`cargo-fuzz`
  target `map_phonemes_to_ids`), so the harness doesn't bit-rot. It targets
  `dengjen_piper::map_phonemes_to_ids` — the one function that parses untrusted, on-disk voice
  config data (`phoneme_id_map`) by hand — with `default-features = false` on the `dengjen-piper`
  dependency to sidestep the `libtashkeel_core` workspace patch (this fuzz crate has its own
  `[workspace]` table, so it doesn't inherit the root's `[patch.crates-io]`) and the
  espeak-ng/cmake build dependency, neither of which the target needs. Actual fuzzing runs are
  manual (`cargo +nightly fuzz run map_phonemes_to_ids` from that directory) — this already found
  and fixed one real panic (a `phoneme_id_map` entry with an empty id list crashed
  `entry.first().unwrap()`; see the regression test
  `map_phonemes_to_ids_skips_an_entry_with_an_empty_id_list_instead_of_panicking`).

## Reviewing changes

Run `/code-review` before merging non-trivial changes — treat a clean report as the merge gate,
the same way a green CI check is.
