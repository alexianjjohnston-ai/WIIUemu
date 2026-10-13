//! The record that lets the next start repair Cemu's window if Wii U Connect was killed while the border showed.

use serde::{Deserialize, Serialize};
use std::path::Path;

pub const RECORD_FILE: &str = "tv-border-restore.json";

/// Cemu's window as it was before the border changed it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedWindow {
    pub handle: u64,
    pub pid: u32,
    /// Process creation time (Windows FILETIME), so a reused pid isn't mistaken for the same Cemu.
    pub process_start: u64,
    pub style: i64,
    pub ex_style: i64,
    /// The menu handle; `SetMenu(hwnd, null)` detaches it without destroying it.
    pub menu: u64,
    /// Normal (restored) position: left, top, right, bottom.
    pub placement: [i32; 4],
    /// `SW_*` show state (normal, maximized, …).
    pub show_cmd: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovery {
    Restore,
    Discard,
}

/// `alive`: pid and start time of the running process with `saved.pid`, if any. `window_owner`: the pid that owns
/// `saved.handle` now, if the window still exists.
pub fn recovery(saved: &SavedWindow, alive: Option<(u32, u64)>, window_owner: Option<u32>) -> Recovery {
    let same_process = alive == Some((saved.pid, saved.process_start));
    if same_process && window_owner == Some(saved.pid) {
        Recovery::Restore
    } else {
        Recovery::Discard
    }
}

pub fn read_record(path: &Path) -> Option<SavedWindow> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

pub fn write_record(path: &Path, saved: &SavedWindow) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(saved).expect("serializes"))
}

pub fn delete_record(path: &Path) {
    if let Err(e) = std::fs::remove_file(path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!("couldn't delete {}: {e}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved() -> SavedWindow {
        SavedWindow {
            handle: 0x1234,
            pid: 4000,
            process_start: 133_000_000_000,
            style: 0x14CF_0000,
            ex_style: 0x100,
            menu: 0x99,
            placement: [100, 100, 1380, 820],
            show_cmd: 1,
        }
    }

    #[test]
    fn restores_only_the_same_cemu_process_and_window() {
        let s = saved();
        assert_eq!(
            recovery(&s, Some((4000, s.process_start)), Some(4000)),
            Recovery::Restore
        );
        assert_eq!(recovery(&s, None, None), Recovery::Discard, "Cemu exited");
        assert_eq!(
            recovery(&s, Some((4000, 999)), Some(4000)),
            Recovery::Discard,
            "pid reused"
        );
        assert_eq!(
            recovery(&s, Some((4000, s.process_start)), None),
            Recovery::Discard,
            "window closed"
        );
        assert_eq!(
            recovery(&s, Some((4000, s.process_start)), Some(7)),
            Recovery::Discard,
            "handle reused"
        );
    }

    fn temp(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("wiiu-tv-record-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(RECORD_FILE)
    }

    #[test]
    fn the_record_round_trips_and_deletes() {
        let path = temp("roundtrip");
        assert_eq!(read_record(&path), None);
        write_record(&path, &saved()).unwrap();
        assert_eq!(read_record(&path), Some(saved()));
        delete_record(&path);
        assert!(!path.exists());
        delete_record(&path); // missing file: no panic
    }

    #[test]
    fn a_corrupt_record_reads_as_none() {
        let path = temp("corrupt");
        std::fs::write(&path, b"{ not json").unwrap();
        assert_eq!(read_record(&path), None);
    }
}
