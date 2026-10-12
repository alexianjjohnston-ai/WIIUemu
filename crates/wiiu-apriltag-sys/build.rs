use std::path::Path;

fn main() {
    let vendor = Path::new("vendor/apriltag");
    let mut build = cc::Build::new();
    build.include(vendor).warnings(false).file("shim/wiiu_apriltag.c");
    for dir in [vendor.to_path_buf(), vendor.join("common")] {
        for entry in std::fs::read_dir(&dir).expect("vendored AprilTag sources") {
            let path = entry.expect("dir entry").path();
            if path.extension().is_some_and(|e| e == "c") {
                println!("cargo:rerun-if-changed={}", path.display());
                build.file(path);
            }
        }
    }
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" {
        // Mirrors upstream CMakeLists.txt.
        build.define("WIN32_LEAN_AND_MEAN", None);
    }
    if target_env == "msvc" {
        build.define("_CRT_SECURE_NO_WARNINGS", None);
        build.define("_CRT_NONSTDC_NO_DEPRECATE", None);
    }
    build.compile("wiiu_apriltag");
    if target_os != "windows" {
        println!("cargo:rustc-link-lib=m");
        println!("cargo:rustc-link-lib=pthread");
    }
    println!("cargo:rerun-if-changed=shim/wiiu_apriltag.c");
}
