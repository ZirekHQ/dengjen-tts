use std::env;
use std::path::PathBuf;

fn main() {
    // Establish the crate root directory from cargo environment
    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    // Navigate to workspace root (three levels up from crate root)
    let root = crate_dir
        .ancestors()
        .nth(3)
        .expect("unable to find workspace root");

    // Paths for the espeak-ng data copy operation
    let src = root.join("deps").join("dev").join("espeak-ng-data");
    let dst = crate_dir.join("python").join("pydengjen").join("espeak-ng-data");

    // Avoid re-copying data if it already exists
    if dst.exists() {
        return;
    }

    // Copy data directory to Python package
    let copy_config = fs_extra::dir::CopyOptions::new();
    fs_extra::dir::copy(&src, dst.parent().unwrap(), &copy_config).unwrap();
}
