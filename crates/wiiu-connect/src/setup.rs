//! First-run setup from the PC's browser (`/setup`): get Cemu, point its Wii Remotes at Wii U Connect, add the
//! user's own game folders, and start games.
//!
//! Cemu is downloaded only from its official GitHub releases (cemu-project/Cemu, MPL-2.0) into a portable
//! folder that Wii U Connect manages. **No games, keys or Nintendo system files are ever downloaded**: games
//! come from folders the user picks (their own dumps).

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use wiiu_cemu::install::{self, CemuInstall, GameEntry};
use wiiu_cemu::Platform;

const RELEASES_API: &str = "https://api.github.com/repos/cemu-project/Cemu/releases/latest";
const RELEASE_URL_PREFIX: &str = "https://github.com/cemu-project/Cemu/releases/download/";
pub const RELEASES_PAGE: &str = "https://github.com/cemu-project/Cemu/releases/latest";
const CEMU_SOURCE: &str = "https://github.com/cemu-project/Cemu";
/// Used for downloaded releases, which ship their own copy of the license text.
const MPL_NOTE: &str = "The full license text: https://www.mozilla.org/MPL/2.0/\n";

/// An official Cemu release built into the executable (Windows release builds; see `build.rs`).
pub struct BundledCemu {
    /// Release tag, e.g. `v2.6`.
    pub tag: &'static str,
    /// The release's portable Windows zip, unchanged.
    pub zip: &'static [u8],
    /// Cemu's license (MPL-2.0), copied next to the unpacked Cemu.
    pub license: &'static str,
}

mod embedded_cemu {
    use super::BundledCemu;
    include!(concat!(env!("OUT_DIR"), "/embedded_cemu.rs"));
}

/// The Cemu built into this executable, if any. Only the Windows zip is ever built in.
pub fn bundled_cemu() -> Option<&'static BundledCemu> {
    embedded_cemu::BUNDLED_CEMU.as_ref()
}
const MAX_DOWNLOAD_BYTES: u64 = 400 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct SetupConfig {
    /// Wii U Connect's data folder; the managed Cemu goes to `<data_dir>/Cemu`.
    pub data_dir: PathBuf,
    /// Run [`Setup::start_auto_setup`] when the server starts.
    pub auto: bool,
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

type EnvLookup = dyn Fn(&str) -> Option<std::ffi::OsString> + Send + Sync;

pub struct Setup {
    cfg: SetupConfig,
    /// Environment variable lookup (replaced in tests).
    env: Box<EnvLookup>,
    /// [`bundled_cemu`] (replaced in tests).
    bundled: Option<&'static BundledCemu>,
    /// Progress of the automatic setup, shown at the top of the setup page.
    auto: Mutex<Vec<AutoStep>>,
    platform: Platform,
    /// Where Cemu reaches our DSU server; written into the controller profiles.
    dsu: SocketAddrV4,
    saved: Mutex<Saved>,
    download: Mutex<DownloadState>,
    /// Last game scan. Scanning walks whole folders (possibly on slow or network drives), so the status page's
    /// polling reuses it; `scanning` makes concurrent polls wait for one scan instead of starting their own.
    games: Mutex<Option<GameScan>>,
    scanning: Mutex<()>,
}

/// One step of the automatic setup.
#[derive(Debug, Clone, Serialize)]
pub struct AutoStep {
    pub id: &'static str,
    pub title: &'static str,
    pub state: StepState,
    /// What happened, for the user ("Downloaded Cemu 2.6", "Found 3 games in D:\\Wii U").
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Pending,
    Running,
    Done,
    /// Nothing to do, or the user has to do it (e.g. no games found yet).
    Skipped,
    Failed,
}

const AUTO_STEPS: [(&str, &str); 4] = [
    ("cemu", "Get Cemu"),
    ("controllers", "Connect Cemu's controllers to the phones"),
    ("gamepad_window", "Open Cemu's GamePad window"),
    ("games", "Find your games"),
];

struct GameScan {
    dirs: Vec<String>,
    games: Vec<GameEntry>,
    at: Instant,
}

fn games_word(n: usize) -> String {
    if n == 1 {
        "1 game".into()
    } else {
        format!("{n} games")
    }
}

/// How long a game scan is reused when the game folders haven't changed.
const GAME_SCAN_TTL: Duration = Duration::from_secs(15);

/// The address Cemu (on this PC) should use for a DSU server bound to `bound`.
pub fn cemu_dsu_endpoint(bound: SocketAddr) -> SocketAddrV4 {
    let ip = match bound.ip() {
        std::net::IpAddr::V4(v4) if !v4.is_unspecified() => v4,
        _ => Ipv4Addr::LOCALHOST,
    };
    SocketAddrV4::new(ip, bound.port())
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
    /// Wii U Connect's own version.
    pub version: &'static str,
    pub platform: &'static str,
    /// Whether Wii U Connect can download Cemu on this OS (otherwise: install it yourself, then locate it).
    pub can_download: bool,
    /// Version of the Cemu built into this executable (installable without internet), if any.
    pub bundled_cemu: Option<&'static str>,
    pub releases_page: &'static str,
    pub cemu: Option<CemuStatus>,
    pub controllers_assigned: bool,
    pub game_dirs: Vec<String>,
    pub games: Vec<GameEntry>,
    pub download: DownloadState,
    /// Automatic setup progress (empty when it is off).
    pub auto: Vec<AutoStep>,
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
    /// `dsu` is the address the DSU server is bound to (see [`cemu_dsu_endpoint`]).
    pub fn new(cfg: SetupConfig, dsu: SocketAddr) -> Arc<Self> {
        let saved = std::fs::read(cfg.data_dir.join("setup.json"))
            .ok()
            .and_then(|d| serde_json::from_slice(&d).ok())
            .unwrap_or_default();
        Arc::new(Self {
            cfg,
            env: Box::new(|k| std::env::var_os(k)),
            bundled: bundled_cemu(),
            auto: Mutex::new(Vec::new()),
            platform: Platform::current(),
            dsu: cemu_dsu_endpoint(dsu),
            saved: Mutex::new(saved),
            download: Mutex::new(DownloadState::Idle),
            games: Mutex::new(None),
            scanning: Mutex::new(()),
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

    fn env(&self, k: &str) -> Option<std::ffi::OsString> {
        (self.env)(k)
    }

    /// For tests: act as a Windows build with `bundled` built in.
    #[cfg(test)]
    fn with_bundled_windows_cemu(mut self: Arc<Self>, bundled: &'static BundledCemu) -> Arc<Self> {
        let this = Arc::get_mut(&mut self).expect("not shared yet");
        this.platform = Platform::Windows;
        this.bundled = Some(bundled);
        self
    }

    /// For tests: read "environment variables" (home folder, `LOCALAPPDATA`, …) from `env` instead.
    #[cfg(test)]
    fn with_env(
        mut self: Arc<Self>,
        env: impl Fn(&str) -> Option<std::ffi::OsString> + Send + Sync + 'static,
    ) -> Arc<Self> {
        Arc::get_mut(&mut self).expect("not shared yet").env = Box::new(env);
        self
    }

    /// The Cemu in use: the one the user located, else the managed one if installed.
    pub fn install(&self) -> Option<CemuInstall> {
        let chosen = self.saved.lock().unwrap_or_else(|e| e.into_inner()).cemu_dir.clone();
        chosen
            .into_iter()
            .chain([self.managed_dir()])
            .find_map(|d| CemuInstall::find(&d, self.platform, &|k| self.env(k)))
    }

    pub fn status(&self) -> SetupStatus {
        let install = self.install();
        let game_dirs = install
            .as_ref()
            .and_then(|i| install::game_paths(i).ok())
            .unwrap_or_default();
        let games = self.games_in(&game_dirs);
        let managed_dir = self.managed_dir();
        SetupStatus {
            version: env!("CARGO_PKG_VERSION"),
            platform: platform_name(self.platform),
            can_download: matches!(self.platform, Platform::Windows | Platform::Linux),
            bundled_cemu: self.bundled().map(|b| b.tag),
            releases_page: RELEASES_PAGE,
            controllers_assigned: install
                .as_ref()
                .is_some_and(|i| install::controllers_assigned(i, self.dsu)),
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
            auto: self.auto.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        }
    }

    /// Games in `dirs`, from the last scan when it is recent and covered the same folders.
    fn games_in(&self, dirs: &[String]) -> Vec<GameEntry> {
        let cached = |c: &Option<GameScan>| {
            c.as_ref()
                .filter(|c| c.dirs == dirs && c.at.elapsed() < GAME_SCAN_TTL)
                .map(|c| c.games.clone())
        };
        if let Some(g) = cached(&self.games.lock().unwrap_or_else(|e| e.into_inner())) {
            return g;
        }
        let _one_scan_at_a_time = self.scanning.lock().unwrap_or_else(|e| e.into_inner());
        // Another request may have finished a scan while this one waited.
        if let Some(g) = cached(&self.games.lock().unwrap_or_else(|e| e.into_inner())) {
            return g;
        }
        let mut games: Vec<GameEntry> = dirs.iter().flat_map(|d| install::scan_games(Path::new(d))).collect();
        games.dedup_by(|a, b| a.path == b.path);
        *self.games.lock().unwrap_or_else(|e| e.into_inner()) = Some(GameScan {
            dirs: dirs.to_vec(),
            games: games.clone(),
            at: Instant::now(),
        });
        games
    }

    fn forget_games(&self) {
        *self.games.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// Whether [`SetupConfig::auto`] asks for the automatic setup.
    pub fn auto_enabled(&self) -> bool {
        self.cfg.auto
    }

    /// Does everything the setup page offers, in the background, without asking: use an existing Cemu or install
    /// the official one, point Cemu's controllers at the phones (backing up the user's own), open Cemu's GamePad
    /// window, and add the folders that hold the user's own games. Each step's result is shown on the setup page;
    /// whatever fails or finds nothing is left for the user to do there.
    pub fn start_auto_setup(self: &Arc<Self>) {
        *self.auto.lock().unwrap_or_else(|e| e.into_inner()) = AUTO_STEPS
            .iter()
            .map(|&(id, title)| AutoStep {
                id,
                title,
                state: StepState::Pending,
                detail: None,
            })
            .collect();
        let this = self.clone();
        let spawned = std::thread::Builder::new()
            .name("auto-setup".into())
            .spawn(move || this.run_auto_setup());
        if let Err(e) = spawned {
            tracing::warn!("could not start the automatic setup: {e}");
        }
    }

    fn set_step(&self, id: &str, state: StepState, detail: Option<String>) {
        let mut steps = self.auto.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(step) = steps.iter_mut().find(|s| s.id == id) {
            step.state = state;
            step.detail = detail;
        }
    }

    fn run_auto_setup(self: &Arc<Self>) {
        let step = |id: &str, f: &dyn Fn() -> Result<(StepState, String), SetupError>| {
            self.set_step(id, StepState::Running, None);
            match f() {
                Ok((state, detail)) => self.set_step(id, state, Some(detail)),
                Err(e) => {
                    tracing::warn!("automatic setup, {id}: {e}");
                    self.set_step(id, StepState::Failed, Some(e.to_string()));
                }
            }
        };
        step("cemu", &|| self.auto_cemu());
        if self.install().is_none() {
            for (id, _) in &AUTO_STEPS[1..] {
                self.set_step(id, StepState::Skipped, Some("Needs Cemu first".into()));
            }
            return;
        }
        step("controllers", &|| {
            let install = self.install().ok_or(SetupError::NoCemu)?;
            if install::controllers_assigned(&install, self.dsu) {
                return Ok((StepState::Done, "Already set up".into()));
            }
            let report = install::assign_controllers(&install, self.dsu)?;
            let detail = if report.backups.is_empty() {
                "Controller 1 is the phone GamePad, Controllers 2–4 the phone Wii Remotes".to_string()
            } else {
                format!(
                    "Done; your previous settings were backed up ({} files)",
                    report.backups.len()
                )
            };
            Ok((StepState::Done, detail))
        });
        step("gamepad_window", &|| {
            let install = self.install().ok_or(SetupError::NoCemu)?;
            let changed = install::enable_gamepad_window(&install)?;
            Ok((
                StepState::Done,
                if changed {
                    "It opens when Cemu starts"
                } else {
                    "Already on"
                }
                .into(),
            ))
        });
        step("games", &|| self.auto_games());
    }

    fn auto_cemu(self: &Arc<Self>) -> Result<(StepState, String), SetupError> {
        if let Some(install) = self.install() {
            return Ok((StepState::Done, format!("Using Cemu in {}", install.dir.display())));
        }
        // Installed with Cemu's own installer?
        for dir in install::installed_cemu_dirs(self.platform, &|k| self.env(k)) {
            if CemuInstall::find(&dir, self.platform, &|k| self.env(k)).is_some() {
                let found = self.locate(&dir)?;
                return Ok((
                    StepState::Done,
                    format!("Using your installed Cemu in {}", found.dir.display()),
                ));
            }
        }
        if !matches!(self.platform, Platform::Windows | Platform::Linux) {
            return Ok((
                StepState::Skipped,
                "Install Cemu from its website, then choose it below".into(),
            ));
        }
        let bundled = self.bundled();
        let started = match bundled {
            Some(_) => self.start_bundled_install(),
            None => self.start_download(),
        };
        match started {
            Ok(()) | Err(SetupError::Busy) => {}
            Err(e) => return Err(e),
        }
        loop {
            std::thread::sleep(Duration::from_millis(300));
            let state = self.download.lock().unwrap_or_else(|e| e.into_inner()).clone();
            match state {
                DownloadState::Running { .. } => continue,
                DownloadState::Done { version } => {
                    let how = if bundled.is_some() { "included" } else { "official" };
                    return Ok((StepState::Done, format!("Installed the {how} Cemu {version}")));
                }
                DownloadState::Failed { error } => return Err(SetupError::Other(error)),
                DownloadState::Idle => return Err(SetupError::Other("the download stopped".into())),
            }
        }
    }

    fn auto_games(&self) -> Result<(StepState, String), SetupError> {
        let install = self.install().ok_or(SetupError::NoCemu)?;
        let existing = install::game_paths(&install)?;
        if !existing.is_empty() {
            let count = self.games_in(&existing).len();
            return Ok((
                StepState::Done,
                format!("{} in your Cemu game folders", games_word(count)),
            ));
        }
        let roots = install::game_search_roots(self.platform, &|k| self.env(k));
        let folders = install::discover_game_folders(&roots);
        if folders.is_empty() {
            return Ok((
                StepState::Skipped,
                "No games found in the usual folders. Add the folder with your game dumps below.".into(),
            ));
        }
        for folder in &folders {
            install::add_game_path(&install, folder)?;
        }
        self.forget_games();
        let dirs = install::game_paths(&install)?;
        let count = self.games_in(&dirs).len();
        let names: Vec<String> = folders.iter().map(|f| f.display().to_string()).collect();
        Ok((
            StepState::Done,
            format!("Found {} in {}", games_word(count), names.join(", ")),
        ))
    }

    /// Uses an existing Cemu in `dir` (the folder with `Cemu.exe`, or `Cemu.app` on macOS).
    pub fn locate(&self, dir: &Path) -> Result<CemuStatus, SetupError> {
        let install = CemuInstall::find(dir, self.platform, &|k| self.env(k))
            .ok_or_else(|| SetupError::NotCemu(dir.to_path_buf()))?;
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
        Ok(install::assign_controllers(&install, self.dsu)?)
    }

    pub fn add_game_folder(&self, dir: &Path) -> Result<usize, SetupError> {
        if !dir.is_dir() {
            return Err(SetupError::NotAFolder(dir.to_path_buf()));
        }
        let install = self.install().ok_or(SetupError::NoCemu)?;
        install::add_game_path(&install, dir)?;
        self.forget_games();
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
        self.start_job("Finding the latest Cemu release", |this| this.download_and_install())
    }

    /// Installs the Cemu built into this executable (no download), like [`Self::start_download`].
    pub fn start_bundled_install(self: &Arc<Self>) -> Result<(), SetupError> {
        let bundled = self
            .bundled()
            .ok_or_else(|| SetupError::Other("no Cemu is built into this version".into()))?;
        self.start_job("Unpacking the included Cemu", move |this| this.install_bundled(bundled))
    }

    /// The built-in Cemu, when there is one for this platform.
    fn bundled(&self) -> Option<&'static BundledCemu> {
        self.bundled.filter(|_| self.platform == Platform::Windows)
    }

    /// Runs `job` (which installs Cemu and returns its version) on a background thread, reporting through
    /// [`DownloadState`]. Only one install runs at a time.
    fn start_job(
        self: &Arc<Self>,
        first_phase: &str,
        job: impl FnOnce(&Arc<Self>) -> Result<String, SetupError> + Send + 'static,
    ) -> Result<(), SetupError> {
        {
            let mut d = self.download.lock().unwrap_or_else(|e| e.into_inner());
            if matches!(*d, DownloadState::Running { .. }) {
                return Err(SetupError::Busy);
            }
            *d = DownloadState::Running {
                phase: first_phase.into(),
                received: 0,
                total: None,
            };
        }
        let this = self.clone();
        std::thread::Builder::new().name("cemu-install".into()).spawn(move || {
            let result = job(&this);
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
        let asset = self.resolve_download(&agent)?;
        if !asset.url.starts_with(RELEASE_URL_PREFIX) || asset.size.is_some_and(|s| s > MAX_DOWNLOAD_BYTES) {
            return Err(SetupError::Other("unexpected Cemu download".into()));
        }

        let downloads = self.cfg.data_dir.join("downloads");
        std::fs::create_dir_all(&downloads)?;
        let file = downloads.join(&asset.name).with_extension("part");
        self.set_phase(&format!("Downloading {}", asset.name), 0, asset.size);
        let resp = agent
            .get(&asset.url)
            .call()
            .map_err(|e| SetupError::Other(format!("download failed: {e}")))?;
        let expected = asset
            .size
            .or_else(|| resp.header("content-length").and_then(|v| v.parse().ok()));
        let total = expected;
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
        if let Some(expected) = expected.filter(|&e| e != received) {
            return Err(SetupError::Other(format!(
                "download incomplete ({received} of {expected} bytes)"
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
        self.finish_managed_install(&target, &asset.tag)?;
        Ok(asset.tag)
    }

    /// Unpacks the built-in Cemu into the managed folder. Same result as a download, without the network.
    fn install_bundled(&self, bundled: &BundledCemu) -> Result<String, SetupError> {
        let downloads = self.cfg.data_dir.join("downloads");
        std::fs::create_dir_all(&downloads)?;
        let file = downloads.join("cemu-included.zip.part");
        let total = bundled.zip.len() as u64;
        self.set_phase("Unpacking the included Cemu", 0, Some(total));
        std::fs::write(&file, bundled.zip)?;
        self.set_phase("Installing", total, Some(total));
        let target = self.managed_dir();
        let result = install_zip(&file, &target, &self.cfg.data_dir);
        let _ = std::fs::remove_file(&file);
        result?;
        self.finish_managed_install(&target, bundled.tag)?;
        Ok(bundled.tag.to_string())
    }

    /// Records a fresh managed install in `target` and points its Controllers 2–4 at the phones.
    fn finish_managed_install(&self, target: &Path, tag: &str) -> Result<(), SetupError> {
        std::fs::write(target.join(".wiiu-connect-version"), tag)?;
        // MPL-2.0: Cemu's license and where its source is, next to the Cemu we put there.
        std::fs::write(
            target.join("CEMU-LICENSE.txt"),
            format!(
                "Cemu {tag} is free software by the Cemu project, licensed under the Mozilla Public License 2.0 \
                 (below).\nSource code: {CEMU_SOURCE} (tag {tag}). Wii U Connect installed this unmodified official \
                 release.\n\n{}",
                self.bundled.filter(|b| b.tag == tag).map_or(MPL_NOTE, |b| b.license)
            ),
        )?;
        {
            let mut saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
            saved.cemu_dir = Some(target.to_path_buf());
        }
        self.save()?;
        // A fresh managed install has no controller setup yet: point Controllers 1–4 at the phones.
        let install = CemuInstall::find(target, self.platform, &|k| self.env(k))
            .ok_or_else(|| SetupError::NotCemu(target.to_path_buf()))?;
        install::assign_controllers(&install, self.dsu)?;
        Ok(())
    }

    /// Finds the latest release file for this platform. Asks GitHub's API first; if that is refused (it allows
    /// only 60 unauthenticated requests per hour per IP), reads the latest version from the release page's
    /// redirect and builds the official file name from it.
    fn resolve_download(&self, agent: &ureq::Agent) -> Result<Download, SetupError> {
        let api = agent
            .get(RELEASES_API)
            .set("Accept", "application/vnd.github+json")
            .call()
            .map_err(|e| e.to_string())
            .and_then(|r| r.into_json::<Release>().map_err(|e| e.to_string()));
        match api {
            Ok(release) => {
                let asset = pick_asset(self.platform, &release.assets).ok_or_else(|| {
                    SetupError::Other(format!(
                        "no {} download in Cemu {}",
                        platform_name(self.platform),
                        release.tag_name
                    ))
                })?;
                Ok(Download {
                    tag: release.tag_name.clone(),
                    name: asset.name.clone(),
                    url: asset.browser_download_url.clone(),
                    size: Some(asset.size),
                })
            }
            Err(api_error) => {
                tracing::info!("GitHub API unavailable ({api_error}); using the release page instead");
                let no_redirects = ureq::AgentBuilder::new()
                    .tls_config(tls_config())
                    .try_proxy_from_env(true)
                    .redirects(0)
                    .user_agent(concat!("wiiu-connect/", env!("CARGO_PKG_VERSION")))
                    .build();
                let resp = no_redirects
                    .get(RELEASES_PAGE)
                    .call()
                    .map_err(|e| SetupError::Other(format!("couldn't reach GitHub: {e}")))?;
                let tag = resp
                    .header("location")
                    .and_then(tag_from_release_location)
                    .ok_or_else(|| SetupError::Other(format!("couldn't find the latest Cemu version ({api_error})")))?;
                fallback_download(self.platform, &tag).ok_or_else(|| {
                    SetupError::Other(format!("no {} download in Cemu {tag}", platform_name(self.platform)))
                })
            }
        }
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

/// A release file to download.
#[derive(Debug, Clone, PartialEq)]
struct Download {
    tag: String,
    name: String,
    url: String,
    /// Known when the API answered; otherwise taken from `Content-Length`.
    size: Option<u64>,
}

/// `https://github.com/cemu-project/Cemu/releases/tag/v2.6` → `v2.6` (tags are short and plain).
fn tag_from_release_location(location: &str) -> Option<String> {
    let tag = location.split("/releases/tag/").nth(1)?.trim_end_matches('/');
    let ok =
        !tag.is_empty() && tag.len() <= 40 && tag.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b));
    ok.then(|| tag.to_string())
}

/// Official file names (Cemu's .github/workflows/deploy_release.yml): `cemu-<ver>-windows-x64.zip`,
/// `Cemu-<ver>-x86_64.AppImage`, where `<ver>` is the tag without its leading `v`.
fn fallback_download(platform: Platform, tag: &str) -> Option<Download> {
    let ver = tag.strip_prefix('v').unwrap_or(tag);
    let name = match platform {
        Platform::Windows => format!("cemu-{ver}-windows-x64.zip"),
        Platform::Linux => format!("Cemu-{ver}-x86_64.AppImage"),
        Platform::MacOs => return None,
    };
    Some(Download {
        tag: tag.to_string(),
        url: format!("{RELEASE_URL_PREFIX}{tag}/{name}"),
        name,
        size: None,
    })
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

/// Extracts the Windows zip into `target`, making it portable and keeping an existing install's `portable` folder
/// (settings, saves, updates). The user's `portable` folder never exists only in a temporary folder: the old install
/// is renamed aside as a whole, and every failure puts it back.
fn install_zip(zip_path: &Path, target: &Path, data_dir: &Path) -> Result<(), SetupError> {
    let staging = data_dir.join("Cemu.new");
    let previous = data_dir.join("Cemu.old");
    recover_interrupted_update(target, &previous)?;
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
    // A `portable` folder from the zip itself would shadow the user's.
    let _ = std::fs::remove_dir_all(root.join("portable"));
    swap_in(&root, target, &previous)?;
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(target.join("portable"))?;
    Ok(())
}

/// Replaces `target` with `new` and moves the old `portable` folder across. Fails without changing anything when
/// the old install can't be moved (on Windows: Cemu is still running).
fn swap_in(new: &Path, target: &Path, previous: &Path) -> Result<(), SetupError> {
    let busy = |e: std::io::Error| {
        SetupError::Other(format!(
            "could not replace the old Cemu in {} ({e}). Close Cemu and try again.",
            target.display()
        ))
    };
    if target.exists() {
        std::fs::rename(target, previous).map_err(busy)?;
    }
    if let Err(e) = std::fs::rename(new, target) {
        let _ = std::fs::rename(previous, target);
        return Err(e.into());
    }
    let old_portable = previous.join("portable");
    if old_portable.is_dir() {
        if let Err(e) = std::fs::rename(&old_portable, target.join("portable")) {
            // Undo: the new files go back to `new`, the old install (with its data) back to `target`.
            if std::fs::rename(target, new).is_ok() {
                let _ = std::fs::rename(previous, target);
            }
            return Err(e.into());
        }
    }
    let _ = std::fs::remove_dir_all(previous);
    Ok(())
}

/// Finishes or undoes an update that was cut off (power loss, crash) between the steps of [`swap_in`].
fn recover_interrupted_update(target: &Path, previous: &Path) -> std::io::Result<()> {
    if !previous.exists() {
        return Ok(());
    }
    let old_portable = previous.join("portable");
    if !target.exists() {
        // Cut off before the new version was moved in: restore the old install.
        return std::fs::rename(previous, target);
    }
    if old_portable.is_dir() && !target.join("portable").exists() {
        // Cut off after the new version was moved in: bring the user's data over.
        std::fs::rename(&old_portable, target.join("portable"))?;
    }
    if !old_portable.exists() {
        let _ = std::fs::remove_dir_all(previous);
    }
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
    fn fallback_builds_official_urls_from_the_release_redirect() {
        assert_eq!(
            tag_from_release_location("https://github.com/cemu-project/Cemu/releases/tag/v2.6").as_deref(),
            Some("v2.6")
        );
        assert_eq!(
            tag_from_release_location("https://evil.example/releases/tag/v1/../../x"),
            None
        );
        assert_eq!(
            tag_from_release_location("https://github.com/cemu-project/Cemu/releases"),
            None
        );
        let d = fallback_download(Platform::Windows, "v2.6").unwrap();
        assert_eq!(
            d.url,
            "https://github.com/cemu-project/Cemu/releases/download/v2.6/cemu-2.6-windows-x64.zip"
        );
        assert_eq!(
            fallback_download(Platform::Linux, "v2.6").unwrap().name,
            "Cemu-2.6-x86_64.AppImage"
        );
        assert!(fallback_download(Platform::MacOs, "v2.6").is_none());
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
    fn a_failed_swap_leaves_the_old_install_and_its_data_in_place() {
        let d = temp("swapfail");
        let target = d.join("Cemu");
        std::fs::create_dir_all(target.join("portable")).unwrap();
        std::fs::write(target.join("Cemu.exe"), b"v1").unwrap();
        std::fs::write(target.join("portable/settings.xml"), "<mine/>").unwrap();
        // The new version is missing, so moving it in fails after the old install was renamed aside.
        assert!(swap_in(&d.join("missing"), &target, &d.join("Cemu.old")).is_err());
        assert_eq!(std::fs::read(target.join("Cemu.exe")).unwrap(), b"v1");
        assert_eq!(
            std::fs::read_to_string(target.join("portable/settings.xml")).unwrap(),
            "<mine/>"
        );
        assert!(!d.join("Cemu.old").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_update_cut_off_midway_is_recovered_on_the_next_install() {
        // Cut off after the old install was renamed aside: it comes back.
        let d = temp("recover1");
        let target = d.join("Cemu");
        let previous = d.join("Cemu.old");
        std::fs::create_dir_all(previous.join("portable")).unwrap();
        std::fs::write(previous.join("portable/settings.xml"), "<mine/>").unwrap();
        recover_interrupted_update(&target, &previous).unwrap();
        assert_eq!(
            std::fs::read_to_string(target.join("portable/settings.xml")).unwrap(),
            "<mine/>"
        );
        assert!(!previous.exists());
        let _ = std::fs::remove_dir_all(&d);

        // Cut off after the new version was moved in: the user's data moves over to it.
        let d = temp("recover2");
        let target = d.join("Cemu");
        let previous = d.join("Cemu.old");
        std::fs::create_dir_all(previous.join("portable")).unwrap();
        std::fs::write(previous.join("portable/settings.xml"), "<mine/>").unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("Cemu.exe"), b"v2").unwrap();
        let z = d.join("cemu.zip");
        make_zip(&z, &[("Cemu_2.7/Cemu.exe", b"v3")]);
        install_zip(&z, &target, &d).unwrap();
        assert_eq!(std::fs::read(target.join("Cemu.exe")).unwrap(), b"v3");
        assert_eq!(
            std::fs::read_to_string(target.join("portable/settings.xml")).unwrap(),
            "<mine/>"
        );
        assert!(!previous.exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn profiles_point_at_the_bound_dsu_address() {
        let e = |s: &str| cemu_dsu_endpoint(s.parse().unwrap()).to_string();
        assert_eq!(e("127.0.0.1:26760"), "127.0.0.1:26760");
        assert_eq!(e("127.0.0.1:12345"), "127.0.0.1:12345");
        assert_eq!(e("0.0.0.0:26760"), "127.0.0.1:26760");
        assert_eq!(e("192.168.1.5:26760"), "192.168.1.5:26760");
        assert_eq!(e("[::1]:26760"), "127.0.0.1:26760");
    }

    #[test]
    fn game_scans_are_reused_until_the_folders_change() {
        let d = temp("scan");
        let games = d.join("games");
        std::fs::create_dir_all(&games).unwrap();
        std::fs::write(games.join("One.wua"), b"").unwrap();
        let setup = Setup::new(
            SetupConfig {
                data_dir: d.clone(),
                auto: false,
            },
            "127.0.0.1:26760".parse().unwrap(),
        );
        let dirs = vec![games.to_string_lossy().into_owned()];
        assert_eq!(setup.games_in(&dirs).len(), 1);
        std::fs::write(games.join("Two.wua"), b"").unwrap();
        assert_eq!(setup.games_in(&dirs).len(), 1, "cached");
        setup.forget_games();
        assert_eq!(setup.games_in(&dirs).len(), 2, "rescanned after a change");
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
    fn automatic_setup_does_every_step_without_asking() {
        let d = temp("auto");
        // An existing Cemu (Linux AppImage layout, since tests run on the build platform) with portable settings.
        let cemu = d.join("MyCemu");
        std::fs::create_dir_all(cemu.join("portable")).unwrap();
        std::fs::write(cemu.join("Cemu-2.6-x86_64.AppImage"), b"").unwrap();
        // The user's own game in Downloads.
        let home = d.join("home");
        let games = home.join("Downloads").join("Wii U");
        std::fs::create_dir_all(&games).unwrap();
        std::fs::write(games.join("Splatoon.wua"), b"").unwrap();
        let env_home = home.clone();
        let setup = Setup::new(
            SetupConfig {
                data_dir: d.join("data"),
                auto: true,
            },
            "127.0.0.1:26760".parse().unwrap(),
        )
        .with_env(move |k| (k == "HOME" || k == "USERPROFILE").then(|| env_home.clone().into_os_string()));
        if Platform::current() != Platform::Linux {
            return; // the fake Cemu above uses the Linux layout
        }
        setup.locate(&cemu).unwrap();

        let finished = |setup: &Arc<Setup>| {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let steps = setup.status().auto;
                if steps
                    .iter()
                    .all(|s| !matches!(s.state, StepState::Pending | StepState::Running))
                {
                    return steps;
                }
                assert!(Instant::now() < deadline, "automatic setup did not finish: {steps:?}");
                std::thread::sleep(Duration::from_millis(50));
            }
        };
        setup.start_auto_setup();
        let steps = finished(&setup);
        for s in &steps {
            assert_eq!(s.state, StepState::Done, "{s:?}");
        }
        let detail = |id: &str| steps.iter().find(|s| s.id == id).unwrap().detail.clone().unwrap();
        assert!(detail("games").starts_with("Found 1 game in"), "{}", detail("games"));
        let status = setup.status();
        assert!(status.controllers_assigned);
        assert_eq!(status.game_dirs, vec![games.to_string_lossy().into_owned()]);
        assert_eq!(status.games.len(), 1);
        let settings = std::fs::read_to_string(cemu.join("portable/settings.xml")).unwrap();
        assert!(settings.contains("<open_pad>true</open_pad>"), "{settings}");

        // Running again finds everything in place.
        setup.start_auto_setup();
        let again = finished(&setup);
        assert!(again.iter().all(|s| s.state == StepState::Done));
        assert_eq!(
            again.iter().find(|s| s.id == "controllers").unwrap().detail.as_deref(),
            Some("Already set up")
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A zip shaped like Cemu's portable Windows release: one top folder with `Cemu.exe`.
    fn fake_cemu_zip() -> &'static [u8] {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        w.start_file("Cemu_9.9/Cemu.exe", opts).unwrap();
        w.write_all(b"MZ").unwrap();
        w.start_file("Cemu_9.9/resources/sharedFonts/x.ttf", opts).unwrap();
        w.write_all(b"font").unwrap();
        Box::leak(w.finish().unwrap().into_inner().into_boxed_slice())
    }

    #[test]
    fn automatic_setup_installs_the_included_cemu_without_downloading() {
        let d = temp("bundled");
        let bundled: &'static BundledCemu = Box::leak(Box::new(BundledCemu {
            tag: "v9.9",
            zip: fake_cemu_zip(),
            license: "Mozilla Public License Version 2.0 (test copy)",
        }));
        let home = d.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let env_home = home.clone();
        let setup = Setup::new(
            SetupConfig {
                data_dir: d.join("data"),
                auto: true,
            },
            "127.0.0.1:26760".parse().unwrap(),
        )
        .with_env(move |k| (k == "USERPROFILE").then(|| env_home.clone().into_os_string()))
        .with_bundled_windows_cemu(bundled);
        assert_eq!(setup.status().bundled_cemu, Some("v9.9"));

        setup.start_auto_setup();
        let deadline = Instant::now() + Duration::from_secs(10);
        let steps = loop {
            let steps = setup.status().auto;
            if steps
                .iter()
                .all(|s| !matches!(s.state, StepState::Pending | StepState::Running))
            {
                break steps;
            }
            assert!(Instant::now() < deadline, "automatic setup did not finish: {steps:?}");
            std::thread::sleep(Duration::from_millis(20));
        };
        let cemu_step = &steps[0];
        assert_eq!(cemu_step.state, StepState::Done, "{steps:?}");
        assert_eq!(cemu_step.detail.as_deref(), Some("Installed the included Cemu v9.9"));
        assert_eq!(steps[1].state, StepState::Done, "{steps:?}");
        assert_eq!(steps[2].state, StepState::Done, "{steps:?}");

        let cemu = d.join("data").join("Cemu");
        assert_eq!(std::fs::read(cemu.join("Cemu.exe")).unwrap(), b"MZ");
        assert!(cemu.join("resources/sharedFonts/x.ttf").is_file());
        assert!(cemu.join("portable/controllerProfiles/controller1.xml").is_file());
        assert!(cemu.join("portable/settings.xml").is_file());
        let license = std::fs::read_to_string(cemu.join("CEMU-LICENSE.txt")).unwrap();
        assert!(
            license.contains("https://github.com/cemu-project/Cemu (tag v9.9)"),
            "{license}"
        );
        assert!(license.contains("(test copy)"), "{license}");
        assert_eq!(
            std::fs::read_to_string(cemu.join(".wiiu-connect-version")).unwrap(),
            "v9.9"
        );
        assert!(
            !d.join("data/downloads/cemu-included.zip.part").exists(),
            "temporary copy removed"
        );
        let status = setup.status();
        assert!(status.cemu.is_some_and(|c| c.managed));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn launch_refuses_paths_that_are_not_scanned_games() {
        let d = temp("launch");
        let setup = Setup::new(
            SetupConfig {
                data_dir: d.clone(),
                auto: false,
            },
            "127.0.0.1:26760".parse().unwrap(),
        );
        assert!(matches!(setup.launch(None, false), Err(SetupError::NoCemu)));
        let _ = std::fs::remove_dir_all(&d);
    }
}
