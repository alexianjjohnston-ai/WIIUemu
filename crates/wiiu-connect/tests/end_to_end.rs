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
            https: None,
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

#[tokio::test]
async fn https_listener_serves_phones_with_self_signed_cert() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let dir = std::env::temp_dir().join(format!("wiiu-e2e-tls-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let (r, _task) = start(
        ServerConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            dsu_bind: "127.0.0.1:0".parse().unwrap(),
            token: Some(TOKEN.into()),
            web_dir: None,
            allowed_origins: vec![],
            public_host: Some("127.0.0.1".into()),
            https: Some(wiiu_connect::HttpsConfig {
                bind: "127.0.0.1:0".parse().unwrap(),
                data_dir: dir.clone(),
            }),
        },
        rx,
    )
    .await
    .unwrap();
    let https = r.https_addr.unwrap();
    assert!(r.phone_url.starts_with(&format!("https://127.0.0.1:{}", https.port())));

    // Trust exactly the generated certificate, as a phone does after accepting the warning.
    let pem = std::fs::read(r.cert_path.as_ref().unwrap()).unwrap();
    let mut roots = tokio_rustls::rustls::RootCertStore::empty();
    for cert in tokio_rustls::rustls::pki_types::pem::PemObject::pem_slice_iter(&pem) {
        roots.add(cert.unwrap()).unwrap();
    }
    let client = tokio_rustls::rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(client));
    let tcp = tokio::net::TcpStream::connect(https).await.unwrap();
    let name = tokio_rustls::rustls::pki_types::ServerName::try_from("127.0.0.1").unwrap();
    let mut tls = connector.connect(name, tcp).await.unwrap();
    tls.write_all(b"GET /api/tracking/layout HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut out = Vec::new();
    let _ = tls.read_to_end(&mut out).await;
    let out = String::from_utf8_lossy(&out);
    assert!(out.starts_with("HTTP/1.1 200"), "{out}");
    assert!(out.contains("tag36h11"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Renders the default marker border frontally (game image centred) as a grayscale JPEG.
fn border_jpeg(w: usize, h: usize) -> Vec<u8> {
    use wiiu_tracking::{BorderConfig, MarkerLayout};
    let layout = MarkerLayout::border(&BorderConfig::default());
    let scale = h as f64 * 0.7;
    let ox = w as f64 / 2.0 - layout.aspect * scale / 2.0;
    let oy = h as f64 / 2.0 - scale / 2.0;
    let to_px = |x: f64, y: f64| (ox + x * layout.aspect * scale, oy + y * scale);
    let mut img = vec![90u8; w * h];
    for m in &layout.markers {
        let cells = wiiu_tracking::tags::render(m.id as usize).unwrap();
        let (x0, y0) = to_px(m.corners[3][0], m.corners[3][1]);
        let (x1, _) = to_px(m.corners[1][0], 0.0);
        let cell = (x1 - x0) / 8.0;
        for py in 0..h {
            for px in 0..w {
                let cx = ((px as f64 + 0.5 - x0) / cell + 1.0).floor();
                let cy = ((py as f64 + 0.5 - y0) / cell + 1.0).floor();
                if (0.0..10.0).contains(&cx) && (0.0..10.0).contains(&cy) {
                    img[py * w + px] = if cells[cy as usize][cx as usize] { 255 } else { 0 };
                }
            }
        }
    }
    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, 75)
        .encode(&img, w as u16, h as u16, jpeg_encoder::ColorType::Luma)
        .unwrap();
    out
}

#[tokio::test]
async fn camera_frames_drive_the_wiimote_pointer_over_protocol_v2() {
    use wiiu_controller::protocol::encode_camera_frame;
    let (r, _stop) = server().await;
    let mut ws = connect(&r).await;
    send(
        &mut ws,
        json!({"type":"hello","protocol":2,"token":TOKEN,"role":"wiimote","deviceId":"device-4000"}),
    )
    .await;
    let welcome = recv(&mut ws).await;
    assert_eq!(welcome["protocol"], 2);
    let probe = DsuProbe::connect(r.dsu_addr).await.unwrap();

    // The phone's own pointer says top-left; the camera says the centre. The camera must win.
    send(
        &mut ws,
        json!({"type":"input","seq":1,"buttons":["B"],"pointer":{"x":0.05,"y":0.05}}),
    )
    .await;
    send(&mut ws, json!({"type":"gyro","samples":[[900000,0.0,0.0,0.0]]})).await;
    let jpeg = border_jpeg(640, 360);
    ws.send(Message::Binary(
        encode_camera_frame(640, 360, 1, 1_000_000, &jpeg).into(),
    ))
    .await
    .unwrap();

    let pad = probe
        .wait_for_pad(1, T, |p| p.touch[0].is_some_and(|t| (t.x as i32 - 960).abs() < 25))
        .await
        .unwrap();
    assert_eq!(pad.buttons, dsu::CIRCLE, "buttons still come from input snapshots");
    let t = pad.touch[0].unwrap();
    assert!((t.y as i32 - 471).abs() < 15, "{t:?}");

    // The phone gets tracking feedback.
    let mut got = None;
    for _ in 0..20 {
        let m = recv(&mut ws).await;
        if m["type"] == "tracking" && m["state"] == "tracking" {
            got = Some(m);
            break;
        }
    }
    let m = got.expect("tracking feedback");
    assert_eq!(m["tags"], 10);
}

#[tokio::test]
async fn camera_frames_are_rejected_on_protocol_v1_and_for_bad_headers() {
    use wiiu_controller::protocol::encode_camera_frame;
    let (r, _stop) = server().await;
    let mut ws = connect(&r).await;
    hello(&mut ws, "device-4100").await; // protocol 1
    ws.send(Message::Binary(
        encode_camera_frame(640, 360, 1, 1, &[0xFF, 0xD8]).into(),
    ))
    .await
    .unwrap();
    assert_eq!(recv_error_and_close(&mut ws).await.unwrap()["code"], "invalid_message");

    let mut ws = connect(&r).await;
    send(
        &mut ws,
        json!({"type":"hello","protocol":2,"token":TOKEN,"role":"wiimote","deviceId":"device-4101"}),
    )
    .await;
    recv(&mut ws).await;
    ws.send(Message::Binary(
        encode_camera_frame(5000, 360, 1, 1, &[0xFF, 0xD8]).into(),
    ))
    .await
    .unwrap();
    assert_eq!(recv_error_and_close(&mut ws).await.unwrap()["code"], "invalid_message");
}
