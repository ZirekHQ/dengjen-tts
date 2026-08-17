use std::env;
use std::path::PathBuf;

fn main() {
    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    let root = crate_dir
        .ancestors()
        .nth(3)
        .expect("unable to find workspace root");

    let src = root.join("deps").join("dev").join("espeak-ng-data");
    let dst = crate_dir
        .join("python")
        .join("pydengjen")
        .join("espeak-ng-data");

    if dst.exists() {
        return;
    }

    let copy_config = fs_extra::dir::CopyOptions::new();
    fs_extra::dir::copy(&src, dst.parent().unwrap(), &copy_config).unwrap();
}
