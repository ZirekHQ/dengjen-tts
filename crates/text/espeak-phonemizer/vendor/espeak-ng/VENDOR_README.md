# Vendored espeak-ng source

Vendored from [rhasspy/espeak-ng](https://github.com/rhasspy/espeak-ng) at
commit
[`8593723f`](https://github.com/rhasspy/espeak-ng/commit/8593723f) — the
exact commit this project's `deps/espeak-ng` submodule was already pinned
to (`2023.9.7-4-13-g8593723f`), 13 commits past the last tag (`2023.9.7-4`).

Pinned to this commit rather than the older tag because those 13 commits
carry real portability fixes this project's build depends on: iOS support,
cross-compiling fixes, two separate Windows build fixes, and shared-library
support for `ucd`. Reverting to the tag would risk breaking exactly the
platforms this project's CI builds for.

## Trim boundary

`src/`, `cmake/`, `vim/`, `CMakeLists.txt`, the four `COPYING*` license
files, `espeak-ng.pc.in`, `README.md`, and `ChangeLog.md` are vendored —
not the full ~59MB checkout. Everything else (`dictsource/`, `phsource/`,
`espeak-ng-data/`, `tests/`, `docs/`, `android/`, `emscripten/`,
`fastlane/`, `m4/`, `data/`, `tools/`, `_layouts/`) is either
autotools-only, documentation, mobile-app wrapper code, or dictionary
source material this crate's build never touches — this crate reads
phoneme data from a runtime-configured directory
(`DENGJEN_ESPEAKNG_DATA_DIRECTORY`, see `src/lib.rs`), never from anything
this build produces.

This is not a guess: `build.rs` explicitly sets `BUILD_ESPEAK_NG_EXE=OFF`
and `BUILD_ESPEAK_NG_TESTS=OFF` (both default `ON` upstream, and were
previously left at their defaults) — those two options are what gate
`cmake/data.cmake` (dictionary compilation, needs `dictsource`/`phsource`)
and `add_subdirectory(tests)`. A from-scratch `cmake -B build && cmake
--build build` against just `src/`+`cmake/`+`CMakeLists.txt`+licenses+
`espeak-ng.pc.in` configured and built both `libespeak-ng.a` and
`libucd.a` cleanly — but `cargo`'s own `cmake` crate invokes the
**`install`** target, not the default `all` target, and the top-level
`CMakeLists.txt` has an *unconditional* `install(DIRECTORY vim/ftdetect
vim/syntax ...)` rule (installing vim syntax-highlighting files, unrelated
to this crate's needs) that only `cmake --build --target install`
reaches. That's why `vim/` (32KB) is vendored despite not being part of
the `all` target — verified by actually running `cargo build`, not just
`cmake --build`, after adding it.

## To update

Pull the desired revision's `src/`, `cmake/`, `vim/`, `CMakeLists.txt`,
license files, `espeak-ng.pc.in`, `README.md`, `ChangeLog.md` from
[rhasspy/espeak-ng](https://github.com/rhasspy/espeak-ng), replace the
files here, and update this file's pinned commit. Re-verify the trim
boundary still holds by running `cargo build -p espeak-phonemizer` itself
(not just a standalone `cmake --build`) — the `install` target's file
requirements can differ from the default build target's, and can change
between upstream revisions.
