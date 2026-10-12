//! A DSU client that behaves like Cemu's `DSUControllerProvider`: it requests the version and port info,
//! then re-requests pad data for a slot after every packet it receives for that slot.
//!
//! It is used by the integration tests and by `wiiu-connect probe` for manual verification without Cemu.

use crate::protocol::{client, parse_server_packet, PadDataRequest, ServerMessage};
use std::io;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;

pub struct DsuProbe {
    socket: UdpSocket,
    client_id: u32,
}

impl DsuProbe {
    pub async fn connect(server: SocketAddr) -> io::Result<Self> {
        let bind: SocketAddr = if server.is_ipv4() {
            "0.0.0.0:0".parse().expect("valid addr")
        } else {
            "[::]:0".parse().expect("valid addr")
        };
        let socket = UdpSocket::bind(bind).await?;
        socket.connect(server).await?;
        Ok(Self {
            socket,
            client_id: 0x5745_5543, // arbitrary; Cemu uses rand()
        })
    }

    pub async fn send_version_request(&self) -> io::Result<()> {
        self.socket
            .send(&client::version_request(self.client_id))
            .await
            .map(|_| ())
    }

    pub async fn send_port_info_request(&self, slots: &[u8]) -> io::Result<()> {
        self.socket
            .send(&client::port_info_request(self.client_id, slots))
            .await
            .map(|_| ())
    }

    pub async fn send_pad_request(&self, req: PadDataRequest) -> io::Result<()> {
        self.socket
            .send(&client::pad_data_request(self.client_id, req))
            .await
            .map(|_| ())
    }

    /// Receives the next valid server message. Invalid datagrams are skipped, like Cemu does.
    pub async fn recv(&self, timeout: Duration) -> io::Result<ServerMessage> {
        let mut buf = [0u8; 100]; // Cemu's receive buffer is exactly 100 bytes
        tokio::time::timeout(timeout, async {
            loop {
                let len = self.socket.recv(&mut buf).await?;
                if let Ok(msg) = parse_server_packet(&buf[..len]) {
                    return Ok(msg);
                }
            }
        })
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "no dsu packet received"))?
    }

    /// Receives messages until a pad-data packet for `slot` matches `pred`, re-requesting data after each
    /// packet as Cemu does.
    pub async fn wait_for_pad(
        &self,
        slot: u8,
        timeout: Duration,
        mut pred: impl FnMut(&crate::PadData) -> bool,
    ) -> io::Result<crate::PadData> {
        let deadline = tokio::time::Instant::now() + timeout;
        self.send_pad_request(PadDataRequest::Slot(slot)).await?;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "pad state never matched"));
            }
            if let ServerMessage::PadData { info, pad, .. } = self.recv(remaining).await? {
                if info.slot == slot {
                    self.send_pad_request(PadDataRequest::Slot(slot)).await?;
                    if pred(&pad) {
                        return Ok(pad);
                    }
                }
            }
        }
    }
}
