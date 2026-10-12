//! Self-signed HTTPS for phones.
//!
//! Browsers only expose the camera (`getUserMedia`) and, on iOS, motion sensors to secure contexts. Phones reach
//! the PC by LAN IP, which no public CA can certify, so Wii U Connect creates a self-signed certificate once and
//! reuses it. Each phone accepts the browser warning once per certificate.

use anyhow::Context;
use std::fs;
use std::path::{Path, PathBuf};

pub const CERT_FILE: &str = "cert.pem";
pub const KEY_FILE: &str = "key.pem";
const HOSTS_FILE: &str = "cert-hosts.txt";

#[derive(Debug, Clone)]
pub struct CertPaths {
    pub cert: PathBuf,
    pub key: PathBuf,
}

/// Default directory for runtime data (certificate, later saved device assignments).
pub fn default_data_dir() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata).join("WiiUConnect");
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("wiiu-connect");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".config").join("wiiu-connect");
    }
    PathBuf::from("data")
}

/// Returns a certificate valid for all `hosts`, creating (or re-creating) it in `dir` when needed.
/// An existing certificate is kept if it already covers every host, so phones don't see a new warning.
pub fn load_or_create(dir: &Path, hosts: &[String]) -> anyhow::Result<CertPaths> {
    let paths = CertPaths {
        cert: dir.join(CERT_FILE),
        key: dir.join(KEY_FILE),
    };
    let hosts_path = dir.join(HOSTS_FILE);
    if paths.cert.is_file() && paths.key.is_file() {
        let covered: Vec<String> = fs::read_to_string(&hosts_path)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect();
        if hosts.iter().all(|h| covered.contains(h)) {
            return Ok(paths);
        }
        tracing::info!("network address changed; creating a new HTTPS certificate");
    }
    fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let generated = rcgen::generate_simple_self_signed(hosts.to_vec()).context("generating certificate")?;
    fs::write(&paths.cert, generated.cert.pem())?;
    write_private(&paths.key, generated.signing_key.serialize_pem().as_bytes())?;
    fs::write(&hosts_path, hosts.join("\n"))?;
    Ok(paths)
}

/// Writes a file readable only by the current user where the platform supports it.
fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(data)
    }
    #[cfg(not(unix))]
    {
        // %APPDATA% is already per-user on Windows.
        fs::write(path, data)
    }
}

/// Installs the process-wide rustls crypto provider (ring). Safe to call more than once.
pub fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("wiiu-tls-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn creates_reuses_and_regenerates() {
        let dir = temp_dir("reuse");
        let hosts = vec!["localhost".to_string(), "192.168.1.20".to_string()];
        let p = load_or_create(&dir, &hosts).unwrap();
        let first = fs::read_to_string(&p.cert).unwrap();
        assert!(first.contains("BEGIN CERTIFICATE"));
        assert!(fs::read_to_string(&p.key).unwrap().contains("PRIVATE KEY"));

        // Same (or fewer) hosts: certificate is reused.
        load_or_create(&dir, &hosts[..1]).unwrap();
        assert_eq!(fs::read_to_string(&p.cert).unwrap(), first);

        // New LAN address: regenerated.
        load_or_create(&dir, &["localhost".into(), "10.0.0.5".into()]).unwrap();
        assert_ne!(fs::read_to_string(&p.cert).unwrap(), first);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn private_key_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("perm");
        let p = load_or_create(&dir, &["localhost".into()]).unwrap();
        assert_eq!(fs::metadata(&p.key).unwrap().permissions().mode() & 0o777, 0o600);
        let _ = fs::remove_dir_all(&dir);
    }
}
