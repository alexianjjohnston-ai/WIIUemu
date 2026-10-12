//! Button definitions per controller role and their mapping to DSU bits.
//!
//! The mapping tables are the single source of truth: the session server uses them to build DSU packets and
//! `wiiu-cemu` uses them to write matching Cemu profiles. See `docs/controller-protocol.md`.

use wiiu_dsu::pad::buttons as dsu;

macro_rules! button_enum {
    ($(#[$m:meta])* $name:ident { $($variant:ident = $wire:literal => $dsu:expr),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// Name used in the phone protocol.
            pub const fn wire_name(self) -> &'static str {
                match self { $($name::$variant => $wire),+ }
            }

            pub fn from_wire(s: &str) -> Option<Self> {
                match s { $($wire => Some($name::$variant),)+ _ => None }
            }

            /// DSU button bit carrying this button (see `wiiu_dsu::pad::buttons`).
            pub const fn dsu_mask(self) -> DsuTarget {
                match self { $($name::$variant => $dsu),+ }
            }

            /// Bit of this button in a [`ButtonSet`].
            pub const fn bit(self) -> u32 {
                1 << (self as u32)
            }
        }
    };
}

/// Where a phone button ends up in a DSU packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DsuTarget {
    /// One of the 16 `state1`/`state2` bits.
    Bit(u16),
    /// The touchpad-click byte (Cemu `kButton16`).
    TouchClick,
}

use DsuTarget::{Bit, TouchClick};

button_enum! {
    /// Wii Remote (+ Nunchuk C/Z) buttons.
    WiimoteButton {
        A = "A" => Bit(dsu::CROSS),
        B = "B" => Bit(dsu::CIRCLE),
        One = "ONE" => Bit(dsu::SQUARE),
        Two = "TWO" => Bit(dsu::TRIANGLE),
        Plus = "PLUS" => Bit(dsu::OPTIONS),
        Minus = "MINUS" => Bit(dsu::SHARE),
        // Cemu ignores the DSU "PS" byte, so HOME rides on L3.
        Home = "HOME" => Bit(dsu::L3),
        Up = "UP" => Bit(dsu::UP),
        Down = "DOWN" => Bit(dsu::DOWN),
        Left = "LEFT" => Bit(dsu::LEFT),
        Right = "RIGHT" => Bit(dsu::RIGHT),
        C = "C" => Bit(dsu::L1),
        Z = "Z" => Bit(dsu::L2),
    }
}

impl WiimoteButton {
    /// Cemu `WiimoteController::ButtonId` (src/input/emulated/WiimoteController.h).
    pub const fn cemu_mapping_id(self) -> u64 {
        match self {
            WiimoteButton::A => 1,
            WiimoteButton::B => 2,
            WiimoteButton::One => 3,
            WiimoteButton::Two => 4,
            WiimoteButton::Z => 5,
            WiimoteButton::C => 6,
            WiimoteButton::Plus => 7,
            WiimoteButton::Minus => 8,
            WiimoteButton::Up => 9,
            WiimoteButton::Down => 10,
            WiimoteButton::Left => 11,
            WiimoteButton::Right => 12,
            WiimoteButton::Home => 17,
        }
    }
}

button_enum! {
    /// Wii U GamePad buttons. Nintendo face-button positions map to the same positions on a DSU pad.
    GamepadButton {
        A = "A" => Bit(dsu::CIRCLE),
        B = "B" => Bit(dsu::CROSS),
        X = "X" => Bit(dsu::TRIANGLE),
        Y = "Y" => Bit(dsu::SQUARE),
        L = "L" => Bit(dsu::L1),
        R = "R" => Bit(dsu::R1),
        ZL = "ZL" => Bit(dsu::L2),
        ZR = "ZR" => Bit(dsu::R2),
        Plus = "PLUS" => Bit(dsu::OPTIONS),
        Minus = "MINUS" => Bit(dsu::SHARE),
        Home = "HOME" => TouchClick,
        Up = "UP" => Bit(dsu::UP),
        Down = "DOWN" => Bit(dsu::DOWN),
        Left = "LEFT" => Bit(dsu::LEFT),
        Right = "RIGHT" => Bit(dsu::RIGHT),
        L3 = "L3" => Bit(dsu::L3),
        R3 = "R3" => Bit(dsu::R3),
    }
}

/// Set of held buttons for one role, as a bitmask of `Button::bit()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ButtonSet(pub u32);

impl ButtonSet {
    pub fn contains(self, bit: u32) -> bool {
        self.0 & bit != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn assert_injective(targets: Vec<DsuTarget>) {
        let unique: HashSet<_> = targets.iter().map(|t| format!("{t:?}")).collect();
        assert_eq!(
            unique.len(),
            targets.len(),
            "two buttons share a DSU target: {targets:?}"
        );
    }

    #[test]
    fn wiimote_buttons_have_distinct_dsu_bits_and_cemu_ids() {
        assert_injective(WiimoteButton::ALL.iter().map(|b| b.dsu_mask()).collect());
        let ids: HashSet<_> = WiimoteButton::ALL.iter().map(|b| b.cemu_mapping_id()).collect();
        assert_eq!(ids.len(), WiimoteButton::ALL.len());
    }

    #[test]
    fn gamepad_buttons_have_distinct_dsu_targets() {
        assert_injective(GamepadButton::ALL.iter().map(|b| b.dsu_mask()).collect());
    }

    #[test]
    fn wire_names_round_trip() {
        for b in WiimoteButton::ALL {
            assert_eq!(WiimoteButton::from_wire(b.wire_name()), Some(*b));
        }
        for b in GamepadButton::ALL {
            assert_eq!(GamepadButton::from_wire(b.wire_name()), Some(*b));
        }
        assert_eq!(WiimoteButton::from_wire("a"), None, "names are case sensitive");
        assert_eq!(WiimoteButton::from_wire("X"), None);
    }

    #[test]
    fn documented_wiimote_mapping() {
        // Mirrors the table in docs/controller-protocol.md.
        assert_eq!(WiimoteButton::A.dsu_mask(), Bit(dsu::CROSS));
        assert_eq!(WiimoteButton::B.dsu_mask(), Bit(dsu::CIRCLE));
        assert_eq!(WiimoteButton::Home.dsu_mask(), Bit(dsu::L3));
        assert_eq!(WiimoteButton::Home.cemu_mapping_id(), 17);
    }
}
