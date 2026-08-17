use std::env;

fn main() {
    println!("cargo:rerun-if-changed=./src/lib.rs");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    let bindings = cbindgen::Builder::new()
        .with_crate(manifest_dir)
        .with_include_version(true)
        .with_documentation(false)
        .with_parse_deps(true)
        .with_parse_include(&["ffi-support"])
        .with_cpp_compat(false)
        .with_language(cbindgen::Language::C)
        .generate()
        .expect("Unable to generate bindings");

    bindings.write_to_file("libdengjen.h");
}
