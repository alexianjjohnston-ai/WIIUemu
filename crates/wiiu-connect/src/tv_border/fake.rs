//! A fake desktop for testing the driver and its thread on every platform.

use super::cemu::WindowInfo;
use super::driver::Desktop;
use super::recovery::{SavedWindow, RECORD_FILE};
use std::path::PathBuf;

pub const CEMU: u64 = 0xC0;
pub const MONITOR: [i32; 4] = [1920, 0, 3840, 1080]; // a second display, right of the first

#[derive(Default)]
pub struct Fake {
    pub windows: Vec<WindowInfo>,
    pub record: PathBuf,
    pub placed: Vec<[i32; 4]>,
    /// The record existed every time `place` ran.
    pub record_before_place: bool,
    pub restored: Vec<SavedWindow>,
    pub border: Option<([i32; 4], usize)>,
    pub borders_shown: usize,
}

pub fn saved_state() -> SavedWindow {
    SavedWindow {
        handle: CEMU,
        pid: 4000,
        process_start: 77,
        style: 0x14CF_0000,
        ex_style: 0x100,
        menu: 0x99,
        placement: [2000, 100, 3280, 820],
        show_cmd: 1,
    }
}

impl Fake {
    pub fn with_cemu(record: PathBuf) -> Self {
        Self {
            windows: vec![
                WindowInfo {
                    handle: 0x10,
                    pid: 4000,
                    title: "GamePad View".into(),
                    exe_name: "Cemu.exe".into(),
                },
                WindowInfo {
                    handle: CEMU,
                    pid: 4000,
                    title: "Cemu 2.6".into(),
                    exe_name: "Cemu.exe".into(),
                },
            ],
            record,
            record_before_place: true,
            ..Self::default()
        }
    }
}

impl Desktop for Fake {
    fn windows(&self) -> Vec<WindowInfo> {
        self.windows.clone()
    }
    fn process(&self, pid: u32) -> Option<(u32, u64)> {
        self.windows.iter().any(|w| w.pid == pid).then_some((pid, 77))
    }
    fn owner(&self, handle: u64) -> Option<u32> {
        self.windows.iter().find(|w| w.handle == handle).map(|w| w.pid)
    }
    fn monitor(&self, handle: u64) -> Option<[i32; 4]> {
        self.owner(handle).map(|_| MONITOR)
    }
    fn save(&self, handle: u64) -> Option<SavedWindow> {
        (handle == CEMU && self.owner(handle).is_some()).then(saved_state)
    }
    fn place(&mut self, _handle: u64, client: [i32; 4]) -> bool {
        self.record_before_place &= self.record.exists();
        self.placed.push(client);
        true
    }
    fn restore(&mut self, saved: &SavedWindow) -> bool {
        self.restored.push(saved.clone());
        true
    }
    fn show_border(&mut self, monitor: [i32; 4], white: Vec<[i32; 4]>) -> bool {
        self.border = Some((monitor, white.len()));
        self.borders_shown += 1;
        true
    }
    fn hide_border(&mut self) {
        self.border = None;
    }
}

pub fn record_path(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("wiiu-tv-driver-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(RECORD_FILE)
}
