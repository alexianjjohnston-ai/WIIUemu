//! Conversion from validated phone input to the DSU pad state Cemu consumes.

use crate::buttons::{DsuTarget, GamepadButton, WiimoteButton};
use crate::protocol::{ControllerInput, MotionInput, Role};
use wiiu_dsu::pad::{axis_to_u8, TOUCH_HEIGHT, TOUCH_WIDTH};
use wiiu_dsu::{Motion, PadData, TouchPoint};

/// Maps a normalised pointer (0..=1, origin top-left) to DSU touch point coordinates.
///
/// Cemu divides by 1920 × 942 and then computes `pos = p * 2 - 1` for the Wii Remote DPD position, so
/// (0, 0) → (-1, -1) and (1, 1) → (1, 1).
pub fn pointer_to_touch(x: f32, y: f32) -> TouchPoint {
    let scale = |v: f32, max: u16| (v.clamp(0.0, 1.0) * max as f32).round() as u16;
    TouchPoint {
        id: 0,
        x: scale(x, TOUCH_WIDTH),
        y: scale(y, TOUCH_HEIGHT),
    }
}

/// Builds the DSU pad for `role`. `motion` must already be on the server's monotonic timeline
/// (see [`MotionClock`]); pass `None` to leave motion untouched (timestamp 0 is never integrated by Cemu).
pub fn to_pad(role: Role, input: &ControllerInput, motion: Option<Motion>) -> PadData {
    let mut pad = PadData::neutral();
    let mut apply = |held: bool, target: DsuTarget| {
        if held {
            match target {
                DsuTarget::Bit(mask) => pad.buttons |= mask,
                DsuTarget::TouchClick => pad.touch_click = true,
            }
        }
    };
    match role {
        Role::Wiimote => {
            for b in WiimoteButton::ALL {
                apply(input.buttons.contains(b.bit()), b.dsu_mask());
            }
        }
        Role::Gamepad => {
            for b in GamepadButton::ALL {
                apply(input.buttons.contains(b.bit()), b.dsu_mask());
            }
        }
    }
    pad.left_stick = [axis_to_u8(input.stick.0), axis_to_u8(input.stick.1)];
    pad.right_stick = [axis_to_u8(input.rstick.0), axis_to_u8(input.rstick.1)];
    pad.touch[0] = input.pointer.map(|(x, y)| pointer_to_touch(x, y));
    if let Some(m) = motion {
        pad.motion = m;
    }
    pad
}

/// Re-bases phone motion timestamps onto one strictly increasing server timeline per slot.
///
/// Cemu ignores samples whose timestamp is not newer than the last one it integrated, and resets when a
/// timestamp jumps more than 10 s backwards. A reconnecting phone restarts its clock, so raw timestamps would
/// stall motion for up to 10 s. This keeps the phone's sample spacing but anchors it to the server clock.
#[derive(Debug, Default)]
pub struct MotionClock {
    /// `server_us - phone_us` for the current phone session.
    offset: Option<i128>,
    last_out: u64,
}

impl MotionClock {
    /// Call when a new phone session starts on this slot.
    pub fn reset_session(&mut self) {
        self.offset = None;
    }

    /// `server_now_us` is a monotonic server clock (µs). Returns the motion sample to put in the DSU packet.
    pub fn map(&mut self, sample: &MotionInput, server_now_us: u64) -> Motion {
        let phone = sample.ts_us as i128;
        let offset = *self.offset.get_or_insert(server_now_us as i128 - phone);
        let mut ts = (phone + offset).max(0) as u64;
        // Never go backwards or repeat; never run ahead of the server clock by more than a frame of jitter.
        if ts <= self.last_out {
            ts = self.last_out + 1;
        }
        if ts > server_now_us + 50_000 {
            // Phone clock runs fast relative to ours; re-anchor.
            self.offset = Some(server_now_us as i128 - phone);
            ts = server_now_us.max(self.last_out + 1);
        }
        self.last_out = ts;
        Motion {
            timestamp_us: ts,
            accel: sample.accel,
            gyro: sample.gyro,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buttons::ButtonSet;
    use wiiu_dsu::pad::buttons as dsu;

    #[test]
    fn pointer_corners_map_to_cemu_range() {
        assert_eq!(pointer_to_touch(0.0, 0.0), TouchPoint { id: 0, x: 0, y: 0 });
        assert_eq!(pointer_to_touch(1.0, 1.0), TouchPoint { id: 0, x: 1920, y: 942 });
        assert_eq!(pointer_to_touch(0.5, 0.5), TouchPoint { id: 0, x: 960, y: 471 });
    }

    #[test]
    fn pointer_round_trip_through_cemu_formula_is_accurate() {
        // Cemu: pos = (touch / [1920, 942]) * 2 - 1. Error must stay below one touch unit.
        for i in 0..=100 {
            let v = i as f32 / 100.0;
            let t = pointer_to_touch(v, v);
            let cx = t.x as f32 / 1920.0;
            let cy = t.y as f32 / 942.0;
            assert!((cx - v).abs() <= 0.5 / 1920.0 + f32::EPSILON);
            assert!((cy - v).abs() <= 0.5 / 942.0 + f32::EPSILON);
        }
    }

    #[test]
    fn wiimote_input_to_pad() {
        let input = ControllerInput {
            buttons: ButtonSet(WiimoteButton::A.bit() | WiimoteButton::Home.bit() | WiimoteButton::Up.bit()),
            pointer: Some((0.5, 0.25)),
            stick: (1.0, -1.0),
            ..Default::default()
        };
        let pad = to_pad(Role::Wiimote, &input, None);
        assert_eq!(pad.buttons, dsu::CROSS | dsu::L3 | dsu::UP);
        assert_eq!(pad.touch[0], Some(TouchPoint { id: 0, x: 960, y: 236 }));
        assert_eq!(pad.touch[1], None);
        assert_eq!(pad.left_stick, [255, 0]);
        assert_eq!(pad.right_stick, [128, 128]);
        assert_eq!(pad.motion.timestamp_us, 0);
    }

    #[test]
    fn empty_input_is_neutral() {
        assert_eq!(
            to_pad(Role::Wiimote, &ControllerInput::default(), None),
            PadData::neutral()
        );
    }

    #[test]
    fn gamepad_home_uses_touch_click() {
        let input = ControllerInput {
            buttons: ButtonSet(GamepadButton::Home.bit() | GamepadButton::A.bit()),
            ..Default::default()
        };
        let pad = to_pad(Role::Gamepad, &input, None);
        assert!(pad.touch_click);
        assert_eq!(pad.buttons, dsu::CIRCLE);
    }

    fn sample(ts: u64) -> MotionInput {
        MotionInput {
            accel: [0.0, -1.0, 0.0],
            gyro: [0.0; 3],
            ts_us: ts,
        }
    }

    #[test]
    fn motion_clock_keeps_spacing_and_is_monotonic() {
        let mut c = MotionClock::default();
        let a = c.map(&sample(5_000_000), 1_000_000).timestamp_us;
        let b = c.map(&sample(5_016_000), 1_016_500).timestamp_us;
        assert_eq!(a, 1_000_000);
        assert_eq!(b - a, 16_000, "phone sample spacing preserved");
        // Duplicate phone timestamp still advances.
        let d = c.map(&sample(5_016_000), 1_020_000).timestamp_us;
        assert!(d > b);
    }

    #[test]
    fn motion_clock_survives_phone_clock_restart() {
        let mut c = MotionClock::default();
        let before = c.map(&sample(9_000_000), 2_000_000).timestamp_us;
        c.reset_session();
        // Phone reconnects with its clock near zero.
        let after = c.map(&sample(1_000), 2_500_000).timestamp_us;
        assert!(after > before);
        assert_eq!(after, 2_500_000);
    }

    #[test]
    fn motion_clock_reanchors_fast_phone_clock() {
        let mut c = MotionClock::default();
        c.map(&sample(0), 1_000_000);
        // Phone claims 1 s passed, server says 10 ms.
        let t = c.map(&sample(1_000_000), 1_010_000).timestamp_us;
        assert!(t <= 1_010_000 + 50_000);
    }
}
