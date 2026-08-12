use std::env;
use std::path::PathBuf;

const SONIC_SOURCE: &str = "../../../deps/sonic/sonic.c";
const SONIC_HEADER: &str = "../../../deps/sonic/sonic.h";

fn main() {
    println!("cargo:rerun-if-changed={SONIC_SOURCE}");
    println!("cargo:rerun-if-changed={SONIC_HEADER}");
    println!("cargo:rustc-link-lib=static=libsonic");

    cc::Build::new()
        .file(SONIC_SOURCE)
        .include(SONIC_HEADER)
        .compile("libsonic");

    let bindings = bindgen::Builder::default()
        .header(SONIC_HEADER)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
