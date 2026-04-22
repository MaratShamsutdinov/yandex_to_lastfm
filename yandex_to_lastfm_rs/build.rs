use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn copy_asset_to_output(asset_rel_path: &str, out_dir: &Path) {
    println!("cargo:rerun-if-changed={}", asset_rel_path);

    let src = PathBuf::from(asset_rel_path);
    let file_name = src.file_name().expect("asset file name").to_owned();

    let dest_dir = out_dir.join("assets");
    fs::create_dir_all(&dest_dir).expect("create output assets dir");

    let dest = dest_dir.join(file_name);
    fs::copy(&src, &dest).unwrap_or_else(|e| {
        panic!(
            "failed to copy asset '{}' -> '{}': {}",
            src.display(),
            dest.display(),
            e
        )
    });
}

fn main() {
    println!("cargo:rerun-if-changed=assets/app.ico");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let profile_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("failed to resolve target profile dir");

    copy_asset_to_output("assets/status_ok.png", profile_dir);
    copy_asset_to_output("assets/status_error.png", profile_dir);
    copy_asset_to_output("assets/status_extension_missing.png", profile_dir);
    copy_asset_to_output("assets/status_reload.png", profile_dir);

    #[cfg(target_os = "windows")]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/app.ico");
        res.compile().expect("failed to compile Windows resources");
    }
}
