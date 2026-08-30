use cbindgen::{Config, ExportConfig};
use std::env;

const HEADER_OUTPUT: &str = "libdengjen.h";

fn main() {
    // Regenerate the C header any time the exported FFI surface changes.
    println!("cargo:rerun-if-changed=./src/lib.rs");

    let crate_root = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set");

    // SynthesisParams.callback is Option<extern "C" fn(...)>, which cbindgen renders as a bare
    // nullable function pointer rather than naming the SpeechSynthesisCallback typedef -- force
    // the typedef to stay exported anyway, since bindings/go/speak.go casts to it by name.
    let config = Config {
        export: ExportConfig {
            include: vec!["SpeechSynthesisCallback".to_string()],
            ..Default::default()
        },
        ..Default::default()
    };

    let header = cbindgen::Builder::new()
        .with_crate(crate_root)
        .with_config(config)
        .with_include_version(true)
        .with_documentation(false)
        .with_parse_deps(true)
        .with_parse_include(&["ffi-support"])
        .with_cpp_compat(false)
        .with_language(cbindgen::Language::C)
        .generate()
        .expect("cbindgen failed to generate C bindings");

    header.write_to_file(HEADER_OUTPUT);
}
