//! Controller models and the phone ↔ PC protocol.
//!
//! This crate has no I/O: it validates phone messages and turns them into DSU pad state, so it can be tested
//! exhaustively without sockets, phones or Cemu.

pub mod buttons;
pub mod protocol;
pub mod state;

pub use buttons::{ButtonSet, GamepadButton, WiimoteButton};
pub use protocol::{ControllerInput, Role};
