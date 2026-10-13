//! Which window is Cemu's game window.

/// A visible top-level window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    pub handle: u64,
    pub pid: u32,
    pub title: String,
    /// File name of the process's executable, e.g. `Cemu.exe`.
    pub exe_name: String,
}

/// Cemu's main window: a `Cemu.exe` window whose title starts with `"Cemu "`. Cemu always titles it with
/// `BUILD_VERSION_WITH_NAME_STRING` ("Cemu " + version, then FPS and game; `src/Common/version.h`,
/// `src/gui/wxgui/wxWindowSystem.cpp`), while its GamePad window is the translated "GamePad View". Never by size.
pub fn choose_main_window(windows: &[WindowInfo]) -> Option<&WindowInfo> {
    windows
        .iter()
        .find(|w| w.exe_name.eq_ignore_ascii_case("cemu.exe") && w.title.starts_with("Cemu "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(handle: u64, title: &str, exe: &str) -> WindowInfo {
        WindowInfo {
            handle,
            pid: 42,
            title: title.into(),
            exe_name: exe.into(),
        }
    }

    #[test]
    fn picks_the_main_window_in_any_language() {
        for title in [
            "Cemu 2.6",
            "Cemu 2.6 [FPS: 59.94] [Mario Kart 8]",
            "Cemu 2.6 - FPS: 30.00 - Zelda: Breath of the Wild",
            "Cemu 2.6 [FPS: 60.00] [マリオカート8 デラックス]",
        ] {
            let windows = [
                win(1, "GamePad View - FPS: 60.00", "Cemu.exe"),
                win(2, title, "Cemu.exe"),
            ];
            assert_eq!(choose_main_window(&windows).map(|w| w.handle), Some(2), "{title}");
        }
    }

    #[test]
    fn never_picks_the_gamepad_window_or_other_programs() {
        for (title, exe) in [
            ("GamePad View", "Cemu.exe"),
            ("GamePad-Ansicht - FPS: 60.00", "Cemu.exe"),
            ("ゲームパッド表示", "Cemu.exe"),
            ("Cemu 2.6", "notepad.exe"),
            ("CemuHook settings", "Cemu.exe"),
            ("", "Cemu.exe"),
        ] {
            assert_eq!(choose_main_window(&[win(1, title, exe)]), None, "{title} / {exe}");
        }
    }

    #[test]
    fn the_exe_name_is_case_insensitive() {
        assert!(choose_main_window(&[win(1, "Cemu 2.6", "cemu.EXE")]).is_some());
    }
}
