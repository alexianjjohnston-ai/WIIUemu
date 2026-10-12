//! Full path: phone (WebSocket) → session server → DSU → Cemu-like client.

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::time::Duration;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use wiiu_connect::{start, Running, ServerConfig};
use wiiu_dsu::pad::buttons as dsu;
use wiiu_dsu::probe::DsuProbe;
use wiiu_dsu::{PadData, TouchPoint};

const T: Duration = Duration::from_secs(3);
const TOKEN: &str = "test-pairing-token-not-secret";

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn server() -> (Running, tokio::sync::watch::Sender<bool>) {
    let (tx, rx) = tokio::sync::watch::channel(false);
    let (running, _task) = start(
        ServerConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            dsu_bind: "127.0.0.1:0".parse().unwrap(),
            token: Some(TOKEN.into()),
            web_dir: None,
            allowed_origins: vec![],
            public_host: None,
        },
        rx,
    )
    .await
    .unwrap();
    (running, tx)
}

async fn connect(r: &Running) -> Ws {
    let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", r.http_addr))
        .await
        .unwrap();
    ws
}

async fn send(ws: &mut Ws, v: Value) {
    ws.send(Message::Text(v.to_string().into())).await.unwrap();
}

async fn recv(ws: &mut Ws) -> Value {
    loop {
        match tokio::time::timeout(T, ws.next())
            .await
            .expect("timed out")
            .expect("closed")
        {
            Ok(Message::Text(t)) => return serde_json::from_str(&t).unwrap(),
            Ok(Message::Close(_)) => panic!("socket closed"),
            Ok(_) => continue,
            Err(e) => panic!("ws error {e}"),
        }
    }
}

/// Waits for the server to close the socket, returning the error message it sent first (if any).
async fn recv_error_and_close(ws: &mut Ws) -> Option<Value> {
    let mut err = None;
    loop {
        match tokio::time::timeout(T, ws.next()).await.expect("timed out") {
            Some(Ok(Message::Text(t))) => err = Some(serde_json::from_str(&t).unwrap()),
            Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return err,
            Some(Ok(_)) => {}
        }
    }
}

async fn hello(ws: &mut Ws, device: &str) -> Value {
    send(
        ws,
        json!({"type":"hello","protocol":1,"token":TOKEN,"role":"wiimote","deviceId":device,"name":"test phone"}),
    )
    .await;
    recv(ws).await
}

#[tokio::test]
async fn wiimote_buttons_and_pointer_reach_cemu_client() {
    let (r, _stop) = server().await;
    let mut ws = connect(&r).await;
    let welcome = hello(&mut ws, "device-0001").await;
    assert_eq!(welcome["type"], "welcome");
    assert_eq!(welcome["slot"], 1);
    assert_eq!(welcome["player"], 1);

    let probe = DsuProbe::connect(r.dsu_addr).await.unwrap();
    send(
        &mut ws,
        json!({"type":"input","seq":1,"buttons":["A","B"],"pointer":{"x":0.5,"y":0.5}}),
    )
    .await;
    let pad = probe.wait_for_pad(1, T, |p| p.buttons != 0).await.unwrap();
    assert_eq!(pad.buttons, dsu::CROSS | dsu::CIRCLE);
    assert_eq!(pad.touch[0], Some(TouchPoint { id: 0, x: 960, y: 471 }));

    // Move the pointer off-screen and release.
    send(&mut ws, json!({"type":"input","seq":2,"buttons":[],"pointer":null})).await;
    let pad = probe.wait_for_pad(1, T, |p| p.buttons == 0).await.unwrap();
    assert_eq!(pad.touch[0], None);

    // Ping/pong.
    send(&mut ws, json!({"type":"ping","id":42})).await;
    assert_eq!(recv(&mut ws).await, json!({"type":"pong","id":42}));
}

#[tokio::test]
async fn closing_the_socket_releases_held_buttons() {
    let (r, _stop) = server().await;
    let mut ws = connect(&r).await;
    hello(&mut ws, "device-0002").await;
    let probe = DsuProbe::connect(r.dsu_addr).await.unwrap();
    send(
        &mut ws,
        json!({"type":"input","seq":1,"buttons":["B"],"pointer":{"x":0.2,"y":0.3}}),
    )
    .await;
    probe.wait_for_pad(1, T, |p| p.buttons == dsu::CIRCLE).await.unwrap();
    drop(ws);
    let pad = probe.wait_for_pad(1, T, |p| p.buttons == 0).await.unwrap();
    assert_eq!(pad, PadData::neutral());
}

#[tokio::test]
async fn silent_phone_is_neutralised_by_watchdog() {
    let (r, _stop) = server().await;
    let mut ws = connect(&r).await;
    hello(&mut ws, "device-0003").await;
    let probe = DsuProbe::connect(r.dsu_addr).await.unwrap();
    send(&mut ws, json!({"type":"input","seq":1,"buttons":["A"]})).await;
    probe.wait_for_pad(1, T, |p| p.buttons != 0).await.unwrap();
    // No heartbeat: within ~1.1 s the server must release A while the socket stays open.
    let start = std::time::Instant::now();
    probe.wait_for_pad(1, T, |p| p.buttons == 0).await.unwrap();
    let waited = start.elapsed();
    assert!(waited >= Duration::from_millis(800), "released too early: {waited:?}");
    assert!(r.sessions.status()[1].stale);
}

#[tokio::test]
async fn three_phones_control_three_independent_wiimotes() {
    let (r, _stop) = server().await;
    let mut phones = Vec::new();
    for i in 0..3 {
        let mut ws = connect(&r).await;
        let w = hello(&mut ws, &format!("device-10{i:02}")).await;
        assert_eq!(w["player"], i + 1);
        phones.push(ws);
    }
    let probe = DsuProbe::connect(r.dsu_addr).await.unwrap();
    let buttons = ["A", "ONE", "HOME"];
    let expected = [dsu::CROSS, dsu::SQUARE, dsu::L3];
    for (i, ws) in phones.iter_mut().enumerate() {
        let x = 0.1 + 0.3 * i as f64;
        send(
            ws,
            json!({"type":"input","seq":1,"buttons":[buttons[i]],"pointer":{"x":x,"y":0.5}}),
        )
        .await;
    }
    for (i, want_buttons) in expected.into_iter().enumerate() {
        let slot = (i + 1) as u8;
        let pad = probe.wait_for_pad(slot, T, |p| p.buttons != 0).await.unwrap();
        assert_eq!(pad.buttons, want_buttons, "slot {slot}");
        let x = pad.touch[0].unwrap().x;
        let want = ((0.1 + 0.3 * i as f64) * 1920.0).round() as u16;
        assert_eq!(x, want, "slot {slot} pointer x");
    }
    // A fourth Wii Remote is refused.
    let mut extra = connect(&r).await;
    let reply = hello(&mut extra, "device-1099").await;
    assert_eq!(reply["code"], "role_unavailable");
}

#[tokio::test]
async fn rejects_bad_token_and_invalid_messages() {
    let (r, _stop) = server().await;

    let mut ws = connect(&r).await;
    send(
        &mut ws,
        json!({"type":"hello","protocol":1,"token":"nope","role":"wiimote","deviceId":"device-2000"}),
    )
    .await;
    assert_eq!(recv_error_and_close(&mut ws).await.unwrap()["code"], "bad_token");

    let mut ws = connect(&r).await;
    send(&mut ws, json!({"type":"input","seq":1,"buttons":[]})).await;
    assert_eq!(recv_error_and_close(&mut ws).await.unwrap()["code"], "invalid_message");

    let mut ws = connect(&r).await;
    hello(&mut ws, "device-2001").await;
    send(&mut ws, json!({"type":"input","seq":1,"buttons":["NOT_A_BUTTON"]})).await;
    assert_eq!(recv_error_and_close(&mut ws).await.unwrap()["code"], "invalid_message");

    let mut ws = connect(&r).await;
    hello(&mut ws, "device-2002").await;
    ws.send(Message::Binary(vec![1, 2, 3].into())).await.unwrap();
    assert_eq!(recv_error_and_close(&mut ws).await.unwrap()["code"], "invalid_message");
}

#[tokio::test]
async fn reconnect_with_same_device_replaces_old_socket() {
    let (r, _stop) = server().await;
    let mut first = connect(&r).await;
    hello(&mut first, "device-3000").await;
    let mut second = connect(&r).await;
    let w = hello(&mut second, "device-3000").await;
    assert_eq!(w["slot"], 1);
    assert_eq!(recv_error_and_close(&mut first).await.unwrap()["code"], "replaced");
}

#[tokio::test]
async fn cross_origin_websocket_is_refused() {
    let (r, _stop) = server().await;
    let mut req = format!("ws://{}/ws", r.http_addr).into_client_request().unwrap();
    req.headers_mut()
        .insert("origin", "https://evil.example".parse().unwrap());
    assert!(tokio_tungstenite::connect_async(req).await.is_err());

    let mut req = format!("ws://{}/ws", r.http_addr).into_client_request().unwrap();
    req.headers_mut()
        .insert("origin", format!("http://{}", r.http_addr).parse().unwrap());
    assert!(tokio_tungstenite::connect_async(req).await.is_ok());
}

#[tokio::test]
async fn pc_only_pages_require_local_host_header() {
    let (r, _stop) = server().await;
    let get = |path: &'static str, host: String| {
        let addr = r.http_addr;
        async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
            s.write_all(format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n").as_bytes())
                .await
                .unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).await.unwrap();
            out
        }
    };
    let port = r.http_addr.port();
    let ok = get("/dev/pointer", format!("localhost:{port}")).await;
    assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
    assert!(ok.contains(TOKEN), "dev page embeds the token for local use");
    let rebinding = get("/dev/pointer", format!("attacker.example:{port}")).await;
    assert!(rebinding.starts_with("HTTP/1.1 403"));
    assert!(!rebinding.contains(TOKEN));
    let pair = get("/pair", format!("127.0.0.1:{port}")).await;
    assert!(pair.starts_with("HTTP/1.1 200") && pair.contains("<svg"));
    let status = get("/api/status", format!("localhost:{port}")).await;
    assert!(status.contains("\"role\":\"gamepad\""));
}

#[tokio::test]
async fn tracking_layout_is_public_and_complete() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (r, _stop) = server().await;
    let mut s = tokio::net::TcpStream::connect(r.http_addr).await.unwrap();
    s.write_all(b"GET /api/tracking/layout HTTP/1.1\r\nHost: 192.168.1.20:8737\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).await.unwrap();
    assert!(out.starts_with("HTTP/1.1 200"), "{out}");
    let body: Value = serde_json::from_str(out.split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(body["family"], "tag36h11");
    let markers = body["markers"].as_array().unwrap();
    assert_eq!(markers.len(), 10);
    for m in markers {
        let cells = m["cells"].as_str().unwrap();
        assert_eq!(cells.split('/').count(), 10);
        assert!(cells.split('/').all(|row| row.len() == 10));
        assert_eq!(m["corners"].as_array().unwrap().len(), 4);
    }
}
