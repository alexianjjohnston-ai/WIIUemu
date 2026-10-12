//! Slot assignment, authentication and input safety rules for connected phones.
//!
//! All state transitions live here, independent of sockets, so the safety rules in
//! `docs/controller-protocol.md` ("disconnect releases everything", staleness watchdog, reconnect, isolation)
//! are unit-testable with an explicit clock.

use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;
use wiiu_controller::protocol::{ControllerInput, ErrorCode, Hello, ProtocolError, RawInput, Role};
use wiiu_controller::state::{to_pad, MotionClock};
use wiiu_dsu::{PadData, SlotTable, MAX_SLOTS};

pub const STALE_AFTER: Duration = Duration::from_millis(1000);

/// Role and human-facing player number of each DSU slot (see docs/controller-protocol.md).
pub const fn slot_layout(slot: usize) -> (Role, u8) {
    match slot {
        0 => (Role::Gamepad, 0),
        n => (Role::Wiimote, n as u8),
    }
}

#[derive(Debug)]
struct Owner {
    conn_id: u64,
    device_id: String,
    name: Option<String>,
    kick: Option<oneshot::Sender<()>>,
}

#[derive(Debug)]
struct SlotSession {
    owner: Option<Owner>,
    /// Device that last held this slot; it may reclaim the slot on reconnect.
    last_device: Option<String>,
    last_seq: Option<u64>,
    last_input: Instant,
    stale: bool,
    input: ControllerInput,
    motion_clock: MotionClock,
}

#[derive(Debug)]
pub struct Assignment {
    pub conn_id: u64,
    pub slot: usize,
    pub role: Role,
    pub player: u8,
    /// Resolves when another connection with the same device id takes over this slot.
    pub replaced: oneshot::Receiver<()>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SlotStatus {
    pub slot: usize,
    pub role: Role,
    pub player: u8,
    pub connected: bool,
    pub device_name: Option<String>,
    pub stale: bool,
    pub buttons: u32,
    pub pointer: Option<(f32, f32)>,
}

pub struct SessionManager {
    token: String,
    table: SlotTable,
    epoch: Instant,
    inner: Mutex<Inner>,
}

struct Inner {
    next_conn_id: u64,
    slots: Vec<SlotSession>,
}

/// Compares two strings without an early exit on the first differing byte.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

impl SessionManager {
    pub fn new(token: String, table: SlotTable) -> Self {
        let now = Instant::now();
        Self {
            token,
            table,
            epoch: now,
            inner: Mutex::new(Inner {
                next_conn_id: 1,
                slots: (0..MAX_SLOTS)
                    .map(|_| SlotSession {
                        owner: None,
                        last_device: None,
                        last_seq: None,
                        last_input: now,
                        stale: false,
                        input: ControllerInput::default(),
                        motion_clock: MotionClock::default(),
                    })
                    .collect(),
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    /// Validates a `hello` and binds the connection to a slot.
    pub fn authenticate(&self, hello: &Hello, now: Instant) -> Result<Assignment, (ErrorCode, String)> {
        hello.validate().map_err(|e| (e.code(), e.to_string()))?;
        if !constant_time_eq(&hello.token, &self.token) {
            return Err((ErrorCode::BadToken, "pairing token rejected".into()));
        }
        let mut inner = self.lock();
        let candidates: Vec<usize> = (0..MAX_SLOTS).filter(|&s| slot_layout(s).0 == hello.role).collect();

        // Prefer the slot this device held before (reconnect), then the first free slot.
        let slot = candidates
            .iter()
            .copied()
            .find(|&s| {
                let sess = &inner.slots[s];
                sess.last_device.as_deref() == Some(hello.device_id.as_str())
                    && sess.owner.as_ref().is_none_or(|o| o.device_id == hello.device_id)
            })
            .or_else(|| candidates.iter().copied().find(|&s| inner.slots[s].owner.is_none()))
            .ok_or_else(|| {
                (
                    ErrorCode::RoleUnavailable,
                    format!("all {:?} slots are in use", hello.role),
                )
            })?;

        let conn_id = inner.next_conn_id;
        inner.next_conn_id += 1;
        let (kick_tx, kick_rx) = oneshot::channel();
        let sess = &mut inner.slots[slot];
        if let Some(mut old) = sess.owner.take() {
            if let Some(k) = old.kick.take() {
                let _ = k.send(());
            }
        }
        sess.owner = Some(Owner {
            conn_id,
            device_id: hello.device_id.clone(),
            name: hello.name.clone(),
            kick: Some(kick_tx),
        });
        sess.last_device = Some(hello.device_id.clone());
        sess.last_seq = None;
        sess.last_input = now;
        sess.stale = false;
        sess.input = ControllerInput::default();
        sess.motion_clock.reset_session();
        drop(inner);
        self.table.set_pad(slot, PadData::neutral());

        let (role, player) = slot_layout(slot);
        Ok(Assignment {
            conn_id,
            slot,
            role,
            player,
            replaced: kick_rx,
        })
    }

    /// Applies an `input` snapshot from connection `conn_id`. Snapshots from a connection that no longer owns
    /// the slot are ignored. Returns an error only for invalid content.
    pub fn apply_input(&self, conn_id: u64, slot: usize, raw: &RawInput, now: Instant) -> Result<(), ProtocolError> {
        let (role, _) = slot_layout(slot);
        let input = raw.validate(role)?;
        let server_us = now.saturating_duration_since(self.epoch).as_micros() as u64;
        let mut inner = self.lock();
        let Some(sess) = inner.slots.get_mut(slot) else {
            return Ok(());
        };
        if sess.owner.as_ref().map(|o| o.conn_id) != Some(conn_id) {
            return Ok(());
        }
        if sess.last_seq.is_some_and(|last| input.seq <= last) {
            return Ok(()); // stale or duplicate snapshot
        }
        sess.last_seq = Some(input.seq);
        sess.last_input = now;
        sess.stale = false;
        sess.input = input;
        let motion = input.motion.map(|m| sess.motion_clock.map(&m, server_us));
        drop(inner);
        self.table.set_pad(slot, to_pad(role, &input, motion));
        Ok(())
    }

    /// Releases the slot if `conn_id` still owns it: buttons up, sticks centred, pointer hidden.
    pub fn disconnect(&self, conn_id: u64, slot: usize) {
        let mut inner = self.lock();
        let Some(sess) = inner.slots.get_mut(slot) else {
            return;
        };
        if sess.owner.as_ref().map(|o| o.conn_id) != Some(conn_id) {
            return;
        }
        sess.owner = None;
        sess.input = ControllerInput::default();
        sess.stale = false;
        drop(inner);
        self.table.set_pad(slot, PadData::neutral());
    }

    /// Neutralises slots whose phone stopped sending input. Returns the slots neutralised by this call.
    pub fn watchdog(&self, now: Instant) -> Vec<usize> {
        let mut neutralised = Vec::new();
        let mut inner = self.lock();
        for (i, sess) in inner.slots.iter_mut().enumerate() {
            if sess.owner.is_some() && !sess.stale && now.saturating_duration_since(sess.last_input) >= STALE_AFTER {
                sess.stale = true;
                sess.input = ControllerInput::default();
                neutralised.push(i);
            }
        }
        drop(inner);
        for &i in &neutralised {
            self.table.set_pad(i, PadData::neutral());
        }
        neutralised
    }

    pub fn status(&self) -> Vec<SlotStatus> {
        let inner = self.lock();
        inner
            .slots
            .iter()
            .enumerate()
            .map(|(slot, s)| {
                let (role, player) = slot_layout(slot);
                SlotStatus {
                    slot,
                    role,
                    player,
                    connected: s.owner.is_some(),
                    device_name: s.owner.as_ref().and_then(|o| o.name.clone()),
                    stale: s.stale,
                    buttons: s.input.buttons.0,
                    pointer: s.input.pointer,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiiu_controller::protocol::Xy;
    use wiiu_dsu::pad::buttons as dsu;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    fn manager() -> (SessionManager, SlotTable) {
        let table = SlotTable::new();
        (SessionManager::new(TOKEN.into(), table.clone()), table)
    }

    fn hello(role: Role, device: &str) -> Hello {
        Hello {
            protocol: 1,
            token: TOKEN.into(),
            role,
            device_id: device.into(),
            name: Some(format!("phone {device}")),
        }
    }

    fn input(seq: u64, buttons: &[&str], pointer: Option<(f64, f64)>) -> RawInput {
        RawInput {
            seq,
            buttons: buttons.iter().map(|s| s.to_string()).collect(),
            pointer: pointer.map(|(x, y)| Xy { x, y }),
            stick: None,
            rstick: None,
            motion: None,
        }
    }

    #[test]
    fn rejects_wrong_token_and_bad_protocol() {
        let (m, _) = manager();
        let now = Instant::now();
        let mut h = hello(Role::Wiimote, "device-aaaa");
        h.token = "wrong".into();
        assert_eq!(m.authenticate(&h, now).unwrap_err().0, ErrorCode::BadToken);
        let mut h = hello(Role::Wiimote, "device-aaaa");
        h.token = TOKEN.replace('0', "1");
        assert_eq!(m.authenticate(&h, now).unwrap_err().0, ErrorCode::BadToken);
        let mut h = hello(Role::Wiimote, "device-aaaa");
        h.protocol = 9;
        assert_eq!(m.authenticate(&h, now).unwrap_err().0, ErrorCode::UnsupportedProtocol);
    }

    #[test]
    fn assigns_three_wiimotes_and_one_gamepad() {
        let (m, _) = manager();
        let now = Instant::now();
        let a = m.authenticate(&hello(Role::Wiimote, "device-aaaa"), now).unwrap();
        let b = m.authenticate(&hello(Role::Wiimote, "device-bbbb"), now).unwrap();
        let c = m.authenticate(&hello(Role::Wiimote, "device-cccc"), now).unwrap();
        assert_eq!((a.slot, a.player), (1, 1));
        assert_eq!((b.slot, b.player), (2, 2));
        assert_eq!((c.slot, c.player), (3, 3));
        let err = m.authenticate(&hello(Role::Wiimote, "device-dddd"), now).unwrap_err();
        assert_eq!(err.0, ErrorCode::RoleUnavailable);
        let g = m.authenticate(&hello(Role::Gamepad, "device-gggg"), now).unwrap();
        assert_eq!((g.slot, g.player, g.role), (0, 0, Role::Gamepad));
    }

    #[test]
    fn input_reaches_only_its_own_slot() {
        let (m, table) = manager();
        let now = Instant::now();
        let a = m.authenticate(&hello(Role::Wiimote, "device-aaaa"), now).unwrap();
        let b = m.authenticate(&hello(Role::Wiimote, "device-bbbb"), now).unwrap();
        m.apply_input(a.conn_id, a.slot, &input(1, &["A"], Some((0.5, 0.5))), now)
            .unwrap();
        m.apply_input(b.conn_id, b.slot, &input(1, &["B"], None), now).unwrap();
        assert_eq!(table.pad(1).unwrap().buttons, dsu::CROSS);
        assert!(table.pad(1).unwrap().touch[0].is_some());
        assert_eq!(table.pad(2).unwrap().buttons, dsu::CIRCLE);
        assert!(table.pad(2).unwrap().touch[0].is_none());
        // Connection A cannot write to B's slot even if it names it.
        m.apply_input(a.conn_id, b.slot, &input(2, &["HOME"], None), now)
            .unwrap();
        assert_eq!(table.pad(2).unwrap().buttons, dsu::CIRCLE);
    }

    #[test]
    fn drops_out_of_order_snapshots() {
        let (m, table) = manager();
        let now = Instant::now();
        let a = m.authenticate(&hello(Role::Wiimote, "device-aaaa"), now).unwrap();
        m.apply_input(a.conn_id, a.slot, &input(5, &["A"], None), now).unwrap();
        m.apply_input(a.conn_id, a.slot, &input(4, &[], None), now).unwrap();
        m.apply_input(a.conn_id, a.slot, &input(5, &[], None), now).unwrap();
        assert_eq!(table.pad(1).unwrap().buttons, dsu::CROSS);
        m.apply_input(a.conn_id, a.slot, &input(6, &[], None), now).unwrap();
        assert_eq!(table.pad(1).unwrap().buttons, 0);
    }

    #[test]
    fn disconnect_releases_everything() {
        let (m, table) = manager();
        let now = Instant::now();
        let a = m.authenticate(&hello(Role::Wiimote, "device-aaaa"), now).unwrap();
        m.apply_input(a.conn_id, a.slot, &input(1, &["A", "B"], Some((0.1, 0.9))), now)
            .unwrap();
        assert_ne!(table.pad(1).unwrap(), PadData::neutral());
        m.disconnect(a.conn_id, a.slot);
        assert_eq!(table.pad(1).unwrap(), PadData::neutral());
        assert!(!m.status()[1].connected);
    }

    #[test]
    fn watchdog_neutralises_silent_phone_and_recovers() {
        let (m, table) = manager();
        let t0 = Instant::now();
        let a = m.authenticate(&hello(Role::Wiimote, "device-aaaa"), t0).unwrap();
        m.apply_input(a.conn_id, a.slot, &input(1, &["B"], None), t0).unwrap();
        assert!(m.watchdog(t0 + Duration::from_millis(500)).is_empty());
        assert_eq!(table.pad(1).unwrap().buttons, dsu::CIRCLE);
        assert_eq!(m.watchdog(t0 + Duration::from_millis(1001)), vec![1]);
        assert_eq!(table.pad(1).unwrap(), PadData::neutral());
        assert!(m.status()[1].stale);
        // Only reported once.
        assert!(m.watchdog(t0 + Duration::from_millis(1500)).is_empty());
        // Fresh input resumes control.
        m.apply_input(
            a.conn_id,
            a.slot,
            &input(2, &["A"], None),
            t0 + Duration::from_millis(1600),
        )
        .unwrap();
        assert_eq!(table.pad(1).unwrap().buttons, dsu::CROSS);
        assert!(!m.status()[1].stale);
    }

    #[tokio::test]
    async fn reconnecting_device_reclaims_slot_and_kicks_old_socket() {
        let (m, table) = manager();
        let now = Instant::now();
        let _a = m.authenticate(&hello(Role::Wiimote, "device-aaaa"), now).unwrap();
        let b1 = m.authenticate(&hello(Role::Wiimote, "device-bbbb"), now).unwrap();
        m.apply_input(b1.conn_id, b1.slot, &input(10, &["A"], None), now)
            .unwrap();

        // Same device connects again while the old socket is still open.
        let b2 = m.authenticate(&hello(Role::Wiimote, "device-bbbb"), now).unwrap();
        assert_eq!(b2.slot, b1.slot);
        b1.replaced.await.expect("old connection is told it was replaced");
        // Fresh session: neutral, and seq restarts.
        assert_eq!(table.pad(2).unwrap(), PadData::neutral());
        m.apply_input(b2.conn_id, b2.slot, &input(1, &["TWO"], None), now)
            .unwrap();
        assert_eq!(table.pad(2).unwrap().buttons, dsu::TRIANGLE);
        // Old connection's late messages and disconnect are ignored.
        m.apply_input(b1.conn_id, b1.slot, &input(99, &[], None), now).unwrap();
        m.disconnect(b1.conn_id, b1.slot);
        assert_eq!(table.pad(2).unwrap().buttons, dsu::TRIANGLE);
    }

    #[test]
    fn device_gets_previous_slot_back_after_disconnect() {
        let (m, _) = manager();
        let now = Instant::now();
        let a = m.authenticate(&hello(Role::Wiimote, "device-aaaa"), now).unwrap();
        let b = m.authenticate(&hello(Role::Wiimote, "device-bbbb"), now).unwrap();
        m.disconnect(a.conn_id, a.slot);
        m.disconnect(b.conn_id, b.slot);
        let b2 = m.authenticate(&hello(Role::Wiimote, "device-bbbb"), now).unwrap();
        assert_eq!(b2.slot, 2, "player 2 stays player 2");
        let a2 = m.authenticate(&hello(Role::Wiimote, "device-aaaa"), now).unwrap();
        assert_eq!(a2.slot, 1);
    }

    #[test]
    fn invalid_input_is_an_error_and_changes_nothing() {
        let (m, table) = manager();
        let now = Instant::now();
        let a = m.authenticate(&hello(Role::Wiimote, "device-aaaa"), now).unwrap();
        m.apply_input(a.conn_id, a.slot, &input(1, &["A"], None), now).unwrap();
        assert!(m.apply_input(a.conn_id, a.slot, &input(2, &["ZR"], None), now).is_err());
        assert_eq!(table.pad(1).unwrap().buttons, dsu::CROSS);
    }

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "abcd"));
    }
}
