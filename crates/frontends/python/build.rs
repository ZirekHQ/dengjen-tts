use std::env;
use std::path::PathBuf;

fn main() {
    let cargo_manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = cargo_manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let espeak_data_dir = workspace_root
        .join("deps")
        .join("dev")
        .join("espeak-ng-data");
    let target_dir = cargo_manifest_dir
        .join("python")
        .join("pysonata")
        .join("espeak-ng-data");
    if target_dir.exists() {
        return
    } else {
         let options = fs_extra::dir::CopyOptions::new();
        fs_extra::dir::copy(&espeak_data_dir, &target_dir.parent().unwrap(), &options).unwrap();
    }
}
