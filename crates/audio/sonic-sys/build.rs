use std::env;
use std::path::PathBuf;

const SONIC_C_SOURCE: &str = "vendor/sonic/sonic.c";
const SONIC_C_HEADER: &str = "vendor/sonic/sonic.h";
const SONIC_INCLUDE_DIR: &str = "vendor/sonic";

fn build_static_lib(source: &str, include_dir: &str) {
    cc::Build::new()
        .file(source)
        .include(include_dir)
        .compile("libsonic");
}

fn write_bindings(header: &str, out_dir: &str) {
    let bindings = bindgen::Builder::default()
        .header(header)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(out_dir).join("bindings.rs");
    bindings
        .write_to_file(out_path)
        .expect("Couldn't write bindings!");
}

fn main() {
    println!("cargo:rustc-link-lib=static=libsonic");
    println!("cargo:rerun-if-changed={SONIC_C_SOURCE}");
    println!("cargo:rerun-if-changed={SONIC_C_HEADER}");

    build_static_lib(SONIC_C_SOURCE, SONIC_INCLUDE_DIR);

    let out_dir = env::var("OUT_DIR").unwrap();
    write_bindings(SONIC_C_HEADER, &out_dir);
}
