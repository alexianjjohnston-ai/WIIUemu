//! Phone ↔ PC protocol v1 (JSON over WebSocket). Specification: `docs/controller-protocol.md`.

use crate::buttons::{ButtonSet, GamepadButton, WiimoteButton};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const PROTOCOL_VERSION: u32 = 1;
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
    Ping { id: u64 },
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
    Error {
        code: ErrorCode,
        message: String,
    },
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
        if self.protocol != PROTOCOL_VERSION {
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

#[cfg(test)]
mod tests {
    use super::*;

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
                protocol: 2,
                ..base.clone()
            }
            .validate(),
            Err(ProtocolError::UnsupportedProtocol(2))
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
