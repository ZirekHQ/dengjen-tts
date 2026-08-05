# Clear RustSec Advisories (Issue #12) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Tasks are sequential and dependent (each bump changes what `cargo update` resolves next) — do not parallelize across subagents.

**Goal:** Clear all 13 vulnerability advisories and 6 warning-level (unmaintained/unsound/yanked) findings that `cargo audit` reports against this workspace's `Cargo.lock`, without breaking the build, tests, or clippy gate.

**Architecture:** No application architecture changes. This is a dependency-version remediation: bump a handful of leaf/near-leaf `Cargo.toml` version requirements, let `cargo update` re-resolve the graph, and fix whatever call-site breakage the resulting major bumps introduce (mainly `ort` 2.0.0-rc.9→rc.13 and `pyo3` 0.20→latest).

**Tech Stack:** Rust workspace (resolver 2), `cargo-audit` 0.22.2, `cargo clippy`.

## Global Constraints

- Every advisory in the issue must end with a fixed version, a documented `cargo audit` ignore (with reasoning), or an explicit "not fixable without X" note — no silently dropped items.
- `cargo clippy --workspace --exclude istft-sys --lib --bins -- -D warnings` (the CI `clippy` job) must stay green throughout.
- Don't change the `default` feature set behavior visible to consumers (e.g. don't force `ort-dylib` on) — only touch dependency *versions*, not feature wiring, except where a version bump requires it.
- `README.md`'s testing caveat applies: `cargo test` from the workspace root can spuriously fail for packages with sub-package `.cargo/config`; if root-level `cargo test --workspace` fails oddly, retest the affected package with `cd` into it per the README before treating it as a real regression.
- Toolchain: `source "$HOME/.cargo/env"` and prepend `$HOME/.cargo/bin` to `PATH` before every `cargo`/`rustc` invocation in this shell (fresh installs aren't in the default `PATH`). Always call the real binary with a leading `\cargo` — the `rtk` hook intercepts plain `cargo clippy` invocations and returns a canned summary instead of real output.

## Findings (baseline, from `cargo audit --json` — see research above)

| Advisory | Package | Current | Patched | Root cause / fix path |
|---|---|---|---|---|
| RUSTSEC-2026-0007 | bytes 1.8.0 | tonic stack | `>=1.11.1` | semver-compatible `cargo update -p bytes` |
| RUSTSEC-2026-0204 | crossbeam-epoch 0.9.18 | `rayon` (dengjen-synth, libtashkeel_core) | `>=0.9.20` | semver-compatible `cargo update -p crossbeam-epoch` |
| RUSTSEC-2024-0421 | idna 0.5.0 | `url` ← `ureq` ← `ort-sys` (build-dep) | `>=1.0.0` | newer `url` 2.5.x internally requires idna `^1.1.0`; resolves via `cargo update -p url` (rides along with the `ort` bump below) |
| RUSTSEC-2026-0177 | pyo3 0.20.3 | dengjen-python | `>=0.29.0` | major bump to pyo3 0.29.x, migrate `Bound<>` API |
| RUSTSEC-2025-0020 | pyo3 0.20.3 | dengjen-python | `>=0.24.1` | covered by the same pyo3 bump |
| RUSTSEC-2025-0009 | ring 0.17.8 | `ureq` ← `ort-sys` (build-dep) | `>=0.17.12` | semver-compatible once resolver re-runs |
| RUSTSEC-2024-0336 | rustls 0.22.3 | `ureq` ← `ort-sys` (build-dep) | `>=0.23.5` (or 0.22.4 patch) | `ort-sys` rc.13 requires `ureq ^3`, which requires `rustls ^0.23.22` — clears via the `ort` bump |
| RUSTSEC-2026-0098/0049/0099/0104 | rustls-webpki 0.102.2 | same chain | `>=0.103.x` | rustls 0.22.x is stuck on webpki `^0.102.1` forever; rustls 0.23.x requires webpki `^0.103.5` — only fixable by the `ort`→ureq3→rustls0.23 chain |
| RUSTSEC-2026-0067/0068 | tar 0.4.40 | `ort-sys` build-dep (extracts downloaded ONNX Runtime archive) | `>=0.4.45` | `ort-sys` rc.13 drops `tar` entirely (switched to `lzma-rust2`) — clears via the `ort` bump |

| Warning | Package | Root cause | Fix path |
|---|---|---|---|
| RUSTSEC-2025-0056 (unmaintained) | adler 1.0.2 | `miniz_oxide` ← `flate2` ← `ort-sys` build-dep | `ort-sys` rc.13 drops `flate2` entirely — clears via the `ort` bump |
| RUSTSEC-2024-0375 + RUSTSEC-2021-0145 (unmaintained + unsound) | atty 0.2.14 | `clap` v3 ← `cbindgen` 0.26.0 ← `libdengjen` (capi) build-dep | `cbindgen` 0.29.4 requires `clap ^4.3` (no atty) — bump `cbindgen` |
| RUSTSEC-2026-0190 (unsound) | anyhow 1.0.82 | transitive, unclear exact parent | patched `>=1.0.103`, well within existing `^1` reqs — `cargo update -p anyhow` |
| RUSTSEC-2026-0097 (unsound) | rand 0.8.5 | transitive | patched `>=0.8.6` within `^0.8` — `cargo update -p rand` |
| RUSTSEC-2025-0023 (unsound) | tokio 1.41.1 | dengjen-grpc direct dep | patched `>=1.44.2` within `^1.41.1` — `cargo update -p tokio` |
| yanked | futures-util 0.3.30, spin 0.9.8 | transitive | `cargo update` picks a non-yanked patch automatically |

**Net effect:** almost none of this actually requires the `tonic` bump the issue speculated about — `bytes` clears via a plain patch update. The real major-version work is exactly two crates: `ort` (2.0.0-rc.9 → 2.0.0-rc.13) and `pyo3` (0.20.3 → latest 0.29.x). `tonic` can stay at 0.12.3.

## File Map

- `Cargo.lock` — regenerated by `cargo update` calls throughout.
- `crates/dengjen/models/piper/Cargo.toml`, `crates/frontends/{python,cli,capi,grpc}/Cargo.toml`, `deps/libtashkeel/crates/core/Cargo.toml` — bump `[dependencies.ort]` version.
- `crates/dengjen/models/piper/src/lib.rs`, `deps/libtashkeel/crates/core/src/backend/ort.rs`, `crates/frontends/{cli/src/main.rs,capi/src/lib.rs,grpc/src/main.rs}` — fix any `ort` API breakage from the rc.9→rc.13 bump.
- `crates/frontends/capi/Cargo.toml` — bump `cbindgen` version.
- `crates/frontends/python/Cargo.toml` — bump `pyo3` version, adjust `abi3-py3X` feature floor if pyo3 requires it.
- `crates/frontends/python/src/lib.rs` — migrate pyo3 API surface to whatever the target version needs (`Bound<'py, T>` module/GIL API).
- `.github/workflows/rust-lint.yml` — once advisories clear, drop `continue-on-error: true` from the `audit` job (or turn any accepted-risk leftovers into explicit `cargo audit` ignores) so the gate actually blocks regressions going forward.

---

### Task 1: Cheap semver-compatible updates

**Files:** `Cargo.lock` only.

- [ ] **Step 1:** `\cargo update -p bytes -p crossbeam-epoch -p ring -p anyhow -p rand -p tokio -p futures-util -p spin`
- [ ] **Step 2:** Re-run `\cargo audit --json` and confirm these advisories are gone from the vulnerabilities/warnings list: RUSTSEC-2026-0007 (bytes), RUSTSEC-2026-0204 (crossbeam-epoch), RUSTSEC-2025-0009 (ring), RUSTSEC-2026-0190 (anyhow), RUSTSEC-2026-0097 (rand), RUSTSEC-2025-0023 (tokio), the two `yanked` warnings.
- [ ] **Step 3:** `\cargo build --workspace --exclude istft-sys` to confirm nothing broke from the patch bumps (should be a no-op risk, but verify).
- [ ] **Step 4:** Commit: `git add Cargo.lock && git commit -m "chore: bump semver-compatible deps to clear RustSec advisories"`.

### Task 2: Bump `ort` 2.0.0-rc.9 → 2.0.0-rc.13

**Files:**
- Modify: `crates/dengjen/models/piper/Cargo.toml:23` (`[dependencies.ort] version = "2.0.0-rc.9"` → `"2.0.0-rc.13"`)
- Modify: `crates/frontends/python/Cargo.toml:32`, `crates/frontends/cli/Cargo.toml:27`, `crates/frontends/capi/Cargo.toml:26`, `crates/frontends/grpc/Cargo.toml:23` (same edit)
- Modify: `deps/libtashkeel/crates/core/Cargo.toml:27` (same edit)
- Possible fix: `crates/dengjen/models/piper/src/lib.rs`, `deps/libtashkeel/crates/core/src/backend/ort.rs`, `crates/frontends/cli/src/main.rs`, `crates/frontends/capi/src/lib.rs`, `crates/frontends/grpc/src/main.rs`

**Interfaces:** No change to any of this workspace's own crate APIs — `ort` is a leaf ML-inference dependency. Only `ort::` call sites (`Session`, `ort::inputs!`, `execution_providers::*`, `ort::init()`) are at risk of signature drift between rc.9 and rc.13.

- [ ] **Step 1:** Edit all six `Cargo.toml` files above, changing `version = "2.0.0-rc.9"` to `version = "2.0.0-rc.13"`.
- [ ] **Step 2:** `\cargo update -p ort -p ort-sys`, then `\cargo update -p url` (pulls idna ^1.1.0 transitively — confirm with `grep -A1 '^name = "idna"' Cargo.lock`).
- [ ] **Step 3:** `\cargo build --workspace --exclude istft-sys 2>&1 | tee /tmp/.../ort_build.log`. Fix any compile errors in the files listed above — likely candidates: `Session::builder()` chain signature changes, `ort::inputs!` macro output type changes, execution-provider `.build()` trait bound changes. Iterate until it compiles clean.
- [ ] **Step 4:** `\cargo audit --json` and confirm gone: RUSTSEC-2024-0421 (idna), RUSTSEC-2025-0009 (ring, if not already cleared in Task 1), RUSTSEC-2024-0336 (rustls), RUSTSEC-2026-0098/0049/0099/0104 (rustls-webpki), RUSTSEC-2026-0067/0068 (tar), RUSTSEC-2025-0056 (adler).
- [ ] **Step 5:** Run whatever test coverage exists for the piper/synth path (`cd crates/dengjen/synth && \cargo test`, `cd crates/dengjen/models/piper && \cargo test` — per the README's per-package testing caveat) and, if model fixtures are available, a smoke synthesis run via the CLI frontend to confirm inference still produces valid audio (not just "compiles").
- [ ] **Step 6:** Commit: `git commit -am "chore: bump ort to 2.0.0-rc.13, clearing rustls/webpki/ring/tar/idna/adler advisories"`.

### Task 3: Bump `cbindgen` to clear `atty`

**Files:** Modify `crates/frontends/capi/Cargo.toml:41` (`cbindgen = "0.26.0"` → `"0.29.4"`)

- [ ] **Step 1:** Edit the version, then `\cargo update -p cbindgen`.
- [ ] **Step 2:** `\cargo build -p libdengjen` (capi crate — this runs cbindgen's build script to regenerate the C header). Diff the generated header (likely under `crates/frontends/capi/` — check `build.rs` for the output path) against what's committed, if headers are checked in; adjust if cbindgen 0.29's output format differs.
- [ ] **Step 3:** `\cargo audit --json` and confirm gone: RUSTSEC-2024-0375 and RUSTSEC-2021-0145 (atty, both).
- [ ] **Step 4:** Commit: `git commit -am "chore: bump cbindgen to 0.29.4, dropping unmaintained atty dependency"`.

### Task 4: Bump `pyo3` 0.20.3 → latest (0.29.x)

**Files:**
- Modify: `crates/frontends/python/Cargo.toml:32-35` (`pyo3` version + `abi3-py37` feature)
- Modify: `crates/frontends/python/src/lib.rs` (full pyo3 API migration — module init signature, `PyBytes` construction)

**Interfaces:** Purely internal to the `dengjen-python` crate; the Python-facing API (class/method names in `#[pymethods]`) must not change behavior, only the Rust-side pyo3 plumbing.

- [ ] **Step 1:** Edit `crates/frontends/python/Cargo.toml`: bump `pyo3` to the latest 0.29.x, and check whether `abi3-py37` still exists as a feature name at that version (pyo3 dropped Python 3.7 abi3 support along the way) — if not, bump to the lowest still-supported `abi3-py3X` (check `\cargo info pyo3` feature list) and note the floor bump in the commit message.
- [ ] **Step 2:** `\cargo update -p pyo3 -p pyo3-macros -p pyo3-ffi -p pyo3-build-config`.
- [ ] **Step 3:** `\cargo build -p dengjen-python 2>&1 | tee /tmp/.../pyo3_build.log`. Fix compile errors in `lib.rs`, expected:
  - `#[pymodule] fn pydengjen(_py: Python, m: &PyModule)` → `fn pydengjen(m: &Bound<'_, PyModule>) -> PyResult<()>` (obtain `py` via `m.py()` where still needed for `m.add(...)`).
  - `PyBytes::new(py, &bytes_vec).into()` → returns `Bound<'py, PyBytes>` now; adjust the `.into()` conversion to `PyObject` accordingly (likely `.into_any().unbind()` or `.into()` still works once the surrounding fn signature is updated — verify against the compiler).
  - `wrap_pyfunction!(phonemize_text, m)` — confirm macro still accepts `&Bound<PyModule>` at this pyo3 version; adjust per compiler guidance if not.
  - Everything else (`PyRef`, `Python::with_gil` equivalents, `py.allow_threads`, `PyErr::restore`, `#[pyclass]`/`#[pymethods]`/`#[new]`/`#[getter]`/`#[setter]`/`#[staticmethod]` attributes) should be source-compatible — only fix if the compiler flags it.
- [ ] **Step 4:** `\cargo test -p dengjen-python` (the `should_diacritize_*` unit tests don't touch pyo3 directly, but confirm the crate still builds+tests under both `tashkeel` and `--no-default-features`).
- [ ] **Step 5:** If a Python interpreter is available on this machine, do a real import smoke test: `cd crates/frontends/python && maturin develop` (or equivalent) then `python3 -c "import pydengjen"`. If no Python toolchain is available, say so explicitly rather than claiming this was verified — a clean `cargo build` proves the Rust side compiles, not that the compiled extension module loads correctly under CPython's ABI.
- [ ] **Step 6:** `\cargo audit --json` and confirm gone: RUSTSEC-2026-0177 and RUSTSEC-2025-0020 (pyo3, both).
- [ ] **Step 7:** Commit: `git commit -am "chore: bump pyo3 to 0.29.x, migrating to the Bound<> API DEV-... / NOJIRA"` (adjust message per this repo's actual convention — this repo is not a Collibra repo, so use a plain Conventional-ish message matching this repo's existing git log style, not the Collibra JIRA convention).

### Task 5: Full verification, cleanup, and re-enable the audit gate

**Files:** `.github/workflows/rust-lint.yml`

- [ ] **Step 1:** `\cargo audit --json` one more time from clean; confirm the vulnerabilities list and warnings list are both empty (or contain only advisories with no available fix, each of which must be individually justified — expected: none remain per the findings table above).
- [ ] **Step 2:** `\cargo clippy --workspace --exclude istft-sys --lib --bins -- -D warnings` — must be clean (matches the CI `clippy` job exactly).
- [ ] **Step 3:** `\cargo build --release --workspace --exclude istft-sys` — full release build, matching what a real consumer would run per the README.
- [ ] **Step 4:** Run the full per-package test sweep per the README's testing note (root `cargo test --workspace` first; if it misbehaves, `cd` into each package with tests and rerun there).
- [ ] **Step 5:** Edit `.github/workflows/rust-lint.yml`: remove `continue-on-error: true` from the `audit` job and delete/rewrite the stale comment above it (currently says advisories "would need major version bumps to clear" — no longer true). If any advisory genuinely couldn't be cleared, keep `continue-on-error: true` and update the comment to name exactly which advisory and why, instead of removing it.
- [ ] **Step 6:** `git commit -am "ci: stop tolerating cargo-audit failures now that advisories are cleared"`.
- [ ] **Step 7:** Close out: post a summary against issue #12 (or leave for the user to close) listing which advisories were cleared by which mechanism, referencing this plan.

---

## Self-Review

- **Spec coverage:** All 13 vulnerabilities + 6 warnings from the issue are each mapped to a task above; the CI comment/gate update (not mentioned in the issue text but implied by "should be triaged and cleared") is covered in Task 5.
- **Placeholder scan:** Ort/pyo3 exact compile fixes are described as "fix per compiler output" rather than fabricated diffs, because the actual breakage can't be known until compiled on this machine — this is a real constraint of a live dependency-version bump, not a skipped placeholder; the task still names the exact files, exact symbols, and exact expected failure modes to fix.
- **Type consistency:** N/A — no new types introduced across tasks.
