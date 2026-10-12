//! WebSocket endpoint for phones (`/ws`).

use crate::session::SessionManager;
use axum::extract::ws::{CloseFrame, Message, WebSocket};
use std::sync::Arc;
use std::time::{Duration, Instant};
use wiiu_controller::protocol::{
    parse_client_message, ClientMessage, ErrorCode, ServerMessage, MAX_FRAME_BYTES, PROTOCOL_VERSION,
};

pub const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
/// Generous cap: phones send ~20–120 snapshots per second.
pub const MAX_MESSAGES_PER_SECOND: u32 = 500;
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

/// Reads the next text frame, enforcing size limits. `Ok(None)` = closed.
async fn next_text(socket: &mut WebSocket) -> Result<Option<String>, &'static str> {
    loop {
        match socket.recv().await {
            None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return Ok(None),
            Some(Ok(Message::Text(t))) => {
                if t.len() > MAX_FRAME_BYTES {
                    return Err("frame too large");
                }
                return Ok(Some(t.to_string()));
            }
            Some(Ok(Message::Binary(_))) => return Err("binary frames are not supported"),
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
        }
    }
}

pub async fn handle(mut socket: WebSocket, sessions: Arc<SessionManager>, peer: std::net::SocketAddr) {
    // 1. hello
    let first = match tokio::time::timeout(HELLO_TIMEOUT, next_text(&mut socket)).await {
        Err(_) => return fail(&mut socket, ErrorCode::Timeout, "no hello within 5 s").await,
        Ok(Err(e)) => return fail(&mut socket, ErrorCode::InvalidMessage, e).await,
        Ok(Ok(None)) => return,
        Ok(Ok(Some(t))) => t,
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
        protocol: PROTOCOL_VERSION,
        role: assignment.role,
        slot: slot as u8,
        player: assignment.player,
    };
    if send(&mut socket, &welcome).await {
        run_session(&mut socket, &sessions, conn_id, slot, assignment.replaced).await;
    }
    sessions.disconnect(conn_id, slot);
    tracing::info!("phone {peer} disconnected from slot {slot}");
}

async fn run_session(
    socket: &mut WebSocket,
    sessions: &SessionManager,
    conn_id: u64,
    slot: usize,
    mut replaced: tokio::sync::oneshot::Receiver<()>,
) {
    let mut window_start = Instant::now();
    let mut window_count = 0u32;
    loop {
        let text = tokio::select! {
            _ = &mut replaced => {
                return fail(socket, ErrorCode::Replaced, "this device connected again").await;
            }
            t = next_text(socket) => t,
        };
        let text = match text {
            Ok(Some(t)) => t,
            Ok(None) => return,
            Err(e) => return fail(socket, ErrorCode::InvalidMessage, e).await,
        };

        let now = Instant::now();
        if now.duration_since(window_start) >= Duration::from_secs(1) {
            window_start = now;
            window_count = 0;
        }
        window_count += 1;
        if window_count > MAX_MESSAGES_PER_SECOND {
            return fail(socket, ErrorCode::InvalidMessage, "message rate limit exceeded").await;
        }

        match parse_client_message(&text) {
            Ok(ClientMessage::Input(raw)) => {
                if let Err(e) = sessions.apply_input(conn_id, slot, &raw, now) {
                    return fail(socket, e.code(), e.to_string()).await;
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
