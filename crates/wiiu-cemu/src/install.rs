//! Working with a Cemu installation on disk: finding it, assigning Wii U Connect's controllers, adding the
//! user's own game folders and listing the games in them.
//!
//! File formats and paths follow Cemu's source (commit `21878f1`):
//! - `CemuApp::DeterminePaths` (src/gui/wxgui/CemuApp.cpp): a `portable` folder next to the executable (on
//!   macOS next to `Cemu.app`) holds all config; otherwise the per-user folder.
//! - `InputManager::load` (src/input/InputManager.cpp): the active config of player N (0 = "Controller 1") is
//!   `controllerProfiles/controller{N}.xml`, in the same format as named profiles.
//! - `CemuConfig::Load` / `Save` (src/config/CemuConfig.cpp): `settings.xml` has a `<content>` root and game
//!   folders under `<GamePaths><Entry>…</Entry></GamePaths>`. Missing settings fall back to defaults, and a
//!   missing `settings.xml` makes Cemu show its first-start dialog.

use crate::{profile_dir_candidates_for, CemuProfile, GamepadProfile, Platform, WiimoteProfile};
use std::fs;
use std::io;
use std::net::SocketAddrV4;
use std::path::{Path, PathBuf};

/// A Cemu installation: its executable and the folder holding `settings.xml` and `controllerProfiles/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CemuInstall {
    /// Folder the user picked (contains the executable, or is/contains `Cemu.app`).
    pub dir: PathBuf,
    /// What to run. On macOS this is the `Cemu.app` bundle (start it with `open`).
    pub executable: PathBuf,
    pub config_dir: PathBuf,
    pub portable: bool,
}

impl CemuInstall {
    /// Finds Cemu in `dir` (or `dir` itself when it is `Cemu.app`). `env` reads environment variables.
    pub fn find(dir: &Path, platform: Platform, env: &dyn Fn(&str) -> Option<std::ffi::OsString>) -> Option<Self> {
        let executable = find_executable(dir, platform)?;
        let base = match platform {
            Platform::MacOs => executable.parent().unwrap_or(dir).to_path_buf(), // folder containing Cemu.app
            _ => executable.parent()?.to_path_buf(),
        };
        let portable_dir = base.join("portable");
        let (config_dir, portable) = if portable_dir.is_dir() {
            (portable_dir, true)
        } else {
            let profiles = profile_dir_candidates_for(platform, None, env).into_iter().next()?;
            (profiles.parent()?.to_path_buf(), false)
        };
        Some(Self {
            dir: dir.to_path_buf(),
            executable,
            config_dir,
            portable,
        })
    }

    pub fn profiles_dir(&self) -> PathBuf {
        self.config_dir.join("controllerProfiles")
    }

    pub fn settings_path(&self) -> PathBuf {
        self.config_dir.join("settings.xml")
    }
}

fn find_executable(dir: &Path, platform: Platform) -> Option<PathBuf> {
    match platform {
        Platform::Windows => [dir.join("Cemu.exe"), dir.join("cemu.exe")]
            .into_iter()
            .find(|p| p.is_file()),
        Platform::MacOs => {
            if dir.extension().is_some_and(|e| e == "app") && dir.join("Contents/MacOS").is_dir() {
                return Some(dir.to_path_buf());
            }
            let app = dir.join("Cemu.app");
            app.join("Contents/MacOS").is_dir().then_some(app)
        }
        Platform::Linux => {
            let plain = dir.join("Cemu");
            if plain.is_file() {
                return Some(plain);
            }
            fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| {
                p.is_file()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("Cemu") && n.ends_with(".AppImage"))
            })
        }
    }
}

/// What [`assign_controllers`] did, for showing to the user.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct AssignReport {
    pub written: Vec<PathBuf>,
    /// Existing active configs that were different and got backed up first.
    pub backups: Vec<PathBuf>,
}

/// The profile Wii U Connect puts on each of Cemu's controllers, all pointing at the DSU server on `dsu`: index 0
/// is Controller 1 (the GamePad, DSU slot 0), indices 1–3 are Controllers 2–4 (Wii Remotes for players 1–3, DSU
/// slots 1–3).
fn controller_profiles(dsu: SocketAddrV4) -> [(String, String); 4] {
    let gamepad = GamepadProfile {
        dsu_ip: *dsu.ip(),
        dsu_port: dsu.port(),
        ..GamepadProfile::default()
    };
    let wiimote = |player: u8| {
        let p = WiimoteProfile {
            dsu_ip: *dsu.ip(),
            dsu_port: dsu.port(),
            ..WiimoteProfile::for_player(player)
        };
        (p.name().to_owned(), p.to_xml())
    };
    [
        (gamepad.name().to_owned(), gamepad.to_xml()),
        wiimote(1),
        wiimote(2),
        wiimote(3),
    ]
}

/// Writes the `WiiUConnect_GamePad` and `WiiUConnect_Wiimote{1,2,3}` profiles and makes them the active configs of
/// Cemu's Controllers 1–4 (`controller0.xml` … `controller3.xml`). Any different existing active config is first
/// copied to `controllerN.xml.before-wiiu-connect` (once; later runs keep the oldest backup).
pub fn assign_controllers(install: &CemuInstall, dsu: SocketAddrV4) -> io::Result<AssignReport> {
    let dir = install.profiles_dir();
    fs::create_dir_all(&dir)?;
    let mut report = AssignReport::default();
    for (index, (name, xml)) in controller_profiles(dsu).into_iter().enumerate() {
        let named = dir.join(format!("{name}.xml"));
        fs::write(&named, &xml)?;
        report.written.push(named);

        let active = dir.join(format!("controller{index}.xml"));
        if let Ok(existing) = fs::read(&active) {
            let backup = dir.join(format!("controller{index}.xml.before-wiiu-connect"));
            if existing != xml.as_bytes() && !backup.exists() {
                fs::write(&backup, &existing)?;
                report.backups.push(backup);
            }
        }
        fs::write(&active, &xml)?;
        report.written.push(active);
    }
    Ok(report)
}

/// True when Controllers 1–4 already use Wii U Connect's profiles for the DSU server on `dsu`.
pub fn controllers_assigned(install: &CemuInstall, dsu: SocketAddrV4) -> bool {
    controller_profiles(dsu)
        .iter()
        .enumerate()
        .all(|(index, (_, expected))| {
            fs::read_to_string(install.profiles_dir().join(format!("controller{index}.xml")))
                .is_ok_and(|s| s == *expected)
        })
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("settings.xml is not valid XML: {0}")]
    Parse(String),
    #[error("game folder path is not valid UTF-8")]
    NotUtf8,
}

/// The game folders listed in Cemu's `settings.xml` (empty when it doesn't exist yet).
pub fn game_paths(install: &CemuInstall) -> Result<Vec<String>, SettingsError> {
    let path = install.settings_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let root = load_settings(&path)?;
    Ok(root
        .get_child("GamePaths")
        .map(|gp| {
            gp.children
                .iter()
                .filter_map(|c| c.as_element())
                .filter(|e| e.name == "Entry")
                .filter_map(|e| e.get_text().map(|t| t.into_owned()))
                .collect()
        })
        .unwrap_or_default())
}

fn load_settings(path: &Path) -> Result<xmltree::Element, SettingsError> {
    let data = fs::read(path)?;
    xmltree::Element::parse(data.as_slice()).map_err(|e| SettingsError::Parse(e.to_string()))
}

/// Adds `folder` to Cemu's game folders. Creates a minimal `settings.xml` if there is none (Cemu fills in every
/// other setting with its defaults). Returns `false` if the folder was already listed.
pub fn add_game_path(install: &CemuInstall, folder: &Path) -> Result<bool, SettingsError> {
    let folder = folder.to_str().ok_or(SettingsError::NotUtf8)?.to_string();
    edit_settings(install, |root| {
        if root.get_child("GamePaths").is_none() {
            root.children
                .push(xmltree::XMLNode::Element(xmltree::Element::new("GamePaths")));
        }
        let game_paths = root.get_mut_child("GamePaths").expect("just ensured");
        let exists = game_paths
            .children
            .iter()
            .filter_map(|c| c.as_element())
            .any(|e| e.name == "Entry" && e.get_text().is_some_and(|t| t == folder));
        if exists {
            return false;
        }
        let mut entry = xmltree::Element::new("Entry");
        entry.children.push(xmltree::XMLNode::Text(folder));
        game_paths.children.push(xmltree::XMLNode::Element(entry));
        true
    })
}

/// Makes Cemu open its separate "GamePad View" window on start (`<open_pad>` under `<content>`, the GUI setting
/// behind *View → Separate GamePad view*; Cemu commit `21878f1`, `wxCemuConfig.cpp`). The phone GamePad shows
/// that window's picture. Returns whether `settings.xml` changed.
pub fn enable_gamepad_window(install: &CemuInstall) -> Result<bool, SettingsError> {
    edit_settings(install, |root| {
        if root
            .get_child("open_pad")
            .and_then(|e| e.get_text())
            .is_some_and(|t| t.trim() == "true")
        {
            return false;
        }
        root.children
            .retain(|c| c.as_element().is_none_or(|e| e.name != "open_pad"));
        let mut e = xmltree::Element::new("open_pad");
        e.children.push(xmltree::XMLNode::Text("true".into()));
        root.children.push(xmltree::XMLNode::Element(e));
        true
    })
}

/// Applies `edit` to Cemu's `settings.xml` (creating a minimal one when missing) and writes it back atomically if
/// `edit` reports a change. A file that isn't valid XML is never touched.
fn edit_settings(
    install: &CemuInstall,
    edit: impl FnOnce(&mut xmltree::Element) -> bool,
) -> Result<bool, SettingsError> {
    let path = install.settings_path();
    let mut root = if path.exists() {
        load_settings(&path)?
    } else {
        xmltree::Element::new("content")
    };
    if !edit(&mut root) {
        return Ok(false);
    }
    fs::create_dir_all(&install.config_dir)?;
    let mut out = Vec::new();
    root.write_with_config(
        &mut out,
        xmltree::EmitterConfig::new().perform_indent(true).indent_string("    "),
    )
    .map_err(|e| SettingsError::Parse(e.to_string()))?;
    let tmp = path.with_extension("xml.wiiu-connect-tmp");
    fs::write(&tmp, out)?;
    fs::rename(&tmp, &path)?;
    Ok(true)
}

/// Where Cemu's own installer puts it, so an installed Cemu is found without asking. Windows: `%LOCALAPPDATA%\Cemu`
/// (`InstallDir` in Cemu's `src/resource/installer.nsi`, commit `21878f1`). macOS: `/Applications` (`Cemu.app`).
pub fn installed_cemu_dirs(platform: Platform, env: &dyn Fn(&str) -> Option<std::ffi::OsString>) -> Vec<PathBuf> {
    match platform {
        Platform::Windows => env("LOCALAPPDATA")
            .map(|d| PathBuf::from(d).join("Cemu"))
            .into_iter()
            .collect(),
        Platform::MacOs => vec![PathBuf::from("/Applications")],
        Platform::Linux => Vec::new(),
    }
}

/// Folders worth searching for the user's own game dumps: the usual user folders, plus top-level folders on other
/// drives whose names suggest games (Windows). Only names are read while searching.
pub fn game_search_roots(platform: Platform, env: &dyn Fn(&str) -> Option<std::ffi::OsString>) -> Vec<PathBuf> {
    let home = match platform {
        Platform::Windows => env("USERPROFILE"),
        _ => env("HOME"),
    }
    .map(PathBuf::from);
    let mut roots: Vec<PathBuf> = home
        .iter()
        .flat_map(|h| ["Downloads", "Documents", "Desktop", "Games", "Videos"].map(|d| h.join(d)))
        .collect();
    if platform == Platform::Windows {
        let looks_like_games = |name: &str| {
            let n = name.to_ascii_lowercase().replace([' ', '-', '_'], "");
            ["wiiu", "games", "roms", "cemu", "emulat", "dumps"]
                .iter()
                .any(|k| n.contains(k))
        };
        for letter in b'C'..=b'Z' {
            let drive = PathBuf::from(format!("{}:\\", letter as char));
            let Ok(entries) = fs::read_dir(&drive) else { continue };
            roots.extend(
                entries
                    .flatten()
                    .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                    .filter(|e| e.file_name().to_str().is_some_and(looks_like_games))
                    .map(|e| e.path()),
            );
        }
    }
    roots.retain(|r| r.is_dir());
    roots
}

/// The folders holding the games found under `roots`: the folder that contains each game file or unpacked game,
/// without folders nested inside another found folder (Cemu searches game folders recursively).
pub fn discover_game_folders(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = roots
        .iter()
        .flat_map(|r| scan_games(r))
        .filter_map(|g| match g.kind {
            // <folder>/<game>/code/<title>.rpx
            "folder" => g.path.ancestors().nth(3).map(Path::to_path_buf),
            _ => g.path.parent().map(Path::to_path_buf),
        })
        .collect();
    folders.sort();
    folders.dedup();
    let all = folders.clone();
    folders.retain(|f| !all.iter().any(|other| other != f && f.starts_with(other)));
    folders
}

/// A game found in one of the user's game folders.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct GameEntry {
    pub name: String,
    /// What to pass to `Cemu -g`.
    pub path: PathBuf,
    /// `wua`, `wud`, `wux`, `iso` or `folder` (an unpacked game: `code/*.rpx` with `meta/meta.xml`).
    pub kind: &'static str,
}

const MAX_SCAN_DEPTH: usize = 4;
const MAX_SCAN_ENTRIES: usize = 20_000;

/// Lists games in `folder` (searched up to a few levels deep). Only the formats Cemu opens directly are listed;
/// nothing is read beyond file names and `meta/meta.xml`.
pub fn scan_games(folder: &Path) -> Vec<GameEntry> {
    let mut out = Vec::new();
    let mut budget = MAX_SCAN_ENTRIES;
    scan(folder, 0, &mut out, &mut budget);
    out.sort_by_key(|g| g.name.to_lowercase());
    out
}

fn scan(dir: &Path, depth: usize, out: &mut Vec<GameEntry>, budget: &mut usize) {
    // An unpacked game folder: <game>/code/*.rpx + <game>/meta/meta.xml.
    if let Some(rpx) = unpacked_rpx(dir) {
        let name = meta_name(&dir.join("meta").join("meta.xml"))
            .or_else(|| dir.file_name().and_then(|n| n.to_str()).map(str::to_string))
            .unwrap_or_else(|| "Unknown game".into());
        out.push(GameEntry {
            name,
            path: rpx,
            kind: "folder",
        });
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            if depth < MAX_SCAN_DEPTH {
                scan(&path, depth + 1, out, budget);
            }
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
        let kind = match ext.as_deref() {
            Some("wua") => "wua",
            Some("wud") => "wud",
            Some("wux") => "wux",
            Some("iso") => "iso",
            _ => continue,
        };
        let name = path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown game")
            .to_string();
        out.push(GameEntry { name, path, kind });
    }
}

fn unpacked_rpx(dir: &Path) -> Option<PathBuf> {
    let code = dir.join("code");
    if !dir.join("meta").join("meta.xml").is_file() {
        return None;
    }
    fs::read_dir(code).ok()?.flatten().map(|e| e.path()).find(|p| {
        p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("rpx"))
    })
}

/// English long name from a title's `meta/meta.xml` (`<longname_en>`), first line only.
fn meta_name(path: &Path) -> Option<String> {
    let data = fs::read(path).ok()?;
    if data.len() > 1 << 20 {
        return None;
    }
    let root = xmltree::Element::parse(data.as_slice()).ok()?;
    let name = root.get_child("longname_en")?.get_text()?.into_owned();
    let name = name
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT_DSU: SocketAddrV4 = SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, wiiu_dsu::DEFAULT_PORT);

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir().join(format!(
                "wiiu-cemu-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn no_env(_: &str) -> Option<std::ffi::OsString> {
        None
    }

    fn portable_windows_install(t: &TempDir) -> CemuInstall {
        fs::write(t.0.join("Cemu.exe"), b"MZ").unwrap();
        fs::create_dir_all(t.0.join("portable")).unwrap();
        CemuInstall::find(&t.0, Platform::Windows, &no_env).unwrap()
    }

    #[test]
    fn gamepad_window_setting_is_added_once_and_keeps_other_settings() {
        let t = TempDir::new("openpad");
        let i = portable_windows_install(&t);
        // No settings.xml yet: a minimal one is created (Cemu then also skips its first-run dialog).
        assert!(enable_gamepad_window(&i).unwrap());
        assert!(!enable_gamepad_window(&i).unwrap(), "second call changes nothing");
        assert!(add_game_path(&i, Path::new("D:/Games")).unwrap());
        // A user who turned it off again in Cemu gets it back on, other settings untouched.
        let xml = fs::read_to_string(i.settings_path())
            .unwrap()
            .replace(">true<", ">false<");
        fs::write(i.settings_path(), xml).unwrap();
        assert!(enable_gamepad_window(&i).unwrap());
        let xml = fs::read_to_string(i.settings_path()).unwrap();
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let root = doc.root_element();
        assert_eq!(root.tag_name().name(), "content");
        let pads: Vec<_> = root.children().filter(|n| n.has_tag_name("open_pad")).collect();
        assert_eq!(pads.len(), 1);
        assert_eq!(pads[0].text(), Some("true"));
        assert_eq!(game_paths(&i).unwrap(), vec!["D:/Games"]);
    }

    #[test]
    fn discovers_the_folders_that_hold_games() {
        let t = TempDir::new("discover");
        let downloads = t.0.join("Downloads");
        let wiiu = downloads.join("Wii U");
        fs::create_dir_all(wiiu.join("Mario Kart 8 [AMKP01]/code")).unwrap();
        fs::create_dir_all(wiiu.join("Mario Kart 8 [AMKP01]/meta")).unwrap();
        fs::write(wiiu.join("Mario Kart 8 [AMKP01]/code/Turbo.rpx"), b"").unwrap();
        fs::write(wiiu.join("Mario Kart 8 [AMKP01]/meta/meta.xml"), "<menu/>").unwrap();
        fs::write(wiiu.join("Splatoon.wua"), b"").unwrap();
        fs::create_dir_all(wiiu.join("more")).unwrap();
        fs::write(wiiu.join("more/Pikmin 3.wux"), b"").unwrap();
        fs::write(downloads.join("holiday.iso.txt"), b"").unwrap();
        let docs = t.0.join("Documents");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("notes.txt"), b"").unwrap();
        // "more" is inside "Wii U" (Cemu searches recursively), so only "Wii U" is added.
        assert_eq!(discover_game_folders(&[downloads, docs]), vec![wiiu]);
    }

    #[test]
    fn search_roots_are_the_usual_user_folders() {
        let t = TempDir::new("roots");
        fs::create_dir_all(t.0.join("Downloads")).unwrap();
        fs::create_dir_all(t.0.join("Games")).unwrap();
        let home = t.0.clone();
        let env = move |k: &str| (k == "HOME").then(|| home.clone().into_os_string());
        assert_eq!(
            game_search_roots(Platform::Linux, &env),
            vec![t.0.join("Downloads"), t.0.join("Games")],
            "missing folders are left out"
        );
    }

    #[test]
    fn installer_location_is_localappdata_on_windows() {
        let env = |k: &str| (k == "LOCALAPPDATA").then(|| std::ffi::OsString::from("C:/Users/me/AppData/Local"));
        assert_eq!(
            installed_cemu_dirs(Platform::Windows, &env),
            vec![PathBuf::from("C:/Users/me/AppData/Local").join("Cemu")]
        );
        assert!(installed_cemu_dirs(Platform::Linux, &env).is_empty());
    }

    #[test]
    fn finds_portable_windows_install() {
        let t = TempDir::new("find");
        assert_eq!(CemuInstall::find(&t.0, Platform::Windows, &no_env), None);
        let i = portable_windows_install(&t);
        assert!(i.portable);
        assert_eq!(i.config_dir, t.0.join("portable"));
        assert_eq!(i.executable, t.0.join("Cemu.exe"));
    }

    #[test]
    fn non_portable_install_uses_the_user_config_folder() {
        let t = TempDir::new("userdir");
        fs::write(t.0.join("Cemu.exe"), b"MZ").unwrap();
        let env = |k: &str| (k == "APPDATA").then(|| std::ffi::OsString::from("/users/me/AppData/Roaming"));
        let i = CemuInstall::find(&t.0, Platform::Windows, &env).unwrap();
        assert!(!i.portable);
        assert_eq!(i.config_dir, PathBuf::from("/users/me/AppData/Roaming/Cemu"));
    }

    #[test]
    fn finds_mac_app_bundle() {
        let t = TempDir::new("mac");
        fs::create_dir_all(t.0.join("Cemu.app/Contents/MacOS")).unwrap();
        fs::create_dir_all(t.0.join("portable")).unwrap();
        let i = CemuInstall::find(&t.0, Platform::MacOs, &no_env).unwrap();
        assert_eq!(i.executable, t.0.join("Cemu.app"));
        assert_eq!(i.config_dir, t.0.join("portable"));
        let via_bundle = CemuInstall::find(&t.0.join("Cemu.app"), Platform::MacOs, &no_env).unwrap();
        assert_eq!(via_bundle.config_dir, t.0.join("portable"));
    }

    #[test]
    fn assigning_controllers_backs_up_once_and_is_detected() {
        let t = TempDir::new("assign");
        let i = portable_windows_install(&t);
        let dir = i.profiles_dir();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("controller1.xml"), "<user's own config/>").unwrap();
        fs::write(dir.join("controller0.xml"), "<gamepad config/>").unwrap();
        assert!(!controllers_assigned(&i, DEFAULT_DSU));

        let r = assign_controllers(&i, DEFAULT_DSU).unwrap();
        assert_eq!(
            r.backups,
            vec![
                dir.join("controller0.xml.before-wiiu-connect"),
                dir.join("controller1.xml.before-wiiu-connect")
            ]
        );
        assert_eq!(
            fs::read_to_string(dir.join("controller0.xml.before-wiiu-connect")).unwrap(),
            "<gamepad config/>"
        );
        assert_eq!(
            fs::read_to_string(dir.join("controller0.xml")).unwrap(),
            GamepadProfile::default().to_xml(),
            "Controller 1 is the phone GamePad"
        );
        assert_eq!(
            fs::read_to_string(dir.join("controller1.xml")).unwrap(),
            WiimoteProfile::for_player(1).to_xml()
        );
        assert!(dir.join("WiiUConnect_GamePad.xml").is_file());
        assert!(controllers_assigned(&i, DEFAULT_DSU));
        // Running again changes nothing and keeps the original backup.
        let again = assign_controllers(&i, DEFAULT_DSU).unwrap();
        assert!(again.backups.is_empty());
        assert_eq!(
            fs::read_to_string(dir.join("controller1.xml.before-wiiu-connect")).unwrap(),
            "<user's own config/>"
        );
    }

    #[test]
    fn assigned_profiles_follow_the_dsu_endpoint() {
        let t = TempDir::new("endpoint");
        let i = portable_windows_install(&t);
        let custom = SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, 12345);
        assign_controllers(&i, custom).unwrap();
        let xml = fs::read_to_string(i.profiles_dir().join("controller1.xml")).unwrap();
        assert!(xml.contains("<port>12345</port>"), "{xml}");
        assert!(controllers_assigned(&i, custom));
        assert!(
            !controllers_assigned(&i, DEFAULT_DSU),
            "a server on another port is not set up"
        );
    }

    #[test]
    fn add_game_path_creates_minimal_settings_then_appends_without_duplicates() {
        let t = TempDir::new("settings");
        let i = portable_windows_install(&t);
        assert!(add_game_path(&i, Path::new("D:/Wii U games")).unwrap());
        let xml = fs::read_to_string(i.settings_path()).unwrap();
        let doc = roxmltree::Document::parse(&xml).unwrap();
        assert_eq!(doc.root_element().tag_name().name(), "content");
        assert!(!add_game_path(&i, Path::new("D:/Wii U games")).unwrap(), "no duplicate");
        assert!(add_game_path(&i, Path::new("E:/More")).unwrap());
        assert_eq!(game_paths(&i).unwrap(), vec!["D:/Wii U games", "E:/More"]);
    }

    #[test]
    fn add_game_path_keeps_existing_cemu_settings() {
        let t = TempDir::new("existing");
        let i = portable_windows_install(&t);
        fs::write(
            i.settings_path(),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<content>
    <logflag>0</logflag>
    <mlc_path>C:/mlc01</mlc_path>
    <GamePaths>
        <Entry>C:/Games</Entry>
    </GamePaths>
    <GameCache/>
    <Graphic><api>1</api></Graphic>
</content>
"#,
        )
        .unwrap();
        assert!(add_game_path(&i, Path::new("D:/New")).unwrap());
        let xml = fs::read_to_string(i.settings_path()).unwrap();
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let root = doc.root_element();
        let child = |n: &str| root.children().find(|c| c.has_tag_name(n)).unwrap();
        assert_eq!(child("mlc_path").text(), Some("C:/mlc01"));
        assert_eq!(
            child("Graphic")
                .children()
                .find(|c| c.has_tag_name("api"))
                .unwrap()
                .text(),
            Some("1")
        );
        assert_eq!(game_paths(&i).unwrap(), vec!["C:/Games", "D:/New"]);
    }

    #[test]
    fn broken_settings_are_reported_not_overwritten() {
        let t = TempDir::new("broken");
        let i = portable_windows_install(&t);
        fs::write(i.settings_path(), "<content><GamePaths>").unwrap();
        assert!(matches!(
            add_game_path(&i, Path::new("D:/x")),
            Err(SettingsError::Parse(_))
        ));
        assert_eq!(fs::read_to_string(i.settings_path()).unwrap(), "<content><GamePaths>");
    }

    #[test]
    fn scans_packed_and_unpacked_games() {
        let t = TempDir::new("scan");
        let g = &t.0;
        fs::create_dir_all(g.join("Packed")).unwrap();
        fs::write(g.join("Packed/Some Game.wua"), b"").unwrap();
        fs::write(g.join("disc.WUX"), b"").unwrap();
        fs::write(g.join("notes.txt"), b"").unwrap();
        let unpacked = g.join("Unpacked/My Title");
        fs::create_dir_all(unpacked.join("code")).unwrap();
        fs::create_dir_all(unpacked.join("meta")).unwrap();
        fs::write(unpacked.join("code/game.rpx"), b"").unwrap();
        fs::write(
            unpacked.join("meta/meta.xml"),
            "<?xml version=\"1.0\"?><menu><longname_en type=\"string\">My Great\nTitle</longname_en></menu>",
        )
        .unwrap();
        let games = scan_games(g);
        let summary: Vec<(&str, &str)> = games.iter().map(|g| (g.name.as_str(), g.kind)).collect();
        assert_eq!(
            summary,
            vec![("disc", "wux"), ("My Great Title", "folder"), ("Some Game", "wua")]
        );
        assert_eq!(games[1].path, unpacked.join("code/game.rpx"));
    }
}
