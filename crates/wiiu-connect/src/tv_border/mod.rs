//! The TV border during play (ADR-0004): while a Wii Remote phone's camera is on, the screen showing Cemu gets the
//! AprilTag border and Cemu's game window moves inside it.

pub mod controller;

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
