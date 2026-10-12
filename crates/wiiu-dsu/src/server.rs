//! Async DSU UDP server.
//!
//! Cemu sends a pad-data request after every packet it receives for a slot, so subscriptions renew themselves
//! while Cemu is running. Subscriptions that are not renewed expire after [`SUBSCRIPTION_TIMEOUT`].

use crate::pad::PadData;
use crate::protocol::{
    encode_pad_data, encode_port_info, encode_version_response, parse_client_packet, ClientRequest, PadDataRequest,
    SlotInfo, MAX_SLOTS,
};
use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::Notify;

pub const DEFAULT_PORT: u16 = 26760;
pub const SUBSCRIPTION_TIMEOUT: Duration = Duration::from_secs(5);
/// Upper bound on concurrent subscribers, so a misbehaving local process can't make us fan out unboundedly.
pub const MAX_SUBSCRIBERS: usize = 16;
/// Data is pushed immediately on change and re-sent at least this often.
pub const REFRESH_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, Copy)]
struct Slot {
    info: SlotInfo,
    pad: PadData,
}

/// Thread-safe slot table shared between the session layer (writer) and the DSU server (reader).
#[derive(Clone)]
pub struct SlotTable {
    inner: Arc<SlotTableInner>,
}

struct SlotTableInner {
    slots: Mutex<[Slot; MAX_SLOTS]>,
    changed: Notify,
}

impl Default for SlotTable {
    fn default() -> Self {
        Self::new()
    }
}

impl SlotTable {
    /// All four slots connected with neutral input.
    pub fn new() -> Self {
        let slots = std::array::from_fn(|i| Slot {
            info: SlotInfo::virtual_connected(i as u8),
            pad: PadData::neutral(),
        });
        Self {
            inner: Arc::new(SlotTableInner {
                slots: Mutex::new(slots),
                changed: Notify::new(),
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, [Slot; MAX_SLOTS]> {
        // A panic while holding this lock can only come from a bug in this module; the data is plain-old-data,
        // so continuing with the last written state is safe.
        self.inner.slots.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Replaces the pad state of `slot`. Out-of-range slots are ignored.
    pub fn set_pad(&self, slot: usize, pad: PadData) {
        if slot >= MAX_SLOTS {
            return;
        }
        self.lock()[slot].pad = pad;
        self.inner.changed.notify_one();
    }

    pub fn pad(&self, slot: usize) -> Option<PadData> {
        (slot < MAX_SLOTS).then(|| self.lock()[slot].pad)
    }

    pub fn info(&self, slot: usize) -> Option<SlotInfo> {
        (slot < MAX_SLOTS).then(|| self.lock()[slot].info)
    }

    fn snapshot(&self) -> [Slot; MAX_SLOTS] {
        *self.lock()
    }
}

#[derive(Debug)]
struct Subscription {
    slots: u8,
    last_request: Instant,
}

pub struct DsuServer {
    socket: UdpSocket,
    table: SlotTable,
    server_id: u32,
    subscribers: HashMap<SocketAddr, Subscription>,
    packet_numbers: [u32; MAX_SLOTS],
}

impl DsuServer {
    pub async fn bind(addr: SocketAddr, table: SlotTable) -> io::Result<Self> {
        let socket = UdpSocket::bind(addr).await?;
        Ok(Self {
            socket,
            table,
            server_id: random_u32(),
            subscribers: HashMap::new(),
            packet_numbers: [0; MAX_SLOTS],
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    /// Runs until `shutdown` resolves.
    pub async fn run(mut self, shutdown: impl Future<Output = ()>) {
        tokio::pin!(shutdown);
        let mut buf = [0u8; 512];
        let mut refresh = tokio::time::interval(REFRESH_INTERVAL);
        refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let table = self.table.clone();
        loop {
            tokio::select! {
                _ = &mut shutdown => break,
                res = self.socket.recv_from(&mut buf) => match res {
                    Ok((len, from)) => self.handle_datagram(&buf[..len], from).await,
                    // On Windows a previous send to a closed port surfaces here as WSAECONNRESET; it is not fatal.
                    Err(e) => tracing::debug!("dsu recv error: {e}"),
                },
                _ = table.inner.changed.notified() => self.broadcast().await,
                _ = refresh.tick() => self.broadcast().await,
            }
        }
    }

    async fn handle_datagram(&mut self, data: &[u8], from: SocketAddr) {
        let packet = match parse_client_packet(data) {
            Ok(p) => p,
            Err(e) => {
                tracing::trace!("ignoring invalid dsu packet from {from}: {e}");
                return;
            }
        };
        match packet.request {
            ClientRequest::Version => {
                self.send(&encode_version_response(self.server_id), from).await;
            }
            ClientRequest::PortInfo { slots } => {
                for slot in slots.into_iter().filter(|&s| (s as usize) < MAX_SLOTS) {
                    if let Some(info) = self.table.info(slot as usize) {
                        self.send(&encode_port_info(self.server_id, &info), from).await;
                    }
                }
            }
            ClientRequest::PadData(req) => {
                let mask = self.request_mask(req);
                if mask == 0 {
                    return;
                }
                self.expire_subscribers();
                if !self.subscribers.contains_key(&from) && self.subscribers.len() >= MAX_SUBSCRIBERS {
                    tracing::warn!("dsu subscriber limit reached; ignoring {from}");
                    return;
                }
                let is_new = !self.subscribers.contains_key(&from);
                let sub = self.subscribers.entry(from).or_insert(Subscription {
                    slots: 0,
                    last_request: Instant::now(),
                });
                sub.slots |= mask;
                sub.last_request = Instant::now();
                if is_new {
                    tracing::info!("dsu client {from} subscribed (slots mask {mask:#06b})");
                    // Answer straight away so the client sees the slot as connected without waiting for a tick.
                    self.send_slots(from, mask).await;
                }
            }
        }
    }

    fn request_mask(&self, req: PadDataRequest) -> u8 {
        let mac_mask = |mac: [u8; 6]| {
            (0..MAX_SLOTS)
                .filter(|&i| self.table.info(i).is_some_and(|info| info.mac == mac))
                .fold(0u8, |m, i| m | (1 << i))
        };
        match req {
            PadDataRequest::AllSlots => (1u8 << MAX_SLOTS) - 1,
            PadDataRequest::Slot(s) if (s as usize) < MAX_SLOTS => 1 << s,
            PadDataRequest::Slot(_) => 0,
            PadDataRequest::Mac(mac) => mac_mask(mac),
            PadDataRequest::SlotAndMac(s, mac) => {
                let slot_mask = if (s as usize) < MAX_SLOTS { 1 << s } else { 0 };
                slot_mask | mac_mask(mac)
            }
        }
    }

    fn expire_subscribers(&mut self) {
        let now = Instant::now();
        self.subscribers.retain(|addr, sub| {
            let keep = now.duration_since(sub.last_request) < SUBSCRIPTION_TIMEOUT;
            if !keep {
                tracing::info!("dsu client {addr} subscription expired");
            }
            keep
        });
    }

    async fn broadcast(&mut self) {
        self.expire_subscribers();
        if self.subscribers.is_empty() {
            return;
        }
        let slots = self.table.snapshot();
        let targets: Vec<(SocketAddr, u8)> = self.subscribers.iter().map(|(a, s)| (*a, s.slots)).collect();
        for (addr, mask) in targets {
            self.send_slots_from(addr, mask, &slots).await;
        }
    }

    async fn send_slots(&mut self, addr: SocketAddr, mask: u8) {
        let slots = self.table.snapshot();
        self.send_slots_from(addr, mask, &slots).await;
    }

    async fn send_slots_from(&mut self, addr: SocketAddr, mask: u8, slots: &[Slot; MAX_SLOTS]) {
        for (i, slot) in slots.iter().enumerate() {
            if mask & (1 << i) == 0 {
                continue;
            }
            self.packet_numbers[i] = self.packet_numbers[i].wrapping_add(1);
            let pkt = encode_pad_data(self.server_id, &slot.info, self.packet_numbers[i], &slot.pad);
            self.send(&pkt, addr).await;
        }
    }

    async fn send(&self, data: &[u8], to: SocketAddr) {
        if let Err(e) = self.socket.send_to(data, to).await {
            tracing::debug!("dsu send to {to} failed: {e}");
        }
    }
}

/// Server id only needs to differ between server restarts; it is not security relevant.
fn random_u32() -> u32 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default(),
    );
    h.finish() as u32
}
