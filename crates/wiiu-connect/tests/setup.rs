//! The PC-side setup API: PC-only, CSRF-protected, and it only touches what the user pointed it at.

use serde_json::Value;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use wiiu_connect::{start, Running, ServerConfig};

async fn server(data_dir: &Path) -> (Running, tokio::sync::watch::Sender<bool>) {
    let (tx, rx) = tokio::sync::watch::channel(false);
    let (running, _task) = start(
        ServerConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            dsu_bind: "127.0.0.1:0".parse().unwrap(),
            token: Some("test-pairing-token-not-secret".into()),
            web_dir: None,
            embedded_web: false,
            setup: Some(wiiu_connect::setup::SetupConfig {
                data_dir: data_dir.to_path_buf(),
                auto: false,
            }),
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

/// Minimal HTTP/1.1 client: returns (status, headers + body).
async fn request(
    addr: SocketAddr,
    method: &str,
    path: &str,
    host: &str,
    extra_headers: &[(&str, String)],
    body: &str,
) -> (u16, String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (k, v) in extra_headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(body);
    s.write_all(req.as_bytes()).await.unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).await.unwrap();
    let status = out[9..12].parse().unwrap();
    (status, out)
}

fn json_body(resp: &str) -> Value {
    serde_json::from_str(resp.split("\r\n\r\n").nth(1).unwrap_or("null")).unwrap_or(Value::Null)
}

/// A folder that looks like Cemu on every OS (Windows `Cemu.exe`, Linux `Cemu`, macOS `Cemu.app`), portable.
fn fake_cemu(dir: &Path) -> PathBuf {
    let cemu = dir.join("MyCemu");
    std::fs::create_dir_all(cemu.join("portable")).unwrap();
    std::fs::create_dir_all(cemu.join("Cemu.app/Contents/MacOS")).unwrap();
    std::fs::write(cemu.join("Cemu.exe"), b"not really").unwrap();
    std::fs::write(cemu.join("Cemu"), b"not really").unwrap();
    cemu
}

fn temp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("wiiu-setup-e2e-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[tokio::test]
async fn setup_flow_locate_controllers_games() {
    let dir = temp("flow");
    let (r, _stop) = server(&dir.join("data")).await;
    let addr = r.http_addr;
    let host = format!("localhost:{}", addr.port());
    let origin = format!("http://{host}");
    let post = |path: &'static str, body: String| {
        let (host, origin) = (host.clone(), origin.clone());
        async move {
            request(
                addr,
                "POST",
                path,
                &host,
                &[("Origin", origin), ("Content-Type", "application/json".into())],
                &body,
            )
            .await
        }
    };

    // On the PC, `/` without a pairing token is the setup page; phones (with ?token=) get the phone app.
    let (code, resp) = request(addr, "GET", "/", &host, &[], "").await;
    assert_eq!(code, 307, "{resp}");
    assert!(resp.to_ascii_lowercase().contains("location: /setup"));
    let (code, _) = request(addr, "GET", "/?token=abc", &host, &[], "").await;
    assert_eq!(code, 200);
    let (code, page) = request(addr, "GET", "/setup", &host, &[], "").await;
    assert_eq!(code, 200);
    assert!(page.contains("never downloads games"));

    let (_, s) = request(addr, "GET", "/api/setup/status", &host, &[], "").await;
    assert!(json_body(&s)["setup"]["cemu"].is_null());

    let cemu = fake_cemu(&dir);
    let (code, resp) = post(
        "/api/setup/cemu/locate",
        serde_json::json!({ "path": cemu }).to_string(),
    )
    .await;
    assert_eq!(code, 200, "{resp}");
    let (code, resp) = post("/api/setup/controllers", "{}".into()).await;
    assert_eq!(code, 200, "{resp}");
    // Controller 1 is the phone GamePad (DSU slot 0), Controllers 2-4 the Wii Remotes (slots 1-3).
    for n in 0..=3 {
        let xml = std::fs::read_to_string(cemu.join(format!("portable/controllerProfiles/controller{n}.xml"))).unwrap();
        assert!(xml.contains("<api>DSUController</api>") && xml.contains(&format!("<uuid>{n}</uuid>")));
        let kind = if n == 0 { "Wii U GamePad" } else { "Wiimote" };
        assert!(xml.contains(&format!("<type>{kind}</type>")), "controller{n}: {xml}");
    }

    let games = dir.join("My games");
    std::fs::create_dir_all(&games).unwrap();
    std::fs::write(games.join("A Game.wua"), b"").unwrap();
    let (code, resp) = post("/api/setup/games", serde_json::json!({ "path": games }).to_string()).await;
    assert_eq!(code, 200, "{resp}");
    assert_eq!(json_body(&resp)["found"], 1);

    let (_, s) = request(addr, "GET", "/api/setup/status", &host, &[], "").await;
    let s = json_body(&s)["setup"].clone();
    assert_eq!(s["controllers_assigned"], true);
    assert_eq!(s["games"][0]["name"], "A Game");
    assert!(std::fs::read_to_string(cemu.join("portable/settings.xml"))
        .unwrap()
        .contains("My games"));

    // Only scanned games can be launched.
    let (code, resp) = post("/api/setup/launch", r#"{"game":"/etc/passwd"}"#.into()).await;
    assert_eq!(code, 400, "{resp}");
    assert!(resp.contains("isn't in your game folders"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn setup_requests_from_other_sites_or_hosts_are_refused() {
    let dir = temp("csrf");
    let (r, _stop) = server(&dir.join("data")).await;
    let addr = r.http_addr;
    let host = format!("localhost:{}", addr.port());
    let cemu = fake_cemu(&dir);
    let body = serde_json::json!({ "path": cemu }).to_string();
    let json = ("Content-Type", "application/json".to_string());

    // Cross-site form post (other origin, or text/plain), and no Origin at all.
    for headers in [
        vec![("Origin", "http://evil.example".to_string()), json.clone()],
        vec![
            ("Origin", format!("http://{host}")),
            ("Content-Type", "text/plain".to_string()),
        ],
        vec![json.clone()],
    ] {
        let (code, resp) = request(addr, "POST", "/api/setup/cemu/locate", &host, &headers, &body).await;
        // 415 = refused by the JSON extractor before the handler runs; 403 = refused by the origin check.
        assert!(code == 403 || code == 415, "{resp}");
    }
    // DNS rebinding: foreign Host header, even with a matching Origin.
    let evil = format!("evil.example:{}", addr.port());
    let (code, _) = request(
        addr,
        "POST",
        "/api/setup/cemu/locate",
        &evil,
        &[("Origin", format!("http://{evil}")), json],
        &body,
    )
    .await;
    assert_eq!(code, 403);
    let (code, _) = request(addr, "GET", "/api/setup/status", &evil, &[], "").await;
    assert_eq!(code, 403);
    assert!(
        !cemu.join("portable/controllerProfiles").exists(),
        "nothing was written"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn tv_border_switch_is_saved_and_shown() {
    let dir = temp("tvborder");
    let data = dir.join("data");
    let (r, stop) = server(&data).await;
    let addr = r.http_addr;
    let host = format!("localhost:{}", addr.port());
    let same_origin = [
        ("Origin", format!("http://{host}")),
        ("Content-Type", "application/json".to_string()),
    ];
    let status = |addr: SocketAddr, host: String| async move {
        let (code, resp) = request(addr, "GET", "/api/setup/status", &host, &[], "").await;
        assert_eq!(code, 200, "{resp}");
        json_body(&resp)
    };

    let (code, page) = request(addr, "GET", "/setup", &host, &[], "").await;
    assert_eq!(code, 200);
    assert!(
        page.contains(r#"id="step-tv""#) && page.contains("/api/setup/tv-border"),
        "TV border card on the page"
    );

    let s = status(addr, host.clone()).await;
    assert_eq!(s["tv_border"]["mode"], "auto", "{s}");
    assert!(s["tv_border"]["status"]["state"].is_string(), "{s}");

    let (code, resp) = request(
        addr,
        "POST",
        "/api/setup/tv-border",
        &host,
        &same_origin,
        r#"{"mode":"off"}"#,
    )
    .await;
    assert_eq!(code, 204, "{resp}");
    assert_eq!(status(addr, host.clone()).await["tv_border"]["mode"], "off");

    // Unknown values are refused, not guessed.
    for body in [r#"{"mode":"maybe"}"#, r#"{"mode":1}"#, "{}"] {
        let (code, resp) = request(addr, "POST", "/api/setup/tv-border", &host, &same_origin, body).await;
        assert_eq!(code, 400, "{body}: {resp}");
    }
    // Like every setup action, only the setup page may change it.
    let (code, _) = request(
        addr,
        "POST",
        "/api/setup/tv-border",
        &host,
        &[("Content-Type", "application/json".to_string())],
        r#"{"mode":"on"}"#,
    )
    .await;
    assert_eq!(code, 403);

    // The choice survives a restart.
    let _ = stop.send(true);
    drop(r);
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let (r2, _stop2) = server(&data).await;
    let host2 = format!("localhost:{}", r2.http_addr.port());
    assert_eq!(status(r2.http_addr, host2).await["tv_border"]["mode"], "off");
}
