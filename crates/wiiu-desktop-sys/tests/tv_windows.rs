//! The TV border's Win32 layer against a real window (Windows CI).
#![cfg(windows)]

use wiiu_desktop_sys::test_support::{menu_of, style_of, window_rect, StandIn};
use wiiu_desktop_sys::tv;

const WS_CAPTION: i64 = 0x00C0_0000;
const WS_THICKFRAME: i64 = 0x0004_0000;

/// Window tests share the desktop: run them one at a time.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn lists_windows_with_their_process() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    tv::make_dpi_aware();
    let w = StandIn::new("Cemu 2.6 stand-in", [100, 100, 900, 700]);
    let listed = tv::top_level_windows();
    let me = listed.iter().find(|t| t.handle == w.handle()).expect("stand-in listed");
    assert_eq!(me.pid, std::process::id());
    assert_eq!(me.title, "Cemu 2.6 stand-in");
    assert!(me.exe_path.to_ascii_lowercase().ends_with(".exe"), "{}", me.exe_path);
    assert_eq!(tv::window_owner_pid(w.handle()), Some(std::process::id()));
    let start = tv::process_start_time(std::process::id()).expect("start time");
    assert_eq!(tv::process_start_time(std::process::id()), Some(start));
}

#[test]
fn places_the_client_area_and_restores_everything() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    tv::make_dpi_aware();
    let w = StandIn::new("Cemu 2.6 stand-in", [120, 120, 920, 720]);
    let h = w.handle();
    let before_rect = window_rect(h);
    let saved = tv::save_window(h).expect("saved");
    assert_ne!(saved.menu, 0, "the stand-in has a menu");
    assert_ne!(saved.style & WS_CAPTION, 0);

    let monitor = tv::monitor_rect(h).expect("monitor");
    let client = [
        monitor[0] + 50,
        monitor[1] + 40,
        monitor[0] + 50 + 640,
        monitor[1] + 40 + 360,
    ];
    assert!(
        tv::place_client(h, client),
        "client area {:?} != {client:?}",
        tv::client_rect(h)
    );
    assert_eq!(tv::client_rect(h), Some(client));
    assert_eq!(style_of(h) & (WS_CAPTION | WS_THICKFRAME), 0, "frame removed");
    assert_eq!(menu_of(h), 0, "menu hidden");
    // Placing again (the 1 s keep-in-place check) is stable.
    assert!(tv::place_client(h, client));

    assert!(tv::restore_window(h, &saved));
    assert_eq!(style_of(h), saved.style);
    assert_eq!(menu_of(h), saved.menu);
    assert_eq!(window_rect(h), before_rect);
}

#[test]
fn the_border_window_opens_on_the_monitor_and_closes() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    tv::make_dpi_aware();
    let w = StandIn::new("Cemu 2.6 stand-in", [100, 100, 900, 700]);
    let monitor = tv::monitor_rect(w.handle()).expect("monitor");
    let border = tv::BorderWindow::open(monitor, vec![[10, 10, 50, 50]]).expect("border window");
    let handle = border.handle();
    assert_eq!(window_rect(handle), monitor);
    assert!(tv::window_owner_pid(handle).is_some());
    drop(border);
    assert_eq!(tv::window_owner_pid(handle), None, "closed");
}

#[test]
fn gone_windows_are_reported_as_gone() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let w = StandIn::new("Cemu 2.6 stand-in", [100, 100, 900, 700]);
    let h = w.handle();
    let saved = tv::save_window(h).unwrap();
    drop(w);
    assert_eq!(tv::window_owner_pid(h), None);
    assert_eq!(tv::save_window(h), None);
    assert!(!tv::place_client(h, [0, 0, 100, 100]));
    assert!(!tv::restore_window(h, &saved));
}
