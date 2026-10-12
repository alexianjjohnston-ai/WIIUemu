//! Pad state carried in DSU data packets.

/// Button bits. The bit index equals Cemu's `kButtonN` id for DSU controllers:
/// Cemu walks `state1` bits 0..7 to `kButton0..7` and `state2` bits 0..7 to `kButton8..15`
/// (`DSUController::raw_state`). The low byte is `state1`, the high byte `state2`.
pub mod buttons {
    // state1
    pub const SHARE: u16 = 1 << 0;
    pub const L3: u16 = 1 << 1;
    pub const R3: u16 = 1 << 2;
    pub const OPTIONS: u16 = 1 << 3;
    pub const UP: u16 = 1 << 4;
    pub const RIGHT: u16 = 1 << 5;
    pub const DOWN: u16 = 1 << 6;
    pub const LEFT: u16 = 1 << 7;
    // state2
    pub const L2: u16 = 1 << 8;
    pub const R2: u16 = 1 << 9;
    pub const L1: u16 = 1 << 10;
    pub const R1: u16 = 1 << 11;
    pub const TRIANGLE: u16 = 1 << 12;
    pub const CIRCLE: u16 = 1 << 13;
    pub const CROSS: u16 = 1 << 14;
    pub const SQUARE: u16 = 1 << 15;

    /// Cemu's button id (`kButtonN`) for a single-bit mask.
    pub const fn cemu_button_id(mask: u16) -> u64 {
        mask.trailing_zeros() as u64
    }

    /// Cemu maps the touchpad-click byte to `kButton16`.
    pub const CEMU_TOUCH_CLICK_ID: u64 = 16;
}

/// Touchpad resolution Cemu assumes when normalising DSU touch coordinates (`DSUController::get_position`).
pub const TOUCH_WIDTH: u16 = 1920;
pub const TOUCH_HEIGHT: u16 = 942;

/// Centre value for stick axes (Cemu maps `0..=255` to `-1..=1`).
pub const STICK_CENTER: u8 = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TouchPoint {
    pub id: u8,
    /// 0..=1920 (Cemu divides by 1920)
    pub x: u16,
    /// 0..=942 (Cemu divides by 942)
    pub y: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Motion {
    /// Microseconds, must increase for Cemu to integrate a sample.
    pub timestamp_us: u64,
    /// In g, DSU axis convention.
    pub accel: [f32; 3],
    /// In deg/s, DSU axis convention (pitch, yaw, roll).
    pub gyro: [f32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PadData {
    pub buttons: u16,
    /// DSU "PS" byte. Cemu ignores it; kept for other DSU clients.
    pub home: bool,
    pub touch_click: bool,
    /// `[x, y]`, 0..=255, +y = up.
    pub left_stick: [u8; 2],
    pub right_stick: [u8; 2],
    pub touch: [Option<TouchPoint>; 2],
    pub motion: Motion,
}

impl PadData {
    /// Nothing pressed, sticks centred, no touch, motion timestamp 0 (never integrated by Cemu).
    pub const fn neutral() -> Self {
        Self {
            buttons: 0,
            home: false,
            touch_click: false,
            left_stick: [STICK_CENTER; 2],
            right_stick: [STICK_CENTER; 2],
            touch: [None, None],
            motion: Motion {
                timestamp_us: 0,
                accel: [0.0; 3],
                gyro: [0.0; 3],
            },
        }
    }
}

impl Default for PadData {
    fn default() -> Self {
        Self::neutral()
    }
}

/// Converts a stick axis in `-1.0..=1.0` (+ = right/up) to the DSU byte. Non-finite input maps to centre.
pub fn axis_to_u8(v: f32) -> u8 {
    if !v.is_finite() {
        return STICK_CENTER;
    }
    // Cemu: value / 255 * 2 - 1, so 0 -> -1, 255 -> +1, 128 -> ~0.0039.
    ((v.clamp(-1.0, 1.0) + 1.0) * 127.5).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_bits_match_cemu_ids() {
        assert_eq!(buttons::cemu_button_id(buttons::SHARE), 0);
        assert_eq!(buttons::cemu_button_id(buttons::L3), 1);
        assert_eq!(buttons::cemu_button_id(buttons::OPTIONS), 3);
        assert_eq!(buttons::cemu_button_id(buttons::UP), 4);
        assert_eq!(buttons::cemu_button_id(buttons::RIGHT), 5);
        assert_eq!(buttons::cemu_button_id(buttons::DOWN), 6);
        assert_eq!(buttons::cemu_button_id(buttons::LEFT), 7);
        assert_eq!(buttons::cemu_button_id(buttons::L2), 8);
        assert_eq!(buttons::cemu_button_id(buttons::L1), 10);
        assert_eq!(buttons::cemu_button_id(buttons::TRIANGLE), 12);
        assert_eq!(buttons::cemu_button_id(buttons::CIRCLE), 13);
        assert_eq!(buttons::cemu_button_id(buttons::CROSS), 14);
        assert_eq!(buttons::cemu_button_id(buttons::SQUARE), 15);
    }

    #[test]
    fn axis_conversion() {
        assert_eq!(axis_to_u8(-1.0), 0);
        assert_eq!(axis_to_u8(1.0), 255);
        assert_eq!(axis_to_u8(0.0), 128);
        assert_eq!(axis_to_u8(5.0), 255);
        assert_eq!(axis_to_u8(f32::NAN), STICK_CENTER);
    }
}
