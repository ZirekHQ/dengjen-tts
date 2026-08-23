//! Compiles the vendored espeak-ng C sources (`vendor/espeak-ng`) with CMake
//! and statically links the result into this crate.
//!
//! espeak-ng's own `CMakeLists.txt` exposes optional backends this crate has
//! no use for — async playback, MBROLA, libsonic, libpcaudio, Klatt and
//! SpeechPlayer — so each is switched off via `configure_arg` to keep the
//! static build to what's actually linked.
fn main() {
    println!("cargo:rerun-if-changed=vendor/espeak-ng/src");
    println!("cargo:rustc-link-lib=static=espeak-ng");
    println!("cargo:rustc-link-lib=static=ucd");

    let build_dir = cmake::Config::new("vendor/espeak-ng")
        .configure_arg("-DUSE_ASYNC:BOOL=OFF")
        .configure_arg("-DUSE_MBROLA:BOOL=OFF")
        .configure_arg("-DUSE_LIBSONIC:BOOL=OFF")
        .configure_arg("-DUSE_LIBPCAUDIO:BOOL=OFF")
        // Klatt is an alternative formant synthesizer this crate never selects.
        .configure_arg("-DUSE_KLATT:BOOL=OFF")
        // SpeechPlayer is another optional synthesis backend this crate never selects.
        .configure_arg("-DUSE_SPEECHPLAYER:BOOL=OFF")
        // A shared build would need its own install/rpath handling at link time;
        // the two `rustc-link-lib=static=...` directives above expect archives.
        .configure_arg("-DBUILD_SHARED_LIBS:BOOL=OFF")
        // Both default ON upstream. EXE builds the espeak-ng CLI binary this
        // crate never uses; TESTS builds espeak-ng's own test suite. Together
        // they're what pull in cmake/data.cmake (dictionary compilation from
        // dictsource/phsource) and tests/ — none of which are vendored here,
        // since this crate reads phoneme data from a runtime-configured
        // directory (see resolve_data_directory in src/lib.rs), never from
        // this build. See vendor/espeak-ng/VENDOR_README.md.
        .configure_arg("-DBUILD_ESPEAK_NG_EXE:BOOL=OFF")
        .configure_arg("-DBUILD_ESPEAK_NG_TESTS:BOOL=OFF")
        .build();

    // `cmake::Config::build` returns the install prefix it built into; the
    // static archives land under its `lib` subdirectory.
    println!(
        "cargo:rustc-link-search={}",
        build_dir.join("lib").display()
    );
}
