//! Embeds the built phone app (`apps/phone/dist`) into the executable, so a release is a single file.
//! Without a build (e.g. `cargo test` on a fresh checkout) nothing is embedded and the server falls back to a
//! folder or to a "not built" page. `WIIU_CONNECT_WEB_DIST` overrides the folder.

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

    let mut files = Vec::new();
    if dist.join("index.html").is_file() {
        collect(&dist, &dist, &mut files);
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
}
