# Rust Code Standards & Architecture
- **Error Handling**:
  - Return explicit `Result<T, E>` / `DengjenResult<T>`[cite: 2]. No panics or `.unwrap()`/`.expect()` on reachable paths outside `#[cfg(test)]`[cite: 1, 2].
  - Use shared domain error enums; convert explicitly at FFI/C-API boundaries[cite: 2].
  - Do NOT perform bulk unwrap/indexing sweeps (`unwrap_used`, `indexing_slicing` are deferred)[cite: 2].
- **Idiomatic Rust**:
  - `impl Trait` > `Box<dyn Trait>` where possible[cite: 1].
  - Avoid unneeded `.clone()` calls or superfluous `#[derive]` attributes[cite: 1].
  - Prefer iterator combinators (`map`, `filter_map`, `fold`) over explicit loops[cite: 1].
- **Unsafe & FFI Policy**:
  - Non-FFI crates must maintain `#![forbid(unsafe_code)]`[cite: 2].
  - Inside `unsafe fn`, scope `unsafe { }` blocks strictly to the minimal raw pointer/FFI operation (no blanket function blocks)[cite: 2].
  - Every `unsafe fn` MUST have a `# Safety` doc comment defining caller obligations[cite: 2].
  - Every `unsafe { }` block MUST have a `// SAFETY:` comment immediately above it explaining soundness[cite: 2].
  - Do not use Miri if linked C libraries/FFI are present; rely on AddressSanitizer (`asan`)[cite: 2].

# Verification Gates (Run Before Finalizing)
- Clippy: `cargo clippy --workspace --lib --bins -- -D warnings`[cite: 2]
- Format: `cargo fmt --all -- --check`[cite: 2]
- Tests: `cargo test --workspace --no-fail-fast -- --skip test_lazy_stream --skip test_parallel_stream --skip test_realtime_stream` (plain `cargo test` stops at the first failing crate and never reaches the rest of the workspace). Needs `espeak-ng` installed or `DENGJEN_ESPEAKNG_DATA_DIRECTORY` pointed at its data dir; the three skipped tests need real model fixtures not present by default — see `.github/workflows/rust-lint.yml`'s `asan` job for the same recipe.
- Dependencies & Licenses: `cargo deny check licenses bans sources`[cite: 2]
- Review: Run `/code-review` before merging non-trivial changes[cite: 2]