//! The phone app compiled into the executable (see `build.rs`).

use axum::http::{header, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};

include!(concat!(env!("OUT_DIR"), "/embedded_web.rs"));

pub fn available() -> bool {
    FILES.iter().any(|(p, _)| *p == "index.html")
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "webmanifest" => "application/manifest+json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

/// Serves an embedded file; unknown paths without an extension get `index.html` (client-side routing).
pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    let found = FILES.iter().find(|(p, _)| *p == path).or_else(|| {
        (!path.rsplit('/').next().unwrap_or("").contains('.'))
            .then(|| FILES.iter().find(|(p, _)| *p == "index.html"))
            .flatten()
    });
    let Some((name, body)) = found else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };
    let mut r = (*body).into_response();
    let headers = r.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type(name)));
    // Vite puts content hashes in asset names; everything else must be revalidated.
    let cache = if name.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    r
}
