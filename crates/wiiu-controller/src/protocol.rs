//! Phone ↔ PC protocol v1 (JSON over WebSocket). Specification: `docs/controller-protocol.md`.

use crate::buttons::{ButtonSet, GamepadButton, WiimoteButton};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Newest protocol version this crate speaks. Version 2 adds camera tracking (`camera_frame`, `gyro`, `tracking`).
pub const PROTOCOL_VERSION: u32 = 2;
/// Oldest protocol version still accepted.
pub const MIN_PROTOCOL_VERSION: u32 = 1;
/// Frames larger than this are rejected before parsing.
pub const MAX_FRAME_BYTES: usize = 4096;
pub const MAX_BUTTONS: usize = 16;
pub const MAX_NAME_CHARS: usize = 40;
/// Largest integer a JavaScript number represents exactly.
pub const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;
/// Bounds for motion values: well above anything a phone IMU reports (±16 g, ±2000 °/s are typical maxima).
pub const MAX_ACCEL_G: f64 = 64.0;
pub const MAX_GYRO_DPS: f64 = 8000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Wiimote,
    Gamepad,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ClientMessage {
    Hello(Hello),
    Input(RawInput),
    Ping {
        id: u64,
    },
    /// Protocol ≥ 2: gyro samples `[t_us, rate_x, rate_y, rate_z]`, rad/s, W3C device frame.
    Gyro {
        samples: Vec<[f64; 4]>,
    },
    /// Protocol ≥ 2: the player is aiming at the centre of the TV image now; measure the aim point.
    Calibrate,
    /// Protocol ≥ 2: apply a stored aim point (fraction of the camera image), or the image centre for `null`.
    /// Check it with [`validate_aim_point`].
    Aim {
        // Required, but may be `null`: without `deserialize_with`, serde would accept a missing field as `None`.
        #[serde(deserialize_with = "Option::deserialize")]
        point: Option<Xy>,
    },
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hello {
    pub protocol: u32,
    pub token: String,
    pub role: Role,
    pub device_id: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Xy {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct RawMotion {
    pub accel: [f64; 3],
    pub gyro: [f64; 3],
    pub ts: u64,
}

/// An `input` message as received, before role-specific validation.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RawInput {
    pub seq: u64,
    pub buttons: Vec<String>,
    #[serde(default)]
    pub pointer: Option<Xy>,
    #[serde(default)]
    pub stick: Option<Xy>,
    #[serde(default)]
    pub rstick: Option<Xy>,
    #[serde(default)]
    pub motion: Option<RawMotion>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ServerMessage {
    Welcome {
        protocol: u32,
        role: Role,
        slot: u8,
        player: u8,
    },
    Pong {
        id: u64,
    },
    /// Protocol ≥ 2: camera-tracking feedback for the phone UI.
    Tracking {
        state: TrackingState,
        pointer: Option<PointerXy>,
        tags: u32,
        fixes_per_s: f32,
    },
    /// Protocol ≥ 2: result of a `calibrate` request.
    Calibration {
        ok: bool,
        /// The new aim point (fraction of the camera image) on success.
        point: Option<PointerXy>,
        reason: Option<CalibrationFailure>,
    },
    Error {
        code: ErrorCode,
        message: String,
    },
}

/// Why a calibration failed (the previous aim point stays in place).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationFailure {
    /// No camera fix within the time limit: the TV border was not in view.
    NoMarkers,
    /// The computed aim point lies outside the camera image: the player was not aiming at the centre.
    OffImage,
}

impl ServerMessage {
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::Error {
            code,
            message: message.into(),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("server messages always serialize")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackingState {
    Tracking,
    GyroOnly,
    Lost,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PointerXy {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    BadToken,
    UnsupportedProtocol,
    InvalidMessage,
    RoleUnavailable,
    Timeout,
    Replaced,
}

#[derive(Debug, Error, PartialEq)]
pub enum ProtocolError {
    #[error("frame exceeds {MAX_FRAME_BYTES} bytes")]
    TooLarge,
    #[error("malformed message: {0}")]
    Malformed(String),
    #[error("unsupported protocol version {0}")]
    UnsupportedProtocol(u32),
    #[error("invalid field `{field}`: {reason}")]
    InvalidField { field: &'static str, reason: String },
}

impl ProtocolError {
    pub fn code(&self) -> ErrorCode {
        match self {
            ProtocolError::UnsupportedProtocol(_) => ErrorCode::UnsupportedProtocol,
            _ => ErrorCode::InvalidMessage,
        }
    }
}

fn invalid(field: &'static str, reason: impl Into<String>) -> ProtocolError {
    ProtocolError::InvalidField {
        field,
        reason: reason.into(),
    }
}

/// Parses a text frame. Does not validate field contents; see [`Hello::validate`] and [`RawInput::validate`].
pub fn parse_client_message(frame: &str) -> Result<ClientMessage, ProtocolError> {
    if frame.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::TooLarge);
    }
    serde_json::from_str(frame).map_err(|e| ProtocolError::Malformed(e.to_string()))
}

impl Hello {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if !(MIN_PROTOCOL_VERSION..=PROTOCOL_VERSION).contains(&self.protocol) {
            return Err(ProtocolError::UnsupportedProtocol(self.protocol));
        }
        let id = &self.device_id;
        if !(8..=64).contains(&id.len()) || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return Err(invalid("deviceId", "must be 8-64 characters of [A-Za-z0-9-]"));
        }
        if self.token.is_empty() || self.token.len() > 128 {
            return Err(invalid("token", "must be 1-128 characters"));
        }
        if let Some(name) = &self.name {
            if name.chars().count() > MAX_NAME_CHARS || name.chars().any(char::is_control) {
                return Err(invalid("name", "at most 40 printable characters"));
            }
        }
        Ok(())
    }
}

/// Validated controller input for one role.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ControllerInput {
    pub seq: u64,
    pub buttons: ButtonSet,
    /// Normalised (0..=1, origin top-left), `None` when off-screen.
    pub pointer: Option<(f32, f32)>,
    /// -1..=1, +y up.
    pub stick: (f32, f32),
    pub rstick: (f32, f32),
    pub motion: Option<MotionInput>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionInput {
    pub accel: [f32; 3],
    pub gyro: [f32; 3],
    pub ts_us: u64,
}

fn check_unit(field: &'static str, v: f64, lo: f64) -> Result<f32, ProtocolError> {
    if !v.is_finite() || v < lo || v > 1.0 {
        return Err(invalid(field, format!("{v} is outside [{lo}, 1]")));
    }
    Ok(v as f32)
}

fn check_xy(field: &'static str, xy: Option<Xy>, lo: f64) -> Result<Option<(f32, f32)>, ProtocolError> {
    xy.map(|p| Ok((check_unit(field, p.x, lo)?, check_unit(field, p.y, lo)?)))
        .transpose()
}

fn check_vec3(field: &'static str, v: [f64; 3], max: f64) -> Result<[f32; 3], ProtocolError> {
    let mut out = [0f32; 3];
    for (o, x) in out.iter_mut().zip(v) {
        if !x.is_finite() || x.abs() > max {
            return Err(invalid(field, format!("{x} is outside ±{max}")));
        }
        *o = x as f32;
    }
    Ok(out)
}

impl RawInput {
    pub fn validate(&self, role: Role) -> Result<ControllerInput, ProtocolError> {
        if self.seq > MAX_SAFE_INTEGER {
            return Err(invalid("seq", "exceeds 2^53 - 1"));
        }
        if self.buttons.len() > MAX_BUTTONS {
            return Err(invalid("buttons", "too many entries"));
        }
        let mut set = 0u32;
        for name in &self.buttons {
            let bit = match role {
                Role::Wiimote => WiimoteButton::from_wire(name).map(WiimoteButton::bit),
                Role::Gamepad => GamepadButton::from_wire(name).map(GamepadButton::bit),
            }
            .ok_or_else(|| invalid("buttons", format!("unknown button {name:?} for {role:?}")))?;
            if set & bit != 0 {
                return Err(invalid("buttons", format!("duplicate button {name:?}")));
            }
            set |= bit;
        }
        if role == Role::Wiimote && self.rstick.is_some() {
            return Err(invalid("rstick", "not available on a Wii Remote"));
        }
        let motion = self
            .motion
            .map(|m| {
                if m.ts > MAX_SAFE_INTEGER {
                    return Err(invalid("motion.ts", "exceeds 2^53 - 1"));
                }
                Ok(MotionInput {
                    accel: check_vec3("motion.accel", m.accel, MAX_ACCEL_G)?,
                    gyro: check_vec3("motion.gyro", m.gyro, MAX_GYRO_DPS)?,
                    ts_us: m.ts,
                })
            })
            .transpose()?;
        Ok(ControllerInput {
            seq: self.seq,
            buttons: ButtonSet(set),
            pointer: check_xy("pointer", self.pointer, 0.0)?,
            stick: check_xy("stick", self.stick, -1.0)?.unwrap_or_default(),
            rstick: check_xy("rstick", self.rstick, -1.0)?.unwrap_or_default(),
            motion,
        })
    }
}

/// Most gyro samples accepted per message, and the largest plausible rate (≈ 2000 °/s).
pub const MAX_GYRO_SAMPLES: usize = 64;
pub const MAX_GYRO_RAD_S: f64 = 35.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GyroSample {
    pub t_us: u64,
    /// rad/s, W3C device frame (x right, y up, z out of the screen).
    pub rate: [f64; 3],
}

/// Validates a `gyro` message's samples (finite, bounded, strictly increasing timestamps).
/// Checks an `aim` point: `null`, or `x` and `y` finite and within [0,1] (reject, don't clamp).
pub fn validate_aim_point(point: Option<Xy>) -> Result<Option<[f64; 2]>, ProtocolError> {
    match point {
        None => Ok(None),
        Some(Xy { x, y }) if (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y) => Ok(Some([x, y])),
        Some(_) => Err(invalid("point", "x and y must be within [0,1]")),
    }
}

pub fn validate_gyro(samples: &[[f64; 4]], last_t_us: Option<u64>) -> Result<Vec<GyroSample>, ProtocolError> {
    if samples.is_empty() || samples.len() > MAX_GYRO_SAMPLES {
        return Err(invalid("samples", format!("1-{MAX_GYRO_SAMPLES} samples required")));
    }
    let mut prev = last_t_us;
    samples
        .iter()
        .map(|s| {
            let t = s[0];
            if !(t.is_finite() && t >= 0.0 && t <= MAX_SAFE_INTEGER as f64 && t.fract() == 0.0) {
                return Err(invalid(
                    "samples",
                    "timestamp must be an integer number of microseconds",
                ));
            }
            let t = t as u64;
            if prev.is_some_and(|p| t <= p) {
                return Err(invalid("samples", "timestamps must strictly increase"));
            }
            prev = Some(t);
            let rate = [s[1], s[2], s[3]];
            if rate.iter().any(|r| !r.is_finite() || r.abs() > MAX_GYRO_RAD_S) {
                return Err(invalid(
                    "samples",
                    format!("rates must be finite and within ±{MAX_GYRO_RAD_S} rad/s"),
                ));
            }
            Ok(GyroSample { t_us: t, rate })
        })
        .collect()
}

pub const CAMERA_FRAME_HEADER: usize = 20;
pub const MAX_CAMERA_PAYLOAD: usize = 65_536;
pub const MAX_CAMERA_FRAME_BYTES: usize = CAMERA_FRAME_HEADER + MAX_CAMERA_PAYLOAD;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CameraFrame<'a> {
    pub width: u16,
    pub height: u16,
    pub seq: u32,
    pub capture_us: u64,
    /// JPEG data.
    pub jpeg: &'a [u8],
}

/// Parses a binary `camera_frame` (protocol ≥ 2). See `docs/controller-protocol.md`.
pub fn parse_camera_frame(frame: &[u8]) -> Result<CameraFrame<'_>, ProtocolError> {
    if frame.len() > MAX_CAMERA_FRAME_BYTES {
        return Err(ProtocolError::TooLarge);
    }
    if frame.len() <= CAMERA_FRAME_HEADER {
        return Err(ProtocolError::Malformed("camera frame too short".into()));
    }
    let u16_at = |o: usize| u16::from_le_bytes([frame[o], frame[o + 1]]);
    if frame[0] != 1 {
        return Err(invalid("kind", "must be 1 (camera frame)"));
    }
    if frame[1] != 1 {
        return Err(invalid("format", "must be 1 (JPEG)"));
    }
    let (width, height) = (u16_at(2), u16_at(4));
    if !(160..=1280).contains(&width) || !(120..=960).contains(&height) {
        return Err(invalid(
            "width/height",
            format!("{width}x{height} outside 160x120..1280x960"),
        ));
    }
    if u16_at(6) != 0 {
        return Err(invalid("reserved", "must be 0"));
    }
    let seq = u32::from_le_bytes(frame[8..12].try_into().expect("4 bytes"));
    let capture_us = u64::from_le_bytes(frame[12..20].try_into().expect("8 bytes"));
    if capture_us > MAX_SAFE_INTEGER {
        return Err(invalid("capture_us", "exceeds 2^53 - 1"));
    }
    Ok(CameraFrame {
        width,
        height,
        seq,
        capture_us,
        jpeg: &frame[CAMERA_FRAME_HEADER..],
    })
}

/// Builds a binary `camera_frame` (used by tests and tools; phones build the same layout in TypeScript).
pub fn encode_camera_frame(width: u16, height: u16, seq: u32, capture_us: u64, jpeg: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(CAMERA_FRAME_HEADER + jpeg.len());
    out.extend_from_slice(&[1, 1]);
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&seq.to_le_bytes());
    out.extend_from_slice(&capture_us.to_le_bytes());
    out.extend_from_slice(jpeg);
    out
}

#[cfg(test)]
mod calibration_tests {
    use super::*;

    #[test]
    fn parses_calibrate_and_aim() {
        assert_eq!(
            parse_client_message(r#"{"type":"calibrate"}"#),
            Ok(ClientMessage::Calibrate)
        );
        assert_eq!(
            parse_client_message(r#"{"type":"aim","point":{"x":0.52,"y":0.44}}"#),
            Ok(ClientMessage::Aim {
                point: Some(Xy { x: 0.52, y: 0.44 })
            })
        );
        assert_eq!(
            parse_client_message(r#"{"type":"aim","point":null}"#),
            Ok(ClientMessage::Aim { point: None })
        );
        assert!(parse_client_message(r#"{"type":"aim","point":{"x":"0.5","y":0.5}}"#).is_err());
        // `point` is required: a missing field must not silently reset the calibration.
        assert!(parse_client_message(r#"{"type":"aim"}"#).is_err());
    }

    #[test]
    fn aim_point_is_rejected_outside_the_image() {
        assert_eq!(validate_aim_point(None), Ok(None));
        assert_eq!(validate_aim_point(Some(Xy { x: 0.0, y: 1.0 })), Ok(Some([0.0, 1.0])));
        for (x, y) in [(-0.01, 0.5), (0.5, 1.01), (f64::NAN, 0.5), (0.5, f64::INFINITY)] {
            assert!(validate_aim_point(Some(Xy { x, y })).is_err(), "({x}, {y})");
        }
    }

    #[test]
    fn calibration_result_json() {
        let ok = ServerMessage::Calibration {
            ok: true,
            point: Some(PointerXy { x: 0.5, y: 0.25 }),
            reason: None,
        };
        assert_eq!(
            ok.to_json(),
            r#"{"type":"calibration","ok":true,"point":{"x":0.5,"y":0.25},"reason":null}"#
        );
        let failed = ServerMessage::Calibration {
            ok: false,
            point: None,
            reason: Some(CalibrationFailure::NoMarkers),
        };
        assert_eq!(
            failed.to_json(),
            r#"{"type":"calibration","ok":false,"point":null,"reason":"no_markers"}"#
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Same golden bytes as `apps/phone/src/tracking/tracking.test.ts`, so both encoders stay in step.
    #[test]
    fn camera_frame_golden_bytes_match_phone_encoder() {
        let jpeg = [0xff, 0xd8, 0xff, 0xd9];
        let bytes = encode_camera_frame(640, 360, 0x0102_0304, 0x01_0203_0405, &jpeg);
        assert_eq!(
            bytes[..20],
            [1, 1, 0x80, 0x02, 0x68, 0x01, 0, 0, 0x04, 0x03, 0x02, 0x01, 0x05, 0x04, 0x03, 0x02, 0x01, 0, 0, 0]
        );
        let f = parse_camera_frame(&bytes).unwrap();
        assert_eq!(
            (f.width, f.height, f.seq, f.capture_us, f.jpeg),
            (640, 360, 0x0102_0304, 0x01_0203_0405, &jpeg[..])
        );
    }

    fn input(json: &str) -> RawInput {
        match parse_client_message(json).unwrap() {
            ClientMessage::Input(i) => i,
            other => panic!("expected input, got {other:?}"),
        }
    }

    #[test]
    fn parses_hello() {
        let m = parse_client_message(
            r#"{"type":"hello","protocol":1,"token":"abc","role":"wiimote","deviceId":"device-1234","name":"Pixel"}"#,
        )
        .unwrap();
        let ClientMessage::Hello(h) = m else { panic!() };
        assert_eq!(h.role, Role::Wiimote);
        assert_eq!(h.device_id, "device-1234");
        h.validate().unwrap();
    }

    #[test]
    fn rejects_bad_hello_fields() {
        let base = Hello {
            protocol: 1,
            token: "t".into(),
            role: Role::Wiimote,
            device_id: "abcdefgh".into(),
            name: None,
        };
        assert_eq!(
            Hello {
                protocol: 3,
                ..base.clone()
            }
            .validate(),
            Err(ProtocolError::UnsupportedProtocol(3))
        );
        assert!(Hello {
            device_id: "short".into(),
            ..base.clone()
        }
        .validate()
        .is_err());
        assert!(Hello {
            device_id: "has spaces in it".into(),
            ..base.clone()
        }
        .validate()
        .is_err());
        assert!(Hello {
            device_id: "x".repeat(65),
            ..base.clone()
        }
        .validate()
        .is_err());
        assert!(Hello {
            token: String::new(),
            ..base.clone()
        }
        .validate()
        .is_err());
        assert!(Hello {
            name: Some("n".repeat(41)),
            ..base.clone()
        }
        .validate()
        .is_err());
        assert!(Hello {
            name: Some("bad\u{7}".into()),
            ..base.clone()
        }
        .validate()
        .is_err());
        base.validate().unwrap();
    }

    #[test]
    fn parses_and_validates_wiimote_input() {
        let i = input(
            r#"{"type":"input","seq":5,"buttons":["A","B"],"pointer":{"x":0.25,"y":0.75},
                "motion":{"accel":[0,-1,0],"gyro":[1,2,3],"ts":100}}"#,
        );
        let v = i.validate(Role::Wiimote).unwrap();
        assert_eq!(v.seq, 5);
        assert!(v.buttons.contains(WiimoteButton::A.bit()));
        assert!(v.buttons.contains(WiimoteButton::B.bit()));
        assert!(!v.buttons.contains(WiimoteButton::Home.bit()));
        assert_eq!(v.pointer, Some((0.25, 0.75)));
        assert_eq!(v.motion.unwrap().gyro, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn null_pointer_means_offscreen() {
        let v = input(r#"{"type":"input","seq":1,"buttons":[],"pointer":null}"#)
            .validate(Role::Wiimote)
            .unwrap();
        assert_eq!(v.pointer, None);
        assert_eq!(v.stick, (0.0, 0.0));
    }

    #[test]
    fn rejects_invalid_input() {
        let cases = [
            r#"{"type":"input","seq":1,"buttons":["X"]}"#,     // gamepad-only name
            r#"{"type":"input","seq":1,"buttons":["A","A"]}"#, // duplicate
            r#"{"type":"input","seq":1,"buttons":[],"pointer":{"x":1.5,"y":0}}"#,
            r#"{"type":"input","seq":1,"buttons":[],"pointer":{"x":-0.1,"y":0}}"#,
            r#"{"type":"input","seq":1,"buttons":[],"stick":{"x":0,"y":-1.01}}"#,
            r#"{"type":"input","seq":1,"buttons":[],"rstick":{"x":0,"y":0}}"#, // no right stick on a Wiimote
            r#"{"type":"input","seq":9007199254740992,"buttons":[]}"#,
            r#"{"type":"input","seq":1,"buttons":[],"motion":{"accel":[0,0,100],"gyro":[0,0,0],"ts":1}}"#,
        ];
        for c in cases {
            assert!(input(c).validate(Role::Wiimote).is_err(), "should reject {c}");
        }
        let many = format!(
            r#"{{"type":"input","seq":1,"buttons":[{}]}}"#,
            vec!["\"A\""; 17].join(",")
        );
        assert!(input(&many).validate(Role::Wiimote).is_err());
    }

    #[test]
    fn gamepad_accepts_its_own_buttons_only() {
        let i = input(r#"{"type":"input","seq":1,"buttons":["X","ZR","HOME"],"rstick":{"x":1,"y":-1}}"#);
        let v = i.validate(Role::Gamepad).unwrap();
        assert!(v.buttons.contains(GamepadButton::ZR.bit()));
        assert_eq!(v.rstick, (1.0, -1.0));
        assert!(input(r#"{"type":"input","seq":1,"buttons":["ONE"]}"#)
            .validate(Role::Gamepad)
            .is_err());
    }

    #[test]
    fn rejects_malformed_frames() {
        assert!(matches!(
            parse_client_message("not json"),
            Err(ProtocolError::Malformed(_))
        ));
        assert!(matches!(
            parse_client_message(r#"{"type":"launch_missiles"}"#),
            Err(ProtocolError::Malformed(_))
        ));
        assert!(matches!(
            parse_client_message(r#"{"type":"input","seq":-1,"buttons":[]}"#),
            Err(ProtocolError::Malformed(_))
        ));
        assert!(matches!(
            parse_client_message(r#"{"type":"input","seq":1,"buttons":[],"pointer":{"x":"0.5","y":0}}"#),
            Err(ProtocolError::Malformed(_))
        ));
        let huge = format!(r#"{{"type":"ping","id":1,"pad":"{}"}}"#, "x".repeat(MAX_FRAME_BYTES));
        assert_eq!(parse_client_message(&huge), Err(ProtocolError::TooLarge));
    }

    #[test]
    fn gyro_validation() {
        let ok = validate_gyro(&[[100.0, 0.1, -0.2, 0.3], [200.0, 0.0, 0.0, 0.0]], Some(50)).unwrap();
        assert_eq!(ok[1].t_us, 200);
        assert_eq!(ok[0].rate, [0.1, -0.2, 0.3]);
        assert!(validate_gyro(&[], None).is_err());
        assert!(
            validate_gyro(&[[100.0, 0.0, 0.0, 0.0]], Some(100)).is_err(),
            "not increasing across messages"
        );
        assert!(validate_gyro(&[[200.0, 0.0, 0.0, 0.0], [150.0, 0.0, 0.0, 0.0]], None).is_err());
        assert!(validate_gyro(&[[1.5, 0.0, 0.0, 0.0]], None).is_err(), "fractional µs");
        assert!(
            validate_gyro(&[[1.0, 99.0, 0.0, 0.0]], None).is_err(),
            "implausible rate"
        );
        assert!(validate_gyro(&vec![[1.0, 0.0, 0.0, 0.0]; 65], None).is_err());
        let m = parse_client_message(r#"{"type":"gyro","samples":[[1,0.1,0.2,0.3]]}"#).unwrap();
        assert!(matches!(m, ClientMessage::Gyro { samples } if samples.len() == 1));
    }

    #[test]
    fn camera_frame_round_trip_and_rejection() {
        let jpeg = [0xFF, 0xD8, 0xFF, 0xD9];
        let bytes = encode_camera_frame(640, 360, 7, 123_456, &jpeg);
        assert_eq!(bytes.len(), CAMERA_FRAME_HEADER + 4);
        let f = parse_camera_frame(&bytes).unwrap();
        assert_eq!(
            (f.width, f.height, f.seq, f.capture_us, f.jpeg),
            (640, 360, 7, 123_456, &jpeg[..])
        );

        assert!(parse_camera_frame(&bytes[..CAMERA_FRAME_HEADER]).is_err(), "no payload");
        let mut bad = bytes.clone();
        bad[0] = 2;
        assert!(parse_camera_frame(&bad).is_err());
        let mut bad = bytes.clone();
        bad[1] = 9;
        assert!(parse_camera_frame(&bad).is_err());
        assert!(parse_camera_frame(&encode_camera_frame(4000, 360, 1, 1, &jpeg)).is_err());
        assert!(parse_camera_frame(&encode_camera_frame(640, 100, 1, 1, &jpeg)).is_err());
        let mut bad = bytes.clone();
        bad[6] = 1;
        assert!(parse_camera_frame(&bad).is_err(), "reserved must be zero");
        let huge = encode_camera_frame(640, 360, 1, 1, &vec![0u8; MAX_CAMERA_PAYLOAD + 1]);
        assert_eq!(parse_camera_frame(&huge), Err(ProtocolError::TooLarge));
        assert!(parse_camera_frame(&encode_camera_frame(640, 360, 1, u64::MAX, &jpeg)).is_err());
    }

    #[test]
    fn accepts_protocol_one_and_two_only() {
        let mut h = Hello {
            protocol: 1,
            token: "t".into(),
            role: Role::Wiimote,
            device_id: "abcdefgh".into(),
            name: None,
        };
        assert!(h.validate().is_ok());
        h.protocol = 2;
        assert!(h.validate().is_ok());
        h.protocol = 3;
        assert_eq!(h.validate(), Err(ProtocolError::UnsupportedProtocol(3)));
        h.protocol = 0;
        assert!(h.validate().is_err());
    }

    #[test]
    fn tracking_message_serializes_per_spec() {
        let m = ServerMessage::Tracking {
            state: TrackingState::GyroOnly,
            pointer: Some(PointerXy { x: 0.5, y: 0.25 }),
            tags: 3,
            fixes_per_s: 12.5,
        };
        assert_eq!(
            m.to_json(),
            r#"{"type":"tracking","state":"gyro_only","pointer":{"x":0.5,"y":0.25},"tags":3,"fixes_per_s":12.5}"#
        );
    }

    #[test]
    fn server_messages_serialize_per_spec() {
        assert_eq!(
            ServerMessage::Welcome {
                protocol: 1,
                role: Role::Wiimote,
                slot: 1,
                player: 1
            }
            .to_json(),
            r#"{"type":"welcome","protocol":1,"role":"wiimote","slot":1,"player":1}"#
        );
        assert_eq!(ServerMessage::Pong { id: 7 }.to_json(), r#"{"type":"pong","id":7}"#);
        assert_eq!(
            ServerMessage::error(ErrorCode::RoleUnavailable, "full").to_json(),
            r#"{"type":"error","code":"role_unavailable","message":"full"}"#
        );
    }
}
