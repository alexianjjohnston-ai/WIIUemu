//! The TV border during play (ADR-0004): while a Wii Remote phone's camera is on, the screen showing Cemu gets the
//! AprilTag border and Cemu's game window moves inside it.

pub mod cemu;
pub mod controller;
pub mod driver;
#[cfg(test)]
mod fake;
pub mod recovery;
pub mod runner;
#[cfg(windows)]
mod windows_desktop;

use serde::{Deserialize, Serialize};

/// The setup page's switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TvBorderMode {
    /// On while at least one Wii Remote camera is on.
    #[default]
    Auto,
    On,
    Off,
}

pub use driver::TvStatus;
pub use runner::TvBorder;

/// Starts the `tv-border` thread on Windows; elsewhere marks the border unsupported. `record` is
/// `<data dir>/tv-border-restore.json`. The thread restores Cemu's window before it ends.
pub fn spawn(
    shared: std::sync::Arc<TvBorder>,
    sessions: std::sync::Arc<crate::session::SessionManager>,
    record: std::path::PathBuf,
    stop: tokio::sync::watch::Receiver<bool>,
) -> Option<std::thread::JoinHandle<()>> {
    #[cfg(windows)]
    {
        std::thread::Builder::new()
            .name("tv-border".into())
            .spawn(move || {
                let driver = driver::Driver::new(windows_desktop::WindowsDesktop::new(), record);
                runner::run(driver, shared, |now| sessions.cameras_active(now), stop, runner::TICK);
            })
            .map_err(|e| tracing::error!("cannot start the TV border thread: {e}"))
            .ok()
    }
    #[cfg(not(windows))]
    {
        let _ = (sessions, record, stop);
        shared.report_unsupported();
        None
    }
}
