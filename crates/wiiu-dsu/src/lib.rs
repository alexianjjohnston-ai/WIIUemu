//! Cemuhook "DSU" protocol server used to feed virtual controllers into Cemu.
//!
//! Cemu's DSU client turns DSU touch point 1 into the emulated Wii Remote's IR pointer and the GamePad's
//! touchscreen, so this crate is how Wii U Connect injects buttons, pointer and motion without patching Cemu.
//! See `docs/controller-protocol.md` and `docs/research.md`.

pub mod pad;
pub mod probe;
pub mod protocol;
pub mod server;

pub use pad::{Motion, PadData, TouchPoint};
pub use protocol::{SlotInfo, MAX_SLOTS};
pub use server::{DsuServer, SlotTable, DEFAULT_PORT};
