//! HTTP routes: phone web app, WebSocket, and PC-only pages (pairing, diagnostics, mouse test pointer).
//!
//! Pages that reveal the pairing token are served only to loopback peers that also address the server by a
//! loopback host name. The host check defeats DNS-rebinding attacks from websites open on the PC.

use crate::session::SessionManager;
use axum::extract::{ConnectInfo, State, WebSocketUpgrade};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use wiiu_controller::protocol::MAX_FRAME_BYTES;

#[derive(Clone)]
pub struct AppState {
    pub sessions: Arc<SessionManager>,
    pub config: Arc<HttpConfig>,
}

pub struct HttpConfig {
    /// URL phones should open (shown as a QR code on the pairing page), without the token.
    pub phone_base_url: String,
    /// Extra origins allowed to open `/ws` (e.g. a Vite dev server). Same-origin is always allowed.
    pub allowed_origins: Vec<String>,
    /// Built phone web app (`apps/phone/dist`).
    pub web_dir: Option<PathBuf>,
}

impl HttpConfig {
    pub fn phone_url(&self, token: &str) -> String {
        format!("{}/?token={token}", self.phone_base_url.trim_end_matches('/'))
    }
}

pub fn router(state: AppState) -> Router {
    let mut app = Router::new()
        .route("/ws", get(ws_upgrade))
        .route("/pair", get(pair_page))
        .route("/dev/pointer", get(dev_pointer_page))
        .route("/api/status", get(status));
    app = match state.config.web_dir.clone().filter(|d| d.join("index.html").is_file()) {
        Some(dir) => {
            let index = dir.join("index.html");
            app.fallback_service(
                tower_http::services::ServeDir::new(dir).fallback(tower_http::services::ServeFile::new(index)),
            )
        }
        None => app.route("/", get(no_web_app)),
    };
    app.with_state(state)
}

fn host_name(headers: &HeaderMap) -> Option<&str> {
    let host = headers.get(header::HOST)?.to_str().ok()?;
    // Strip the port; keep IPv6 brackets.
    Some(match host.rfind(':') {
        Some(i) if !host[i..].contains(']') => &host[..i],
        _ => host,
    })
}

/// True when the request comes from this PC *and* names this PC as the host.
pub fn is_local_request(peer: SocketAddr, headers: &HeaderMap) -> bool {
    peer.ip().is_loopback() && matches!(host_name(headers), Some("localhost" | "127.0.0.1" | "[::1]"))
}

/// Browsers always send `Origin` on WebSocket upgrades; require it to match the host (or an allow-listed
/// origin) so other websites can't drive a controller. Non-browser clients send no `Origin`.
fn origin_allowed(headers: &HeaderMap, config: &HttpConfig) -> bool {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|o| o.to_str().ok()) else {
        return true;
    };
    if config.allowed_origins.iter().any(|o| o == origin) {
        return true;
    }
    let origin_host = origin.split_once("://").map(|(_, rest)| rest).unwrap_or("");
    headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|host| host.eq_ignore_ascii_case(origin_host))
}

async fn ws_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if !origin_allowed(&headers, &state.config) {
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    ws.max_message_size(MAX_FRAME_BYTES)
        .max_frame_size(MAX_FRAME_BYTES)
        .on_upgrade(move |socket| crate::ws::handle(socket, state.sessions, peer))
}

/// `Some(403 response)` unless the request is local (see [`is_local_request`]).
fn reject_remote(peer: SocketAddr, headers: &HeaderMap) -> Option<Response> {
    (!is_local_request(peer, headers)).then(|| {
        (
            StatusCode::FORBIDDEN,
            "this page is only available on the PC running Wii U Connect",
        )
            .into_response()
    })
}

fn no_store(mut r: Response) -> Response {
    r.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

async fn status(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Some(r) = reject_remote(peer, &headers) {
        return r;
    }
    no_store(Json(state.sessions.status()).into_response())
}

async fn pair_page(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Some(r) = reject_remote(peer, &headers) {
        return r;
    }
    let url = state.config.phone_url(state.sessions.token());
    let svg = qrcode::QrCode::new(url.as_bytes())
        .map(|code| {
            code.render::<qrcode::render::svg::Color<'_>>()
                .min_dimensions(240, 240)
                .quiet_zone(true)
                .build()
        })
        .unwrap_or_default();
    let html = include_str!("../web/pair.html")
        .replace("{{QR_SVG}}", &svg)
        .replace("{{PHONE_URL}}", &html_escape(&url));
    no_store(Html(html).into_response())
}

async fn dev_pointer_page(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Some(r) = reject_remote(peer, &headers) {
        return r;
    }
    let token_js = serde_json::to_string(state.sessions.token()).expect("string serializes");
    let html = include_str!("../web/dev-pointer.html").replace("\"{{TOKEN_JSON}}\"", &token_js);
    no_store(Html(html).into_response())
}

async fn no_web_app() -> Html<&'static str> {
    Html(
        "<!doctype html><meta charset=utf-8><title>Wii U Connect</title>\
         <p>The phone app is not built yet. Run <code>npm ci &amp;&amp; npm run build</code> in <code>apps/phone</code>, \
         or pass <code>--web-dir</code>.</p>",
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, HeaderValue::from_str(v).unwrap());
        }
        h
    }

    fn config(allowed: &[&str]) -> HttpConfig {
        HttpConfig {
            phone_base_url: "http://192.168.1.2:8737".into(),
            allowed_origins: allowed.iter().map(|s| s.to_string()).collect(),
            web_dir: None,
        }
    }

    #[test]
    fn local_request_requires_loopback_peer_and_host() {
        let local: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let lan: SocketAddr = "192.168.1.50:5000".parse().unwrap();
        assert!(is_local_request(local, &headers(&[("host", "localhost:8737")])));
        assert!(is_local_request(local, &headers(&[("host", "127.0.0.1:8737")])));
        assert!(is_local_request(local, &headers(&[("host", "[::1]:8737")])));
        assert!(!is_local_request(lan, &headers(&[("host", "localhost:8737")])));
        // DNS rebinding: loopback peer but attacker-controlled host name.
        assert!(!is_local_request(local, &headers(&[("host", "evil.example:8737")])));
        assert!(!is_local_request(local, &headers(&[])));
    }

    #[test]
    fn origin_checks() {
        let c = config(&["http://localhost:5173"]);
        assert!(origin_allowed(&headers(&[("host", "192.168.1.2:8737")]), &c));
        assert!(origin_allowed(
            &headers(&[("host", "192.168.1.2:8737"), ("origin", "http://192.168.1.2:8737")]),
            &c
        ));
        assert!(origin_allowed(
            &headers(&[("host", "192.168.1.2:8737"), ("origin", "http://localhost:5173")]),
            &c
        ));
        assert!(!origin_allowed(
            &headers(&[("host", "192.168.1.2:8737"), ("origin", "https://evil.example")]),
            &c
        ));
        assert!(!origin_allowed(
            &headers(&[("host", "localhost:8737"), ("origin", "http://localhost:8738")]),
            &c
        ));
    }

    #[test]
    fn phone_url_includes_token() {
        assert_eq!(config(&[]).phone_url("abc"), "http://192.168.1.2:8737/?token=abc");
    }
}
