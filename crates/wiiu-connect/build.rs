//! Embeds the built phone app (`apps/phone/dist`) into the executable, so a release is a single file.
//! Without a build (e.g. `cargo test` on a fresh checkout) nothing is embedded and the server falls back to a
//! folder or to a "not built" page. `WIIU_CONNECT_WEB_DIST` overrides the folder; `WIIU_CONNECT_REQUIRE_WEB=1`
//! (set by the release workflow) turns a missing phone app into a build error.
//!
//! For Windows targets it also adds the icon, version information and an `asInvoker` manifest to the executable.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn collect(dir: &Path, base: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(&path, base, out);
        } else if path.extension().is_none_or(|e| e != "map") {
            let rel = path
                .strip_prefix(base)
                .expect("under base")
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, path));
        }
    }
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let dist = std::env::var_os("WIIU_CONNECT_WEB_DIST")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("../../apps/phone/dist"));
    println!("cargo:rerun-if-env-changed=WIIU_CONNECT_WEB_DIST");
    // The parent changes when `dist` appears or disappears; each file is watched below.
    println!("cargo:rerun-if-changed={}", manifest.join("../../apps/phone").display());

    println!("cargo:rerun-if-env-changed=WIIU_CONNECT_REQUIRE_WEB");
    let mut files = Vec::new();
    if dist.join("index.html").is_file() {
        collect(&dist, &dist, &mut files);
    } else if std::env::var_os("WIIU_CONNECT_REQUIRE_WEB").is_some_and(|v| v == "1") {
        panic!(
            "WIIU_CONNECT_REQUIRE_WEB=1 but the phone app is not built at {} (run `npm ci && npm run build` in apps/phone)",
            dist.display()
        );
    }
    let mut code = String::from("/// Built phone app files: (path relative to the web root, contents).\n");
    code.push_str("pub static FILES: &[(&str, &[u8])] = &[\n");
    for (rel, path) in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        let abs = path.canonicalize().expect("file exists");
        writeln!(code, "    ({rel:?}, include_bytes!({:?})),", abs.to_string_lossy()).unwrap();
    }
    code.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("set by cargo")).join("embedded_web.rs");
    std::fs::write(out, code).expect("write embedded_web.rs");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows_resources(&manifest);
    }
}

/// Icon, file/product version and manifest shown in Explorer's Properties and used by Windows.
fn windows_resources(manifest: &Path) {
    let assets = manifest.join("assets");
    println!("cargo:rerun-if-changed={}", assets.display());
    let version = std::env::var("CARGO_PKG_VERSION").expect("set by cargo");
    let mut res = winresource::WindowsResource::new();
    res.set_icon(&assets.join("wiiu-connect.ico").to_string_lossy())
        .set_manifest_file(&assets.join("wiiu-connect.manifest").to_string_lossy())
        .set("ProductName", "Wii U Connect")
        .set("FileDescription", "Wii U Connect: phones as Wii U controllers for Cemu")
        .set("CompanyName", "Wii U Connect contributors")
        .set(
            "LegalCopyright",
            "Wii U Connect contributors. Not affiliated with Nintendo.",
        )
        .set("OriginalFilename", "wiiu-connect.exe")
        .set("ProductVersion", &version)
        .set("FileVersion", &version);
    if let Err(e) = res.compile() {
        panic!("could not add Windows resources (icon, version info): {e}");
    }
}
