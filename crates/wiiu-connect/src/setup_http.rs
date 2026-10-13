//! HTTP API for the PC-side setup page (`/setup`). Everything here is PC-only: requests must come from this PC
//! (`reject_remote`), and state-changing requests must be same-origin JSON POSTs, so other websites open in the
//! user's browser can't trigger downloads, write Cemu files or start programs (CSRF).

use crate::http::{reject_remote, AppState};
use crate::setup::{pick_folder, SetupError};
use axum::extract::{ConnectInfo, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use std::net::SocketAddr;
use std::path::PathBuf;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/setup", get(page))
        .route("/api/setup/status", get(status))
        .route("/api/setup/phone-qr.svg", get(phone_qr))
        .route("/api/setup/cemu/download", post(download))
        .route("/api/setup/cemu/install-included", post(install_included))
        .route("/api/setup/cemu/locate", post(locate))
        .route("/api/setup/controllers", post(controllers))
        .route("/api/setup/games", post(add_games))
        .route("/api/setup/launch", post(launch))
}

fn err(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(serde_json::json!({ "error": message.into() }))).into_response()
}

fn setup_err(e: SetupError) -> Response {
    let status = match e {
        SetupError::Io(_) | SetupError::Settings(_) => StatusCode::INTERNAL_SERVER_ERROR,
        SetupError::Busy => StatusCode::CONFLICT,
        _ => StatusCode::BAD_REQUEST,
    };
    err(status, e.to_string())
}

/// Same-origin JSON POST from this PC (blocks cross-site form posts and DNS rebinding).
fn check_post(peer: SocketAddr, headers: &HeaderMap) -> Option<Response> {
    if let Some(r) = reject_remote(peer, headers) {
        return Some(r);
    }
    let json = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/json"));
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    let origin_ok = headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .and_then(|o| o.strip_prefix("http://"))
        .is_some_and(|o| Some(o) == host);
    (!(json && origin_ok)).then(|| err(StatusCode::FORBIDDEN, "setup requests must come from the setup page"))
}

macro_rules! setup_or_404 {
    ($state:expr) => {
        match $state.setup.clone() {
            Some(s) => s,
            None => return err(StatusCode::NOT_FOUND, "setup is disabled"),
        }
    };
}

async fn page(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Some(r) = reject_remote(peer, &headers) {
        return r;
    }
    if state.setup.is_none() {
        return err(StatusCode::NOT_FOUND, "setup is disabled");
    }
    let mut r = Html(include_str!("../web/setup.html")).into_response();
    r.headers_mut()
        .insert(header::CACHE_CONTROL, header::HeaderValue::from_static("no-store"));
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
    let setup = setup_or_404!(state);
    let s = tokio::task::spawn_blocking(move || setup.status()).await;
    match s {
        Ok(s) => {
            let mut r = Json(serde_json::json!({
                "setup": s,
                "phones": state.sessions.status(),
                // Contains the pairing token: this endpoint is PC-only (`reject_remote`), like `/pair`.
                "phone_url": state.config.phone_url(state.sessions.token()),
            }))
            .into_response();
            no_store(&mut r);
            r
        }
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

/// The pairing QR code, shown on the setup page so phones can be paired without opening `/pair`.
async fn phone_qr(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Some(r) = reject_remote(peer, &headers) {
        return r;
    }
    let svg = crate::http::qr_svg(&state.config.phone_url(state.sessions.token()));
    let mut r = ([(header::CONTENT_TYPE, "image/svg+xml")], svg).into_response();
    no_store(&mut r);
    r
}

fn no_store(r: &mut Response) {
    r.headers_mut()
        .insert(header::CACHE_CONTROL, header::HeaderValue::from_static("no-store"));
}

async fn download(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Some(r) = check_post(peer, &headers) {
        return r;
    }
    let setup = setup_or_404!(state);
    match setup.start_download() {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(e) => setup_err(e),
    }
}

/// Installs the Cemu built into this executable (no download).
async fn install_included(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Some(r) = check_post(peer, &headers) {
        return r;
    }
    let setup = setup_or_404!(state);
    match setup.start_bundled_install() {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(e) => setup_err(e),
    }
}

#[derive(Deserialize, Default)]
struct PathRequest {
    /// Typed by the user; when absent the OS folder picker opens on this PC.
    path: Option<String>,
}

async fn chosen_path(req: PathRequest, title: &str) -> Option<PathBuf> {
    match req
        .path
        .map(|p| p.trim().trim_matches('"').to_string())
        .filter(|p| !p.is_empty())
    {
        Some(p) => Some(PathBuf::from(p)),
        None => pick_folder(title).await,
    }
}

async fn locate(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<PathRequest>,
) -> Response {
    if let Some(r) = check_post(peer, &headers) {
        return r;
    }
    let setup = setup_or_404!(state);
    let Some(dir) = chosen_path(req, "Choose the folder that contains Cemu").await else {
        return err(StatusCode::BAD_REQUEST, "no folder chosen");
    };
    match tokio::task::spawn_blocking(move || setup.locate(&dir)).await {
        Ok(Ok(c)) => Json(c).into_response(),
        Ok(Err(e)) => setup_err(e),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn controllers(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Some(r) = check_post(peer, &headers) {
        return r;
    }
    let setup = setup_or_404!(state);
    match tokio::task::spawn_blocking(move || setup.assign_controllers()).await {
        Ok(Ok(report)) => Json(serde_json::json!({ "backups": report.backups })).into_response(),
        Ok(Err(e)) => setup_err(e),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn add_games(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<PathRequest>,
) -> Response {
    if let Some(r) = check_post(peer, &headers) {
        return r;
    }
    let setup = setup_or_404!(state);
    let Some(dir) = chosen_path(req, "Choose the folder with your own Wii U game dumps").await else {
        return err(StatusCode::BAD_REQUEST, "no folder chosen");
    };
    match tokio::task::spawn_blocking(move || setup.add_game_folder(&dir)).await {
        Ok(Ok(found)) => Json(serde_json::json!({ "found": found })).into_response(),
        Ok(Err(e)) => setup_err(e),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[derive(Deserialize)]
struct LaunchRequest {
    /// A `path` from the status game list; absent = just open Cemu.
    game: Option<PathBuf>,
    #[serde(default)]
    fullscreen: bool,
}

async fn launch(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<LaunchRequest>,
) -> Response {
    if let Some(r) = check_post(peer, &headers) {
        return r;
    }
    let setup = setup_or_404!(state);
    match tokio::task::spawn_blocking(move || setup.launch(req.game.as_deref(), req.fullscreen)).await {
        Ok(Ok(())) => StatusCode::NO_CONTENT.into_response(),
        Ok(Err(e)) => setup_err(e),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
