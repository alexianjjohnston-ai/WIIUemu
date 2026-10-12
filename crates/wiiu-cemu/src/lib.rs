//! Cemu integration helpers.
//!
//! Cemu stores controller profiles as XML in `<cemu config dir>/controllerProfiles/`. The format here mirrors
//! `InputManager::save` / `InputManager::load` (src/input/InputManager.cpp, Cemu commit `aa20e2f`). Writing
//! a profile with the exact DSU bindings saves users from binding every button by hand.

use std::fs;
use std::io;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use wiiu_controller::WiimoteButton;
use wiiu_dsu::pad::buttons::cemu_button_id;

/// Cemu `Buttons2` ids for the left stick (src/input/api/Controller.h).
const CEMU_AXIS_XP: u64 = 38;
const CEMU_AXIS_YP: u64 = 39;
const CEMU_AXIS_XN: u64 = 44;
const CEMU_AXIS_YN: u64 = 45;

/// Cemu `WiimoteController::ButtonId` for the Nunchuk stick directions.
const NUNCHUK_UP: u64 = 13;
const NUNCHUK_DOWN: u64 = 14;
const NUNCHUK_LEFT: u64 = 15;
const NUNCHUK_RIGHT: u64 = 16;

/// `WPADDeviceType` values Cemu accepts in `<device_type>` (src/input/emulated/WPADController.h).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WiimoteExtension {
    /// Wii Remote alone (`kWAPDevCore`).
    None = 0,
    /// Wii Remote + Nunchuk (`kWAPDevFreestyle`).
    Nunchuk = 1,
    /// Wii Remote + MotionPlus (`kWAPDevMPLS`).
    MotionPlus = 5,
    /// MotionPlus + Nunchuk (`kWAPDevMPLSFreeStyle`).
    MotionPlusNunchuk = 6,
}

#[derive(Debug, Clone)]
pub struct WiimoteProfile {
    /// Profile name; also the file name (`<name>.xml`).
    pub name: String,
    /// DSU slot (1–3 for Wii Remotes, see docs/controller-protocol.md).
    pub dsu_slot: u8,
    pub dsu_ip: Ipv4Addr,
    pub dsu_port: u16,
    pub extension: WiimoteExtension,
    pub motion: bool,
}

impl WiimoteProfile {
    /// Default profile for Wii Remote player `player` (1–3).
    pub fn for_player(player: u8) -> Self {
        Self {
            name: format!("WiiUConnect_Wiimote{player}"),
            dsu_slot: player,
            dsu_ip: Ipv4Addr::LOCALHOST,
            dsu_port: wiiu_dsu::DEFAULT_PORT,
            extension: WiimoteExtension::None,
            motion: true,
        }
    }

    /// The `(cemu mapping id, cemu controller button id)` pairs for this profile.
    pub fn mappings(&self) -> Vec<(u64, u64)> {
        let mut m: Vec<(u64, u64)> = WiimoteButton::ALL
            .iter()
            .filter_map(|b| match b.dsu_mask() {
                wiiu_controller::buttons::DsuTarget::Bit(mask) => Some((b.cemu_mapping_id(), cemu_button_id(mask))),
                wiiu_controller::buttons::DsuTarget::TouchClick => None,
            })
            .collect();
        m.extend([
            (NUNCHUK_UP, CEMU_AXIS_YP),
            (NUNCHUK_DOWN, CEMU_AXIS_YN),
            (NUNCHUK_LEFT, CEMU_AXIS_XN),
            (NUNCHUK_RIGHT, CEMU_AXIS_XP),
        ]);
        m.sort_unstable();
        m
    }

    pub fn to_xml(&self) -> String {
        let mut entries = String::new();
        for (mapping, button) in self.mappings() {
            entries.push_str(&format!(
                "\t\t\t<entry>\n\t\t\t\t<mapping>{mapping}</mapping>\n\t\t\t\t<button>{button}</button>\n\t\t\t</entry>\n"
            ));
        }
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<emulated_controller>
	<type>Wiimote</type>
	<profile>{name}</profile>
	<device_type>{device_type}</device_type>
	<controller>
		<api>DSUController</api>
		<uuid>{slot}</uuid>
		<display_name>Controller {display}</display_name>
		<motion>{motion}</motion>
		<axis>
			<deadzone>0.15</deadzone>
			<range>1</range>
		</axis>
		<rotation>
			<deadzone>0.15</deadzone>
			<range>1</range>
		</rotation>
		<trigger>
			<deadzone>0.25</deadzone>
			<range>1</range>
		</trigger>
		<ip>{ip}</ip>
		<port>{port}</port>
		<mappings>
{entries}		</mappings>
	</controller>
</emulated_controller>
"#,
            name = xml_escape(&self.name),
            device_type = self.extension as u8,
            slot = self.dsu_slot,
            display = u16::from(self.dsu_slot) + 1,
            motion = self.motion,
            ip = self.dsu_ip,
            port = self.dsu_port,
        )
    }
}

fn xml_escape(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&apos;".to_string(),
            c => c.to_string(),
        })
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("profile name {0:?} must be 1-64 characters of [A-Za-z0-9_-]")]
    BadName(String),
    #[error("{0} already exists (use --force to overwrite)")]
    Exists(PathBuf),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Writes `profile` as `<dir>/<name>.xml`. Refuses to overwrite unless `force` is set, and never touches
/// Cemu's active per-player files (`controller0.xml` …).
pub fn write_profile(dir: &Path, profile: &WiimoteProfile, force: bool) -> Result<PathBuf, InstallError> {
    let name = &profile.name;
    let valid = !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        && !(name.starts_with("controller") && name[10..].bytes().all(|b| b.is_ascii_digit()));
    if !valid {
        return Err(InstallError::BadName(name.clone()));
    }
    fs::create_dir_all(dir)?;
    let path = dir.join(format!("{name}.xml"));
    if path.exists() && !force {
        return Err(InstallError::Exists(path));
    }
    fs::write(&path, profile.to_xml())?;
    Ok(path)
}

/// Best guess at Cemu's `controllerProfiles` directory. Cemu 2.x keeps config next to the executable in
/// portable mode, otherwise in the user config dir. Returns candidates in priority order.
pub fn profile_dir_candidates(cemu_exe_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(dir) = cemu_exe_dir {
        out.push(dir.join("portable").join("controllerProfiles"));
        out.push(dir.join("controllerProfiles"));
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        out.push(PathBuf::from(appdata).join("Cemu").join("controllerProfiles"));
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        out.push(PathBuf::from(xdg).join("Cemu").join("controllerProfiles"));
    } else if let Some(home) = std::env::var_os("HOME") {
        out.push(
            PathBuf::from(home)
                .join(".config")
                .join("Cemu")
                .join("controllerProfiles"),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text<'a>(node: roxmltree::Node<'a, 'a>, tag: &str) -> &'a str {
        node.children()
            .find(|n| n.has_tag_name(tag))
            .and_then(|n| n.text())
            .unwrap_or_else(|| panic!("missing <{tag}>"))
    }

    #[test]
    fn profile_matches_cemu_loader_schema() {
        let xml = WiimoteProfile::for_player(2).to_xml();
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let root = doc.root_element();
        assert_eq!(root.tag_name().name(), "emulated_controller");
        assert_eq!(text(root, "type"), "Wiimote"); // EmulatedController::type_from_string
        assert_eq!(text(root, "profile"), "WiiUConnect_Wiimote2");
        assert_eq!(text(root, "device_type"), "0");

        let c = root.children().find(|n| n.has_tag_name("controller")).unwrap();
        assert_eq!(text(c, "api"), "DSUController"); // InputAPI::to_string(DSUClient)
        assert_eq!(text(c, "uuid"), "2"); // ControllerFactory parses the DSU slot index from the uuid
        assert_eq!(text(c, "ip"), "127.0.0.1"); // DSUController::load
        assert_eq!(text(c, "port"), "26760");
        assert_eq!(text(c, "motion"), "true"); // ConvertString<bool> accepts "true"

        let entries: Vec<(u64, u64)> = c
            .descendants()
            .filter(|n| n.has_tag_name("entry"))
            .map(|e| (text(e, "mapping").parse().unwrap(), text(e, "button").parse().unwrap()))
            .collect();
        // A -> Cross(14), B -> Circle(13), HOME -> L3(1), Plus -> Options(3), Minus -> Share(0)
        for expected in [
            (1, 14),
            (2, 13),
            (17, 1),
            (7, 3),
            (8, 0),
            (9, 4),
            (10, 6),
            (11, 7),
            (12, 5),
        ] {
            assert!(
                entries.contains(&expected),
                "missing mapping {expected:?} in {entries:?}"
            );
        }
        assert!(entries.contains(&(NUNCHUK_UP, CEMU_AXIS_YP)));
        assert_eq!(entries.len(), WiimoteButton::ALL.len() + 4);
    }

    #[test]
    fn every_wiimote_button_is_bound_once() {
        let m = WiimoteProfile::for_player(1).mappings();
        let mut ids: Vec<u64> = m.iter().map(|(id, _)| *id).collect();
        ids.dedup();
        assert_eq!(ids.len(), m.len());
        let mut buttons: Vec<u64> = m.iter().map(|(_, b)| *b).collect();
        buttons.sort_unstable();
        buttons.dedup();
        assert_eq!(buttons.len(), m.len(), "two Wii Remote buttons share a DSU button");
    }

    #[test]
    fn extension_sets_device_type() {
        let mut p = WiimoteProfile::for_player(1);
        p.extension = WiimoteExtension::MotionPlusNunchuk;
        assert!(p.to_xml().contains("<device_type>6</device_type>"));
    }

    #[test]
    fn write_profile_is_safe() {
        let dir = std::env::temp_dir().join(format!("wiiu-cemu-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let p = WiimoteProfile::for_player(1);
        let path = write_profile(&dir, &p, false).unwrap();
        assert!(path.ends_with("WiiUConnect_Wiimote1.xml"));
        assert!(matches!(write_profile(&dir, &p, false), Err(InstallError::Exists(_))));
        write_profile(&dir, &p, true).unwrap();

        for bad in [
            "",
            "../evil",
            "a b",
            "controller0",
            "controller12",
            "x".repeat(65).as_str(),
        ] {
            let mut q = p.clone();
            q.name = bad.to_string();
            assert!(
                matches!(write_profile(&dir, &q, true), Err(InstallError::BadName(_))),
                "{bad}"
            );
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn escapes_xml() {
        assert_eq!(xml_escape("a<b>&\"'"), "a&lt;b&gt;&amp;&quot;&apos;");
    }
}
