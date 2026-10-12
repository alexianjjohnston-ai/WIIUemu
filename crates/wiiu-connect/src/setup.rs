//! First-run setup from the PC's browser (`/setup`): get Cemu, point its Wii Remotes at Wii U Connect, add the
//! user's own game folders, and start games.
//!
//! Cemu is downloaded only from its official GitHub releases (cemu-project/Cemu, MPL-2.0) into a portable
//! folder that Wii U Connect manages. **No games, keys or Nintendo system files are ever downloaded**: games
//! come from folders the user picks (their own dumps).

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use wiiu_cemu::install::{self, CemuInstall, GameEntry};
use wiiu_cemu::Platform;

const RELEASES_API: &str = "https://api.github.com/repos/cemu-project/Cemu/releases/latest";
const RELEASE_URL_PREFIX: &str = "https://github.com/cemu-project/Cemu/releases/download/";
pub const RELEASES_PAGE: &str = "https://github.com/cemu-project/Cemu/releases/latest";
const MAX_DOWNLOAD_BYTES: u64 = 400 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct SetupConfig {
    /// Wii U Connect's data folder; the managed Cemu goes to `<data_dir>/Cemu`.
    pub data_dir: PathBuf,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Saved {
    cemu_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DownloadState {
    Idle,
    Running {
        phase: String,
        received: u64,
        total: Option<u64>,
    },
    Done {
        version: String,
    },
    Failed {
        error: String,
    },
}

pub struct Setup {
    cfg: SetupConfig,
    platform: Platform,
    saved: Mutex<Saved>,
    download: Mutex<DownloadState>,
}

#[derive(Debug, Serialize)]
pub struct CemuStatus {
    pub dir: PathBuf,
    pub executable: PathBuf,
    pub config_dir: PathBuf,
    pub portable: bool,
    /// Installed by Wii U Connect (in its data folder).
    pub managed: bool,
    pub version: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SetupStatus {
    pub platform: &'static str,
    /// Whether Wii U Connect can download Cemu on this OS (otherwise: install it yourself, then locate it).
    pub can_download: bool,
    pub releases_page: &'static str,
    pub cemu: Option<CemuStatus>,
    pub controllers_assigned: bool,
    pub game_dirs: Vec<String>,
    pub games: Vec<GameEntry>,
    pub download: DownloadState,
}

#[derive(Debug, thiserror::Error)]
pub enum SetupError {
    #[error("Cemu isn't set up yet")]
    NoCemu,
    #[error("no Cemu found in {0}")]
    NotCemu(PathBuf),
    #[error("{0} is not a folder")]
    NotAFolder(PathBuf),
    #[error("that game isn't in your game folders")]
    UnknownGame,
    #[error("a download is already running")]
    Busy,
    #[error(transparent)]
    Settings(#[from] install::SettingsError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

fn platform_name(p: Platform) -> &'static str {
    match p {
        Platform::Windows => "windows",
        Platform::MacOs => "macos",
        Platform::Linux => "linux",
    }
}

impl Setup {
    pub fn new(cfg: SetupConfig) -> Arc<Self> {
        let saved = std::fs::read(cfg.data_dir.join("setup.json"))
            .ok()
            .and_then(|d| serde_json::from_slice(&d).ok())
            .unwrap_or_default();
        Arc::new(Self {
            cfg,
            platform: Platform::current(),
            saved: Mutex::new(saved),
            download: Mutex::new(DownloadState::Idle),
        })
    }

    fn managed_dir(&self) -> PathBuf {
        self.cfg.data_dir.join("Cemu")
    }

    fn save(&self) -> std::io::Result<()> {
        let data =
            serde_json::to_vec_pretty(&*self.saved.lock().unwrap_or_else(|e| e.into_inner())).expect("serializes");
        std::fs::create_dir_all(&self.cfg.data_dir)?;
        std::fs::write(self.cfg.data_dir.join("setup.json"), data)
    }

    fn env(k: &str) -> Option<std::ffi::OsString> {
        std::env::var_os(k)
    }

    /// The Cemu in use: the one the user located, else the managed one if installed.
    pub fn install(&self) -> Option<CemuInstall> {
        let chosen = self.saved.lock().unwrap_or_else(|e| e.into_inner()).cemu_dir.clone();
        chosen
            .into_iter()
            .chain([self.managed_dir()])
            .find_map(|d| CemuInstall::find(&d, self.platform, &Self::env))
    }

    pub fn status(&self) -> SetupStatus {
        let install = self.install();
        let game_dirs = install
            .as_ref()
            .and_then(|i| install::game_paths(i).ok())
            .unwrap_or_default();
        let mut games: Vec<GameEntry> = game_dirs
            .iter()
            .flat_map(|d| install::scan_games(Path::new(d)))
            .collect();
        games.dedup_by(|a, b| a.path == b.path);
        let managed_dir = self.managed_dir();
        SetupStatus {
            platform: platform_name(self.platform),
            can_download: matches!(self.platform, Platform::Windows | Platform::Linux),
            releases_page: RELEASES_PAGE,
            controllers_assigned: install.as_ref().is_some_and(install::wiimotes_assigned),
            cemu: install.map(|i| CemuStatus {
                managed: i.dir == managed_dir,
                version: std::fs::read_to_string(i.dir.join(".wiiu-connect-version")).ok(),
                dir: i.dir,
                executable: i.executable,
                config_dir: i.config_dir,
                portable: i.portable,
            }),
            game_dirs,
            games,
            download: self.download.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        }
    }

    /// Uses an existing Cemu in `dir` (the folder with `Cemu.exe`, or `Cemu.app` on macOS).
    pub fn locate(&self, dir: &Path) -> Result<CemuStatus, SetupError> {
        let install =
            CemuInstall::find(dir, self.platform, &Self::env).ok_or_else(|| SetupError::NotCemu(dir.to_path_buf()))?;
        self.saved.lock().unwrap_or_else(|e| e.into_inner()).cemu_dir = Some(install.dir.clone());
        self.save()?;
        // A Cemu is set up now; an earlier failed download is no longer relevant.
        let mut d = self.download.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(*d, DownloadState::Failed { .. }) {
            *d = DownloadState::Idle;
        }
        drop(d);
        Ok(self.status().cemu.expect("just located"))
    }

    pub fn assign_controllers(&self) -> Result<install::AssignReport, SetupError> {
        let install = self.install().ok_or(SetupError::NoCemu)?;
        Ok(install::assign_wiimotes(&install)?)
    }

    pub fn add_game_folder(&self, dir: &Path) -> Result<usize, SetupError> {
        if !dir.is_dir() {
            return Err(SetupError::NotAFolder(dir.to_path_buf()));
        }
        let install = self.install().ok_or(SetupError::NoCemu)?;
        install::add_game_path(&install, dir)?;
        Ok(install::scan_games(dir).len())
    }

    /// Starts Cemu, optionally with one of the games found in the user's game folders.
    pub fn launch(&self, game: Option<&Path>, fullscreen: bool) -> Result<(), SetupError> {
        let install = self.install().ok_or(SetupError::NoCemu)?;
        if let Some(game) = game {
            // Only games from the scanned list: the page can't make us open arbitrary files.
            if !self.status().games.iter().any(|g| g.path == game) {
                return Err(SetupError::UnknownGame);
            }
        }
        let mut args: Vec<std::ffi::OsString> = Vec::new();
        if let Some(game) = game {
            args.push("-g".into());
            args.push(game.into());
        }
        if fullscreen {
            args.push("-f".into());
        }
        let mut cmd = if self.platform == Platform::MacOs {
            let mut c = std::process::Command::new("open");
            c.arg("-a").arg(&install.executable).arg("--args").args(&args);
            c
        } else {
            let mut c = std::process::Command::new(&install.executable);
            c.args(&args);
            if let Some(dir) = install.executable.parent() {
                c.current_dir(dir);
            }
            c
        };
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        cmd.spawn()?;
        Ok(())
    }

    /// Starts downloading the latest official Cemu release in the background (progress in [`Setup::status`]).
    pub fn start_download(self: &Arc<Self>) -> Result<(), SetupError> {
        if !matches!(self.platform, Platform::Windows | Platform::Linux) {
            return Err(SetupError::Other(format!(
                "Automatic install isn't available on macOS yet. Download Cemu from {RELEASES_PAGE}, then locate it."
            )));
        }
        {
            let mut d = self.download.lock().unwrap_or_else(|e| e.into_inner());
            if matches!(*d, DownloadState::Running { .. }) {
                return Err(SetupError::Busy);
            }
            *d = DownloadState::Running {
                phase: "Finding the latest Cemu release".into(),
                received: 0,
                total: None,
            };
        }
        let this = self.clone();
        std::thread::Builder::new()
            .name("cemu-download".into())
            .spawn(move || {
                let result = this.download_and_install();
                let mut d = this.download.lock().unwrap_or_else(|e| e.into_inner());
                *d = match result {
                    Ok(version) => DownloadState::Done { version },
                    Err(e) => {
                        tracing::warn!("Cemu install failed: {e}");
                        DownloadState::Failed { error: e.to_string() }
                    }
                };
            })?;
        Ok(())
    }

    fn set_phase(&self, phase: &str, received: u64, total: Option<u64>) {
        *self.download.lock().unwrap_or_else(|e| e.into_inner()) = DownloadState::Running {
            phase: phase.into(),
            received,
            total,
        };
    }

    fn download_and_install(&self) -> Result<String, SetupError> {
        let agent = ureq::AgentBuilder::new()
            .tls_config(tls_config())
            .try_proxy_from_env(true)
            .timeout_connect(std::time::Duration::from_secs(15))
            .user_agent(concat!("wiiu-connect/", env!("CARGO_PKG_VERSION")))
            .build();
        let release: Release = agent
            .get(RELEASES_API)
            .set("Accept", "application/vnd.github+json")
            .call()
            .map_err(|e| SetupError::Other(format!("couldn't reach GitHub: {e}")))?
            .into_json()?;
        let asset = pick_asset(self.platform, &release.assets)
            .ok_or_else(|| {
                SetupError::Other(format!(
                    "no {} download in Cemu {}",
                    platform_name(self.platform),
                    release.tag_name
                ))
            })?
            .clone();
        if !asset.browser_download_url.starts_with(RELEASE_URL_PREFIX) || asset.size > MAX_DOWNLOAD_BYTES {
            return Err(SetupError::Other("unexpected Cemu download".into()));
        }

        let downloads = self.cfg.data_dir.join("downloads");
        std::fs::create_dir_all(&downloads)?;
        let file = downloads.join(&asset.name).with_extension("part");
        let total = Some(asset.size);
        self.set_phase(&format!("Downloading {}", asset.name), 0, total);
        let resp = agent
            .get(&asset.browser_download_url)
            .call()
            .map_err(|e| SetupError::Other(format!("download failed: {e}")))?;
        let mut reader = resp.into_reader().take(MAX_DOWNLOAD_BYTES + 1);
        let mut out = std::fs::File::create(&file)?;
        let mut buf = vec![0u8; 256 * 1024];
        let mut received = 0u64;
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
            received += n as u64;
            if received > MAX_DOWNLOAD_BYTES {
                return Err(SetupError::Other("download larger than expected".into()));
            }
            self.set_phase(&format!("Downloading {}", asset.name), received, total);
        }
        out.sync_all()?;
        drop(out);
        if received != asset.size {
            return Err(SetupError::Other(format!(
                "download incomplete ({received} of {} bytes)",
                asset.size
            )));
        }

        self.set_phase("Installing", received, total);
        let target = self.managed_dir();
        match self.platform {
            Platform::Windows => install_zip(&file, &target, &self.cfg.data_dir)?,
            Platform::Linux => install_appimage(&file, &asset.name, &target)?,
            Platform::MacOs => unreachable!("checked in start_download"),
        }
        let _ = std::fs::remove_file(&file);
        std::fs::write(target.join(".wiiu-connect-version"), &release.tag_name)?;
        {
            let mut saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
            saved.cemu_dir = Some(target.clone());
        }
        self.save()?;
        // A fresh managed install has no controller setup yet: point Controllers 2–4 at the phones.
        let install = CemuInstall::find(&target, self.platform, &Self::env).ok_or(SetupError::NotCemu(target))?;
        install::assign_wiimotes(&install)?;
        Ok(release.tag_name)
    }
}

/// TLS roots for the Cemu download: the OS certificate store (so HTTPS-inspecting antivirus or company proxies
/// the user's PC already trusts keep working) plus the bundled Mozilla roots as a fallback.
fn tls_config() -> Arc<rustls::ClientConfig> {
    crate::tls::install_crypto_provider();
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let native = rustls_native_certs::load_native_certs();
    for e in &native.errors {
        tracing::debug!("skipping an OS certificate: {e}");
    }
    let (added, _ignored) = roots.add_parsable_certificates(native.certs);
    tracing::debug!(
        "TLS roots: {} bundled + {added} from the OS",
        webpki_roots::TLS_SERVER_ROOTS.len()
    );
    Arc::new(
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    )
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

/// The release file for this platform: the portable Windows zip (not the installer) or the Linux AppImage.
pub fn pick_asset(platform: Platform, assets: &[Asset]) -> Option<&Asset> {
    assets.iter().find(|a| {
        let n = a.name.to_ascii_lowercase();
        match platform {
            Platform::Windows => n.ends_with("-windows-x64.zip"),
            Platform::Linux => n.ends_with("-x86_64.appimage"),
            Platform::MacOs => false,
        }
    })
}

/// Extracts the Windows zip into `target` (keeping an existing install's `portable` data), making it portable.
fn install_zip(zip_path: &Path, target: &Path, data_dir: &Path) -> Result<(), SetupError> {
    let staging = data_dir.join("Cemu.new");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;
    let mut archive =
        zip::ZipArchive::new(std::fs::File::open(zip_path)?).map_err(|e| SetupError::Other(format!("bad zip: {e}")))?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| SetupError::Other(format!("bad zip: {e}")))?;
        // `enclosed_name` rejects absolute paths and `..` (zip-slip).
        let Some(rel) = entry.enclosed_name() else {
            return Err(SetupError::Other(format!("unsafe path in zip: {}", entry.name())));
        };
        let out = staging.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
        } else {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::io::copy(&mut entry, &mut std::fs::File::create(&out)?)?;
        }
    }
    // The zip contains one top folder (e.g. `Cemu_2.6/`) with Cemu.exe inside.
    let root =
        find_dir_with(&staging, "Cemu.exe", 2).ok_or_else(|| SetupError::Other("Cemu.exe not in the zip".into()))?;
    let old_portable = target.join("portable");
    if old_portable.is_dir() {
        let new_portable = root.join("portable");
        let _ = std::fs::remove_dir_all(&new_portable);
        std::fs::rename(&old_portable, &new_portable)?;
    }
    if target.exists() {
        std::fs::remove_dir_all(target)?;
    }
    std::fs::rename(&root, target)?;
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(target.join("portable"))?;
    Ok(())
}

fn install_appimage(file: &Path, name: &str, target: &Path) -> Result<(), SetupError> {
    std::fs::create_dir_all(target.join("portable"))?;
    // Remove older AppImages so `CemuInstall::find` picks the new one.
    for e in std::fs::read_dir(target)?.flatten() {
        if e.file_name().to_string_lossy().ends_with(".AppImage") {
            let _ = std::fs::remove_file(e.path());
        }
    }
    let dest = target.join(name);
    std::fs::rename(file, &dest)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

fn find_dir_with(dir: &Path, file: &str, depth: usize) -> Option<PathBuf> {
    if dir.join(file).is_file() {
        return Some(dir.to_path_buf());
    }
    if depth == 0 {
        return None;
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().is_dir())
        .find_map(|e| find_dir_with(&e.path(), file, depth - 1))
}

/// Shows the OS folder picker on this PC. `None` when cancelled or when no picker is available.
pub async fn pick_folder(title: &str) -> Option<PathBuf> {
    let output = match Platform::current() {
        Platform::Windows => {
            // The title is a fixed string from this program, never user input.
            let script = format!(
                "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8;\
                 Add-Type -AssemblyName System.Windows.Forms;\
                 $f=New-Object System.Windows.Forms.FolderBrowserDialog;\
                 $f.Description='{}';$f.ShowNewFolderButton=$false;\
                 $o=New-Object System.Windows.Forms.Form -Property @{{TopMost=$true}};\
                 if($f.ShowDialog($o) -eq [System.Windows.Forms.DialogResult]::OK){{[Console]::Out.Write($f.SelectedPath)}}",
                title.replace('\'', "''")
            );
            tokio::process::Command::new("powershell")
                .args(["-NoProfile", "-STA", "-Command", &script])
                .output()
                .await
        }
        Platform::MacOs => {
            let script = format!("POSIX path of (choose folder with prompt \"{}\")", title.replace('"', "'"));
            tokio::process::Command::new("osascript").args(["-e", &script]).output().await
        }
        Platform::Linux => {
            tokio::process::Command::new("zenity")
                .args(["--file-selection", "--directory", "--title", title])
                .output()
                .await
        }
    }
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8(output.stdout).ok()?;
    let path = path.trim_end_matches(['\r', '\n']);
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// Opens `url` in the default browser (best effort).
pub fn open_browser(url: &str) {
    let result = match Platform::current() {
        Platform::Windows => std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn(),
        Platform::MacOs => std::process::Command::new("open").arg(url).spawn(),
        Platform::Linux => std::process::Command::new("xdg-open").arg(url).spawn(),
    };
    if let Err(e) = result {
        tracing::info!("couldn't open a browser ({e}); open {url} yourself");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> Asset {
        Asset {
            name: name.into(),
            browser_download_url: format!("{RELEASE_URL_PREFIX}v2.6/{name}"),
            size: 1,
        }
    }

    #[test]
    fn picks_the_portable_windows_zip_and_the_appimage() {
        let assets = [
            asset("cemu-2.6-windows-x64-installer.exe"),
            asset("cemu-2.6-windows-x64.zip"),
            asset("cemu-2.6-ubuntu-22.04-x64.zip"),
            asset("Cemu-2.6-x86_64.AppImage"),
            asset("cemu-2.6-macos-12-x64.dmg"),
        ];
        assert_eq!(
            pick_asset(Platform::Windows, &assets).unwrap().name,
            "cemu-2.6-windows-x64.zip"
        );
        assert_eq!(
            pick_asset(Platform::Linux, &assets).unwrap().name,
            "Cemu-2.6-x86_64.AppImage"
        );
        assert!(pick_asset(Platform::MacOs, &assets).is_none());
    }

    fn temp(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("wiiu-setup-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn make_zip(path: &Path, files: &[(&str, &[u8])]) {
        let mut w = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, data) in files {
            w.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();
    }

    #[test]
    fn installs_zip_portably_and_keeps_user_data_on_update() {
        let d = temp("zip");
        let target = d.join("Cemu");
        let z = d.join("cemu.zip");
        make_zip(&z, &[("Cemu_2.5/Cemu.exe", b"v1"), ("Cemu_2.5/resources/a.txt", b"x")]);
        install_zip(&z, &target, &d).unwrap();
        assert_eq!(std::fs::read(target.join("Cemu.exe")).unwrap(), b"v1");
        assert!(target.join("portable").is_dir());
        std::fs::write(target.join("portable/settings.xml"), "<content/>").unwrap();

        make_zip(&z, &[("Cemu_2.6/Cemu.exe", b"v2")]);
        install_zip(&z, &target, &d).unwrap();
        assert_eq!(std::fs::read(target.join("Cemu.exe")).unwrap(), b"v2");
        assert_eq!(
            std::fs::read_to_string(target.join("portable/settings.xml")).unwrap(),
            "<content/>"
        );
        assert!(!target.join("resources").exists(), "old files replaced");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rejects_zip_slip() {
        let d = temp("slip");
        let z = d.join("evil.zip");
        make_zip(&z, &[("../evil.txt", b"x"), ("Cemu/Cemu.exe", b"v")]);
        assert!(install_zip(&z, &d.join("Cemu"), &d).is_err());
        assert!(!d.parent().unwrap().join("evil.txt").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn launch_refuses_paths_that_are_not_scanned_games() {
        let d = temp("launch");
        let setup = Setup::new(SetupConfig { data_dir: d.clone() });
        assert!(matches!(setup.launch(None, false), Err(SetupError::NoCemu)));
        let _ = std::fs::remove_dir_all(&d);
    }
}
