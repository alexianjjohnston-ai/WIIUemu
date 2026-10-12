//! WebSocket endpoint for phones (`/ws`).

use crate::session::SessionManager;
use axum::extract::ws::{CloseFrame, Message, WebSocket};
use std::sync::Arc;
use std::time::{Duration, Instant};
use wiiu_controller::protocol::{
    parse_camera_frame, parse_client_message, validate_gyro, ClientMessage, ErrorCode, Role, ServerMessage,
    MAX_CAMERA_FRAME_BYTES, MAX_FRAME_BYTES,
};

pub const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
/// Generous cap: phones send ~20–120 snapshots per second.
pub const MAX_MESSAGES_PER_SECOND: u32 = 500;
/// Camera frames accepted per second per phone (protocol v2); extra frames are dropped silently.
pub const MAX_CAMERA_FPS: u32 = 60;
/// Small delay before rejecting a bad token, to slow down guessing.
const BAD_TOKEN_DELAY: Duration = Duration::from_millis(300);

async fn send(socket: &mut WebSocket, msg: &ServerMessage) -> bool {
    socket.send(Message::Text(msg.to_json().into())).await.is_ok()
}

async fn fail(socket: &mut WebSocket, code: ErrorCode, message: impl Into<String>) {
    let message = message.into();
    tracing::info!("closing phone connection: {code:?}: {message}");
    let _ = send(socket, &ServerMessage::error(code, message)).await;
    let _ = socket
        .send(Message::Close(Some(CloseFrame {
            code: 1008, // policy violation
            reason: "".into(),
        })))
        .await;
}

enum Incoming {
    Text(String),
    Binary(Vec<u8>),
}

/// Reads the next data frame, enforcing size limits. `Ok(None)` = closed.
async fn next_message(socket: &mut WebSocket) -> Result<Option<Incoming>, &'static str> {
    loop {
        match socket.recv().await {
            None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return Ok(None),
            Some(Ok(Message::Text(t))) => {
                if t.len() > MAX_FRAME_BYTES {
                    return Err("frame too large");
                }
                return Ok(Some(Incoming::Text(t.to_string())));
            }
            Some(Ok(Message::Binary(b))) => {
                if b.len() > MAX_CAMERA_FRAME_BYTES {
                    return Err("frame too large");
                }
                return Ok(Some(Incoming::Binary(b.to_vec())));
            }
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
        }
    }
}

pub async fn handle(mut socket: WebSocket, sessions: Arc<SessionManager>, peer: std::net::SocketAddr) {
    // 1. hello
    let first = match tokio::time::timeout(HELLO_TIMEOUT, next_message(&mut socket)).await {
        Err(_) => return fail(&mut socket, ErrorCode::Timeout, "no hello within 5 s").await,
        Ok(Err(e)) => return fail(&mut socket, ErrorCode::InvalidMessage, e).await,
        Ok(Ok(None)) => return,
        Ok(Ok(Some(Incoming::Binary(_)))) => {
            return fail(&mut socket, ErrorCode::InvalidMessage, "first message must be hello").await
        }
        Ok(Ok(Some(Incoming::Text(t)))) => t,
    };
    let hello = match parse_client_message(&first) {
        Ok(ClientMessage::Hello(h)) => h,
        Ok(_) => return fail(&mut socket, ErrorCode::InvalidMessage, "first message must be hello").await,
        Err(e) => return fail(&mut socket, e.code(), e.to_string()).await,
    };
    let assignment = match sessions.authenticate(&hello, Instant::now()) {
        Ok(a) => a,
        Err((code, msg)) => {
            if code == ErrorCode::BadToken {
                tokio::time::sleep(BAD_TOKEN_DELAY).await;
            }
            return fail(&mut socket, code, msg).await;
        }
    };
    let (conn_id, slot) = (assignment.conn_id, assignment.slot);
    tracing::info!(
        "phone {peer} ({}) connected as {:?} player {} (slot {slot})",
        hello.name.as_deref().unwrap_or("unnamed"),
        assignment.role,
        assignment.player
    );
    let welcome = ServerMessage::Welcome {
        protocol: hello.protocol,
        role: assignment.role,
        slot: slot as u8,
        player: assignment.player,
    };
    if send(&mut socket, &welcome).await {
        let ctx = SessionCtx {
            sessions: sessions.clone(),
            conn_id,
            slot,
            role: assignment.role,
            protocol: hello.protocol,
        };
        run_session(&mut socket, ctx, assignment.replaced).await;
    }
    sessions.disconnect(conn_id, slot);
    tracing::info!("phone {peer} disconnected from slot {slot}");
}

struct SessionCtx {
    sessions: Arc<SessionManager>,
    conn_id: u64,
    slot: usize,
    role: Role,
    protocol: u32,
}

/// Simple fixed-window counter.
struct RateWindow {
    start: Instant,
    count: u32,
}

impl RateWindow {
    fn new() -> Self {
        Self {
            start: Instant::now(),
            count: 0,
        }
    }

    /// Counts one event; returns `false` once more than `limit` happened in the current second.
    fn allow(&mut self, now: Instant, limit: u32) -> bool {
        if now.duration_since(self.start) >= Duration::from_secs(1) {
            self.start = now;
            self.count = 0;
        }
        self.count += 1;
        self.count <= limit
    }
}

async fn run_session(socket: &mut WebSocket, ctx: SessionCtx, mut replaced: tokio::sync::oneshot::Receiver<()>) {
    let mut messages = RateWindow::new();
    let mut frames = RateWindow::new();
    let mut last_frame_seq: Option<u32> = None;
    let mut last_gyro_t: Option<u64> = None;
    let tracking_allowed = ctx.protocol >= 2 && ctx.role == Role::Wiimote;
    let (status_tx, mut status_rx) = tokio::sync::mpsc::unbounded_channel::<ServerMessage>();
    let mut tracker: Option<crate::tracking::TrackingHandle> = None;
    let ensure_tracker = |tracker: &mut Option<crate::tracking::TrackingHandle>| {
        if tracker.is_none() {
            match crate::tracking::spawn(ctx.sessions.clone(), ctx.conn_id, ctx.slot, status_tx.clone()) {
                Ok(h) => *tracker = Some(h),
                Err(e) => tracing::error!("slot {}: cannot start tracking worker: {e}", ctx.slot),
            }
        }
    };
    loop {
        let incoming = tokio::select! {
            _ = &mut replaced => {
                return fail(socket, ErrorCode::Replaced, "this device connected again").await;
            }
            Some(status) = status_rx.recv() => {
                if !send(socket, &status).await {
                    return;
                }
                continue;
            }
            m = next_message(socket) => m,
        };
        let incoming = match incoming {
            Ok(Some(m)) => m,
            Ok(None) => return,
            Err(e) => return fail(socket, ErrorCode::InvalidMessage, e).await,
        };

        let now = Instant::now();
        if !messages.allow(now, MAX_MESSAGES_PER_SECOND) {
            return fail(socket, ErrorCode::InvalidMessage, "message rate limit exceeded").await;
        }

        let text = match incoming {
            Incoming::Binary(bytes) => {
                if !tracking_allowed {
                    return fail(socket, ErrorCode::InvalidMessage, "binary frames are not supported").await;
                }
                let frame = match parse_camera_frame(&bytes) {
                    Ok(f) => f,
                    Err(e) => return fail(socket, e.code(), e.to_string()).await,
                };
                if last_frame_seq.is_some_and(|s| frame.seq <= s) {
                    return fail(socket, ErrorCode::InvalidMessage, "camera frame seq must increase").await;
                }
                last_frame_seq = Some(frame.seq);
                if frames.allow(now, MAX_CAMERA_FPS) {
                    ensure_tracker(&mut tracker);
                    if let Some(t) = &tracker {
                        t.submit(crate::tracking::Work::Frame {
                            width: frame.width,
                            height: frame.height,
                            capture_us: frame.capture_us,
                            jpeg: frame.jpeg.to_vec(),
                        });
                    }
                }
                continue;
            }
            Incoming::Text(t) => t,
        };

        match parse_client_message(&text) {
            Ok(ClientMessage::Input(raw)) => {
                if let Err(e) = ctx.sessions.apply_input(ctx.conn_id, ctx.slot, &raw, now) {
                    return fail(socket, e.code(), e.to_string()).await;
                }
            }
            Ok(ClientMessage::Gyro { samples }) => {
                if !tracking_allowed {
                    return fail(
                        socket,
                        ErrorCode::InvalidMessage,
                        "gyro requires protocol 2 and the wiimote role",
                    )
                    .await;
                }
                let samples = match validate_gyro(&samples, last_gyro_t) {
                    Ok(s) => s,
                    Err(e) => return fail(socket, e.code(), e.to_string()).await,
                };
                last_gyro_t = samples.last().map(|s| s.t_us);
                ensure_tracker(&mut tracker);
                if let Some(t) = &tracker {
                    t.submit(crate::tracking::Work::Gyro(samples));
                }
            }
            Ok(ClientMessage::Ping { id }) => {
                if !send(socket, &ServerMessage::Pong { id }).await {
                    return;
                }
            }
            Ok(ClientMessage::Hello(_)) => {
                return fail(socket, ErrorCode::InvalidMessage, "already authenticated").await;
            }
            Err(e) => return fail(socket, e.code(), e.to_string()).await,
        }
    }
}
