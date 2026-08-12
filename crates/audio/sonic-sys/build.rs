use std::env;
use std::path::PathBuf;

const SONIC_SOURCE: &str = "../../../deps/sonic/sonic.c";
const SONIC_HEADER: &str = "../../../deps/sonic/sonic.h";

fn compile_sonic_static_lib() {
    let mut build = cc::Build::new();
    build.file(SONIC_SOURCE);
    build.include(SONIC_HEADER);
    build.compile("libsonic");
}

fn generate_bindings() -> bindgen::Bindings {
    let builder = bindgen::Builder::default()
        .header(SONIC_HEADER)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks));

    builder.generate().expect("Unable to generate bindings")
}

fn main() {
    println!("cargo:rustc-link-lib=static=libsonic");
    println!("cargo:rerun-if-changed={SONIC_SOURCE}");
    println!("cargo:rerun-if-changed={SONIC_HEADER}");

    compile_sonic_static_lib();

    let bindings = generate_bindings();
    let out_dir = env::var("OUT_DIR").unwrap();
    let bindings_path = PathBuf::from(out_dir).join("bindings.rs");

    bindings
        .write_to_file(bindings_path)
        .expect("Couldn't write bindings!");
}
