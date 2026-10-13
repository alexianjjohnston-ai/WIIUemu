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

/// Cemu's separate GamePad window, belonging to process `pid`. Cemu creates it titled with the translated
/// `"GamePad View"` (`src/gui/wxgui/PadViewFrame.cpp`) and retitles it `"<GamePad View> - FPS: <n>"` while a game
/// runs (`src/gui/wxgui/wxWindowSystem.cpp`, Cemu main `2c212b5`). So: not the main window, and either the English
/// title or an FPS suffix (any language while a game runs).
pub fn choose_gamepad_window(windows: &[WindowInfo], pid: u32) -> Option<&WindowInfo> {
    windows.iter().find(|w| {
        w.pid == pid
            && w.exe_name.eq_ignore_ascii_case("cemu.exe")
            && !w.title.starts_with("Cemu ")
            && (w.title == "GamePad View" || w.title.starts_with("GamePad View - ") || w.title.contains(" - FPS: "))
    })
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

    #[test]
    fn finds_the_gamepad_window_of_the_same_cemu_in_any_language_while_a_game_runs() {
        let main = win(
            2,
            "Cemu 2.6 - FPS: 59.94 Vulkan [TitleId: 00050000-101c9400] Zelda",
            "Cemu.exe",
        );
        for title in [
            "GamePad View",
            "GamePad View - FPS: 59.94",
            "GamePad-Ansicht - FPS: 59.94",
        ] {
            let windows = [main.clone(), win(7, title, "Cemu.exe")];
            assert_eq!(
                choose_gamepad_window(&windows, 42).map(|w| w.handle),
                Some(7),
                "{title}"
            );
        }
        for (title, exe, pid) in [
            ("General settings", "Cemu.exe", 42),
            ("GamePad View", "notepad.exe", 42),
            ("GamePad View", "Cemu.exe", 43), // another Cemu
        ] {
            let other = WindowInfo {
                pid,
                ..win(7, title, exe)
            };
            assert!(
                choose_gamepad_window(&[main.clone(), other], 42).is_none(),
                "{title} {exe} {pid}"
            );
        }
    }
}
