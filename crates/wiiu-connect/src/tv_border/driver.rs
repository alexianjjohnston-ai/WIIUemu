//! Applies the border: find Cemu, draw the tags, move Cemu inside, and undo it all. Platform calls go through
//! [`Desktop`], so this logic is tested with a fake on every platform.

use super::cemu::{choose_main_window, WindowInfo};
use super::recovery::{delete_record, read_record, recovery, write_record, Recovery, SavedWindow};
use serde::Serialize;
use std::path::PathBuf;
use wiiu_tracking::{screen_layout, BorderConfig, MarkerLayout, PixelRect};

/// What the driver needs from the operating system.
pub trait Desktop {
    /// Visible top-level windows.
    fn windows(&self) -> Vec<WindowInfo>;
    /// Pid and start time of a running process, if it runs.
    fn process(&self, pid: u32) -> Option<(u32, u64)>;
    /// The pid that owns window `handle`, if it still exists.
    fn owner(&self, handle: u64) -> Option<u32>;
    /// Left, top, right, bottom of the monitor showing most of window `handle`.
    fn monitor(&self, handle: u64) -> Option<[i32; 4]>;
    /// The window's current frame, menu and position, with its process id and start time.
    fn save(&self, handle: u64) -> Option<SavedWindow>;
    /// Frame and menu off, client area exactly `client` (screen coordinates). True when it got there.
    fn place(&mut self, handle: u64, client: [i32; 4]) -> bool;
    fn restore(&mut self, saved: &SavedWindow) -> bool;
    /// Black window over `monitor` with `white` cells (relative to the monitor's top-left).
    fn show_border(&mut self, monitor: [i32; 4], white: Vec<[i32; 4]>) -> bool;
    fn hide_border(&mut self);
}

/// Shown on the setup page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TvStatus {
    Off,
    WaitingForCemu,
    On {
        width: i32,
        height: i32,
    },
    /// Not available on this platform yet.
    Unsupported,
    Error {
        message: String,
    },
}

/// The border as it is applied now.
struct Applied {
    saved: SavedWindow,
    client: [i32; 4],
}

pub struct Driver<D: Desktop> {
    desktop: D,
    record: PathBuf,
    applied: Option<Applied>,
}

fn rect(r: PixelRect, dx: i32, dy: i32) -> [i32; 4] {
    [r.left + dx, r.top + dy, r.right + dx, r.bottom + dy]
}

impl<D: Desktop> Driver<D> {
    /// `record`: where the recovery record lives (`<data dir>/tv-border-restore.json`).
    pub fn new(desktop: D, record: PathBuf) -> Self {
        Self {
            desktop,
            record,
            applied: None,
        }
    }

    /// Repairs Cemu's window if a previous run was killed while the border showed. Call once at start.
    pub fn recover(&mut self) {
        let Some(saved) = read_record(&self.record) else {
            return;
        };
        let alive = self.desktop.process(saved.pid);
        let owner = self.desktop.owner(saved.handle);
        if recovery(&saved, alive, owner) == Recovery::Restore {
            tracing::info!("restoring Cemu's window after an unclean stop");
            self.desktop.restore(&saved);
        }
        delete_record(&self.record);
    }

    /// Brings the screen to the wanted state and reports it.
    pub fn tick(&mut self, want: bool) -> TvStatus {
        match (
            want,
            self.applied.as_ref().map(|a| (a.saved.handle, a.saved.pid, a.client)),
        ) {
            (false, None) => TvStatus::Off,
            (false, Some(_)) => {
                self.undo();
                TvStatus::Off
            }
            (true, Some((handle, pid, client))) => {
                if self.desktop.owner(handle) != Some(pid) {
                    // Cemu closed: nothing to restore.
                    self.desktop.hide_border();
                    delete_record(&self.record);
                    self.applied = None;
                    return TvStatus::WaitingForCemu;
                }
                // Keep in place: Cemu may have recreated its menu or toggled full screen.
                self.desktop.place(handle, client);
                TvStatus::On {
                    width: client[2] - client[0],
                    height: client[3] - client[1],
                }
            }
            (true, None) => self.apply(),
        }
    }

    fn apply(&mut self) -> TvStatus {
        let windows = self.desktop.windows();
        let Some(cemu) = choose_main_window(&windows) else {
            return TvStatus::WaitingForCemu;
        };
        let handle = cemu.handle;
        let Some(monitor) = self.desktop.monitor(handle) else {
            return TvStatus::Error {
                message: "couldn't find the display that shows Cemu".into(),
            };
        };
        let (width, height) = (monitor[2] - monitor[0], monitor[3] - monitor[1]);
        let layout = MarkerLayout::border(&BorderConfig::default());
        let Some(screen) = screen_layout(width.max(0) as u32, height.max(0) as u32, &layout) else {
            return TvStatus::Error {
                message: format!("the display is {width}×{height}"),
            };
        };
        let Some(saved) = self.desktop.save(handle) else {
            return TvStatus::WaitingForCemu;
        };
        // The record goes first: if Wii U Connect dies after this point, the next start repairs Cemu.
        if let Err(e) = write_record(&self.record, &saved) {
            return TvStatus::Error {
                message: format!("couldn't save Cemu's window state: {e}"),
            };
        }
        let white = screen.white.iter().map(|&r| rect(r, 0, 0)).collect();
        if !self.desktop.show_border(monitor, white) {
            delete_record(&self.record);
            return TvStatus::Error {
                message: "couldn't open the border window".into(),
            };
        }
        let client = rect(screen.game, monitor[0], monitor[1]);
        if !self.desktop.place(handle, client) {
            tracing::warn!("Cemu's window didn't land exactly on {client:?}");
        }
        self.applied = Some(Applied { saved, client });
        TvStatus::On {
            width: client[2] - client[0],
            height: client[3] - client[1],
        }
    }

    fn undo(&mut self) {
        if let Some(a) = self.applied.take() {
            self.desktop.restore(&a.saved);
            self.desktop.hide_border();
            delete_record(&self.record);
        }
    }

    /// Undoes everything (Wii U Connect is stopping).
    pub fn shutdown(&mut self) {
        self.undo();
    }

    #[cfg(test)]
    fn desktop(&self) -> &D {
        &self.desktop
    }

    #[cfg(test)]
    pub(crate) fn into_desktop(self) -> D {
        self.desktop
    }
}

#[cfg(test)]
mod tests {
    use super::super::recovery::{read_record, write_record};
    use super::*;
    use wiiu_tracking::{screen_layout, BorderConfig, MarkerLayout};

    use super::super::fake::{record_path, saved_state, Fake, MONITOR};

    fn expected_client() -> [i32; 4] {
        let l = screen_layout(1920, 1080, &MarkerLayout::border(&BorderConfig::default())).unwrap();
        [l.game.left + 1920, l.game.top, l.game.right + 1920, l.game.bottom]
    }

    #[test]
    fn without_cemu_it_waits_and_changes_nothing() {
        let path = record_path("nocemu");
        let fake = Fake {
            record: path.clone(),
            ..Fake::default()
        };
        let mut d = Driver::new(fake, path.clone());
        assert_eq!(d.tick(true), TvStatus::WaitingForCemu);
        assert!(d.desktop().border.is_none() && d.desktop().placed.is_empty());
        assert!(!path.exists());
        assert_eq!(d.tick(false), TvStatus::Off);
    }

    #[test]
    fn shows_the_border_and_places_cemu_after_writing_the_record() {
        let path = record_path("on");
        let mut d = Driver::new(Fake::with_cemu(path.clone()), path.clone());
        let client = expected_client();
        assert_eq!(
            d.tick(true),
            TvStatus::On {
                width: client[2] - client[0],
                height: client[3] - client[1]
            }
        );
        let f = d.desktop();
        assert_eq!(f.placed, vec![client]);
        assert!(
            f.record_before_place,
            "the record must exist before Cemu's window changes"
        );
        assert_eq!(f.border.map(|b| b.0), Some(MONITOR));
        assert!(f.border.unwrap().1 > 0, "tag cells drawn");
        assert_eq!(read_record(&path), Some(saved_state()));
        // Staying on re-places Cemu (keep in place) without reopening the border.
        d.tick(true);
        assert_eq!(d.desktop().placed.len(), 2);
        assert_eq!(d.desktop().borders_shown, 1);
    }

    #[test]
    fn turning_off_restores_cemu_hides_the_border_and_deletes_the_record() {
        let path = record_path("off");
        let mut d = Driver::new(Fake::with_cemu(path.clone()), path.clone());
        d.tick(true);
        assert_eq!(d.tick(false), TvStatus::Off);
        assert_eq!(d.desktop().restored, vec![saved_state()]);
        assert!(d.desktop().border.is_none());
        assert!(!path.exists());
        // Off stays off without touching Cemu again.
        d.tick(false);
        assert_eq!(d.desktop().restored.len(), 1);
    }

    #[test]
    fn cemu_closing_while_on_closes_the_border_without_restoring() {
        let path = record_path("closed");
        let mut d = Driver::new(Fake::with_cemu(path.clone()), path.clone());
        d.tick(true);
        d.desktop.windows.clear();
        assert_eq!(d.tick(true), TvStatus::WaitingForCemu);
        assert!(d.desktop().border.is_none());
        assert!(d.desktop().restored.is_empty());
        assert!(!path.exists());
    }

    #[test]
    fn shutdown_restores_cemu() {
        let path = record_path("shutdown");
        let mut d = Driver::new(Fake::with_cemu(path.clone()), path.clone());
        d.tick(true);
        d.shutdown();
        assert_eq!(d.desktop().restored, vec![saved_state()]);
        assert!(d.desktop().border.is_none());
        assert!(!path.exists());
    }

    #[test]
    fn recovery_restores_the_same_cemu_and_discards_anything_else() {
        let path = record_path("recover");
        write_record(&path, &saved_state()).unwrap();
        let mut d = Driver::new(Fake::with_cemu(path.clone()), path.clone());
        d.recover();
        assert_eq!(d.desktop().restored, vec![saved_state()]);
        assert!(!path.exists());

        // A record from a Cemu that has since exited (pid gone): nothing restored, record removed.
        let path = record_path("recover-gone");
        write_record(
            &path,
            &SavedWindow {
                pid: 5555,
                ..saved_state()
            },
        )
        .unwrap();
        let mut d = Driver::new(Fake::with_cemu(path.clone()), path.clone());
        d.recover();
        assert!(d.desktop().restored.is_empty());
        assert!(!path.exists());
    }
}
