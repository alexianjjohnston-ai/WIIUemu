# Desktop App Window Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** On Windows and macOS, double-clicking Wii U Connect opens one "Wii U Connect" window showing the setup page
(no console, no browser tab). Linux and every command-line use behave as before.

**Architecture:** `wry` (web view) + `tao` (window, event loop) show `http://localhost:<port>/setup` from the server
already running in the same process. The tokio runtime runs the server on worker threads; the main thread runs the
`tao` event loop. Pure decisions (which URLs stay in the window, which UI to open, AppleScript quoting) live in
`wiiu_connect::app_window` and are unit-tested on every platform. The few Win32 calls (attach to the parent
terminal, message box) live in a new FFI crate, `wiiu-desktop-sys`, because the workspace forbids `unsafe`
elsewhere.

**Tech Stack:** Rust 2021, `wry` 0.57, `tao` 0.37, `windows-sys` 0.61, axum 0.8, tokio, GitHub Actions.

**Spec:** `docs/adr/0003-desktop-app-window.md`

## Global Constraints

- Windows download stays **one exe, nothing to install**. The release `dumpbin /dependents` check must reject
  `webview2loader` as well as the C runtime DLLs.
- `unsafe` only in `-sys` crates (`[workspace.lints.rust] unsafe_code = "forbid"`). `wiiu-desktop-sys` follows the
  `wiiu-apriltag-sys` pattern: no `[lints] workspace = true`, clippy still on.
- `wry`/`tao` are dependencies **only** for `cfg(any(windows, target_os = "macos"))`. Linux builds must not need
  WebKitGTK.
- `--no-browser` means "open no UI at all" (no window, no browser), as the CI smoke tests already rely on.
  `--no-window` means "today's console behaviour" (console output, browser unless `--no-browser`).
- `POST /api/app/show` uses `setup_http::check_post` (loopback peer and Host, JSON content type, `Origin` =
  `http://<Host>`).
- The window loads `http://localhost:<port>/setup`, or `/pair` when setup is disabled (`--no-setup`).
- Closing the window stops the server through the same stop channel as Ctrl+C (phones' input is released by the
  existing session teardown).
- Window: title "Wii U Connect", 1100×800 logical px, resizable.
- macOS bundle: `Wii U Connect.app`, bundle id `io.github.alexianjjohnston-ai.wiiuconnect`,
  `LSMinimumSystemVersion` 11.0, executable `Contents/MacOS/wiiu-connect`, icon `Contents/Resources/wiiu-connect.icns`.
- No phone app or protocol change.
- Run `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
  before every commit. Never run prettier in `apps/phone`.

## Review Focus

1. **A link to another site, or a crafted URL such as `http://localhost:8737@evil.example/`, inside the window:**
   must open in the default browser (http/https only) and never load in the window. Tests: Task 2.
2. **A second double-click while the first copy runs:** the first window comes to the front, no second server. If
   the running copy is an older version without `/api/app/show` (404), fall back to opening the setup page in the
   browser. Tests: Task 3.
3. **Started from a terminal with output redirected** (`--version > file`, the CI smoke tests): output must still
   arrive. The console is attached only when stdout is not already redirected. Tests: the existing release smoke test
   `& $exe --version` (Task 5) runs against the GUI-subsystem exe.
4. **No WebView2 runtime:** the setup page opens in the browser, a message explains why and links the runtime, and
   clicking OK stops Wii U Connect, so no invisible process is left behind. Tests: the forced-fallback self-test in
   Task 5.
5. **A web page trying to raise the window** (`fetch('http://localhost:8737/api/app/show', {method:'POST'})`):
   403. Tests: Task 3.

---

### Task 1: `wiiu-desktop-sys` crate (Win32 console attach and message box)

**Files:**
- Create: `crates/wiiu-desktop-sys/Cargo.toml`
- Create: `crates/wiiu-desktop-sys/src/lib.rs`
- Create: `crates/wiiu-desktop-sys/src/windows.rs`
- Modify: `Cargo.toml` (workspace members and `[workspace.dependencies]`)

**Interfaces:**
- Produces (Windows only): `wiiu_desktop_sys::attach_parent_console() -> bool`,
  `wiiu_desktop_sys::message_box(title: &str, text: &str, kind: wiiu_desktop_sys::MessageKind)`,
  `pub enum MessageKind { Info, Error }`. On other targets the crate is empty.

- [ ] **Step 1: Create the crate manifest**

`crates/wiiu-desktop-sys/Cargo.toml`:
```toml
[package]
name = "wiiu-desktop-sys"
description = "The Win32 calls of Wii U Connect's desktop app (console attach, message box) behind safe functions"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true
license = "MIT OR Apache-2.0"

[target.'cfg(windows)'.dependencies]
windows-sys = { version = "0.61", features = [
    "Win32_Foundation",
    "Win32_System_Console",
    "Win32_UI_WindowsAndMessaging",
] }

# Not `[lints] workspace = true`: like wiiu-apriltag-sys, this crate is a place FFI (`unsafe`) is allowed.
[lints.clippy]
all = { level = "warn", priority = -1 }
```
Check `license` against the workspace's other crates (`grep -n license crates/*/Cargo.toml`) and use the same value.

- [ ] **Step 2: Write the library**

`crates/wiiu-desktop-sys/src/lib.rs`:
```rust
//! Win32 calls for Wii U Connect's desktop app, behind safe functions. Empty on other platforms.
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{attach_parent_console, message_box, MessageKind};
```

`crates/wiiu-desktop-sys/src/windows.rs`:
```rust
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::System::Console::{AttachConsole, GetStdHandle, ATTACH_PARENT_PROCESS, STD_OUTPUT_HANDLE};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND,
};

/// A GUI-subsystem exe starts without a console. When it was started from a terminal and its output is not
/// redirected, attach to that terminal so `println!` shows there. Output already redirected to a file or pipe
/// (scripts, CI) is left alone. Returns true when it attached.
pub fn attach_parent_console() -> bool {
    // SAFETY: GetStdHandle and AttachConsole take no pointers; any returned handle is only compared.
    unsafe {
        let out = GetStdHandle(STD_OUTPUT_HANDLE);
        if !out.is_null() && out != INVALID_HANDLE_VALUE {
            return false;
        }
        AttachConsole(ATTACH_PARENT_PROCESS) != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Info,
    Error,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A modal message box with an OK button. Blocks until the user closes it.
pub fn message_box(title: &str, text: &str, kind: MessageKind) {
    let (title, text) = (wide(title), wide(text));
    let icon = match kind {
        MessageKind::Info => MB_ICONINFORMATION,
        MessageKind::Error => MB_ICONERROR,
    };
    // SAFETY: both buffers are NUL-terminated UTF-16 and outlive the call; a null owner window is allowed.
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_OK | icon | MB_SETFOREGROUND);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn wide_strings_are_nul_terminated_utf16() {
        assert_eq!(super::wide("Wii"), vec![0x57, 0x69, 0x69, 0]);
        assert_eq!(super::wide("é"), vec![0xe9, 0]);
    }
}
```

- [ ] **Step 3: Register it in the workspace**

In the root `Cargo.toml`, add `"crates/wiiu-desktop-sys"` to `members` (keep the list's order style) and under
`[workspace.dependencies]` add:
```toml
wiiu-desktop-sys = { path = "crates/wiiu-desktop-sys" }
```

- [ ] **Step 4: Check it builds and the test passes**

Run: `cargo build -p wiiu-desktop-sys && cargo test -p wiiu-desktop-sys && cargo clippy -p wiiu-desktop-sys -- -D warnings`
Expected on Linux: builds (empty crate), 0 tests. If `rustup target list --installed` shows
`x86_64-pc-windows-msvc`, also run `cargo check -p wiiu-desktop-sys --target x86_64-pc-windows-msvc`; otherwise
the Windows CI job is the check (Task 5).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates/wiiu-desktop-sys
git commit -m "feat(desktop): wiiu-desktop-sys crate for the Win32 console attach and message box"
```

---

### Task 2: Pure window decisions (`app_window` module)

**Files:**
- Create: `crates/wiiu-connect/src/app_window/mod.rs`
- Create: `crates/wiiu-connect/src/app_window/nav.rs`
- Create: `crates/wiiu-connect/src/app_window/message.rs`
- Modify: `crates/wiiu-connect/src/lib.rs` (add `pub mod app_window;`)
- Modify: `crates/wiiu-connect/Cargo.toml` (Windows-only `wiiu-desktop-sys` dependency)

**Interfaces:**
- Produces: `app_window::nav::stays_in_app(url: &str, port: u16) -> bool`,
  `app_window::nav::opens_in_browser(url: &str) -> bool`,
  `app_window::UiMode { Window, Browser, None }`,
  `app_window::ui_mode(window_supported: bool, no_browser: bool, no_window: bool) -> UiMode`,
  `app_window::WINDOW_SUPPORTED: bool`,
  `app_window::message::show(title: &str, text: &str, kind: MessageKind)`,
  `app_window::message::MessageKind { Info, Error }`,
  `app_window::message::applescript_string(s: &str) -> String`.

- [ ] **Step 1: Write the failing tests**

`crates/wiiu-connect/src/app_window/nav.rs`, tests first (module body empty except the tests):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_servers_pages_stay_in_the_window() {
        assert!(stays_in_app("http://localhost:8737/setup", 8737));
        assert!(stays_in_app("http://localhost:8737", 8737));
        assert!(stays_in_app("http://localhost:8737/?token=x", 8737));
        assert!(stays_in_app("http://localhost:8737#top", 8737));
        assert!(stays_in_app("about:blank", 8737));
    }

    #[test]
    fn everything_else_leaves_the_window() {
        assert!(!stays_in_app("http://localhost:8738/setup", 8737)); // other port (HTTPS listener)
        assert!(!stays_in_app("http://localhost:87370/setup", 8737)); // longer port
        assert!(!stays_in_app("http://localhost:8737@evil.example/", 8737)); // userinfo trick
        assert!(!stays_in_app("http://localhost:8737.evil.example/", 8737));
        assert!(!stays_in_app("https://localhost:8737/setup", 8737));
        assert!(!stays_in_app("http://127.0.0.1:8737/setup", 8737)); // the window always uses `localhost`
        assert!(!stays_in_app("https://github.com/cemu-project/Cemu", 8737));
        assert!(!stays_in_app("file:///C:/Windows/win.ini", 8737));
        assert!(!stays_in_app("javascript:alert(1)", 8737));
    }

    #[test]
    fn only_web_links_go_to_the_browser() {
        assert!(opens_in_browser("https://github.com/cemu-project/Cemu"));
        assert!(opens_in_browser("http://192.168.1.5:8737/?token=x"));
        assert!(opens_in_browser("HTTPS://EXAMPLE.COM/"));
        assert!(!opens_in_browser("file:///C:/Windows/win.ini"));
        assert!(!opens_in_browser("javascript:alert(1)"));
        assert!(!opens_in_browser("ms-settings:privacy"));
        assert!(!opens_in_browser("https://example.com/a b")); // spaces could split a shell argument
        assert!(!opens_in_browser("https://example.com/\n"));
    }
}
```

`crates/wiiu-connect/src/app_window/mod.rs`:
```rust
//! The desktop app window (ADR-0003). Pure decisions live here and in `nav`/`message`, so every platform tests
//! them; the window itself (`window`, Windows and macOS only) is added in a later task.

pub mod message;
pub mod nav;

/// Which UI to open on start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiMode {
    /// The app window (Windows, macOS).
    Window,
    /// The setup page in the default browser (Linux, `--no-window`).
    Browser,
    /// No UI (`--no-browser`; scripts and CI).
    None,
}

/// True where the app window is built in.
pub const WINDOW_SUPPORTED: bool = cfg!(any(windows, target_os = "macos"));

/// `--no-browser` means no UI at all; `--no-window` means today's console + browser behaviour.
pub fn ui_mode(window_supported: bool, no_browser: bool, no_window: bool) -> UiMode {
    if no_browser {
        UiMode::None
    } else if window_supported && !no_window {
        UiMode::Window
    } else {
        UiMode::Browser
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_mode_follows_the_flags() {
        assert_eq!(ui_mode(true, false, false), UiMode::Window);
        assert_eq!(ui_mode(true, false, true), UiMode::Browser);
        assert_eq!(ui_mode(true, true, false), UiMode::None);
        assert_eq!(ui_mode(true, true, true), UiMode::None);
        assert_eq!(ui_mode(false, false, false), UiMode::Browser); // Linux
        assert_eq!(ui_mode(false, true, false), UiMode::None);
    }
}
```

`crates/wiiu-connect/src/app_window/message.rs`, tests first:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applescript_strings_are_quoted_and_escaped() {
        assert_eq!(applescript_string("plain"), "\"plain\"");
        assert_eq!(applescript_string("say \"hi\""), "\"say \\\"hi\\\"\"");
        assert_eq!(applescript_string("C:\\path"), "\"C:\\\\path\"");
        assert_eq!(applescript_string("two\nlines"), "\"two\\nlines\"");
    }
}
```

In `crates/wiiu-connect/src/lib.rs` add `pub mod app_window;` next to the other `pub mod` lines.

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p wiiu-connect --lib app_window`
Expected: compile errors, `cannot find function stays_in_app` / `applescript_string`.

- [ ] **Step 3: Implement**

Top of `nav.rs` (above the tests):
```rust
//! Which URLs the app window keeps and which it hands to the default browser.

/// Pages of this Wii U Connect (`http://localhost:<port>`), and `about:blank`. Anything else leaves the window.
pub fn stays_in_app(url: &str, port: u16) -> bool {
    if url == "about:blank" {
        return true;
    }
    let base = format!("http://localhost:{port}");
    url.strip_prefix(&base)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(['/', '?', '#']))
}

/// Links the app may open in the default browser: http and https only, without whitespace or control characters,
/// so a page can't make the PC open files, settings or other apps.
pub fn opens_in_browser(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://"))
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}
```

Top of `message.rs` (above the tests):
```rust
//! A blocking native message: a Win32 message box on Windows, an AppleScript dialog on macOS, stderr elsewhere.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Info,
    Error,
}

/// An AppleScript string literal for `s`.
pub fn applescript_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Shows `text` and waits until the user closes the message.
pub fn show(title: &str, text: &str, kind: MessageKind) {
    #[cfg(windows)]
    wiiu_desktop_sys::message_box(
        title,
        text,
        match kind {
            MessageKind::Info => wiiu_desktop_sys::MessageKind::Info,
            MessageKind::Error => wiiu_desktop_sys::MessageKind::Error,
        },
    );
    #[cfg(target_os = "macos")]
    {
        let icon = match kind {
            MessageKind::Info => "note",
            MessageKind::Error => "stop",
        };
        let script = format!(
            "display dialog {} with title {} buttons {{\"OK\"}} default button \"OK\" with icon {icon}",
            applescript_string(text),
            applescript_string(title)
        );
        if std::process::Command::new("osascript").args(["-e", &script]).status().is_err() {
            eprintln!("{title}: {text}");
        }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = kind;
        eprintln!("{title}: {text}");
    }
}
```

In `crates/wiiu-connect/Cargo.toml` add:
```toml
[target.'cfg(windows)'.dependencies]
wiiu-desktop-sys.workspace = true
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p wiiu-connect --lib app_window && cargo clippy -p wiiu-connect --all-targets -- -D warnings`
Expected: 4 tests pass, no clippy warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/wiiu-connect/src/app_window crates/wiiu-connect/src/lib.rs crates/wiiu-connect/Cargo.toml Cargo.lock
git commit -m "feat(desktop): pure app-window decisions: URL rules, UI mode, native message"
```

---

### Task 3: `POST /api/app/show` and asking a running copy to show its window

**Files:**
- Modify: `crates/wiiu-connect/src/lib.rs` (`ServerConfig.show_window`, pass to `AppState`)
- Modify: `crates/wiiu-connect/src/http.rs` (`AppState.show_window`, route, handler)
- Modify: `crates/wiiu-connect/src/setup_http.rs:43` (`fn check_post` → `pub(crate) fn check_post`)
- Modify: `crates/wiiu-connect/src/app_window/mod.rs` (add `request_show`)
- Modify: every `ServerConfig { … }` literal: `crates/wiiu-connect/src/main.rs`, `crates/wiiu-connect/tests/end_to_end.rs`
  (2×), `crates/wiiu-connect/tests/setup.rs` (add `show_window: None`)
- Create: `crates/wiiu-connect/tests/app_show.rs`

**Interfaces:**
- Consumes: `setup_http::check_post(peer: SocketAddr, headers: &HeaderMap) -> Option<Response>`.
- Produces: `ServerConfig::show_window: Option<Arc<tokio::sync::Notify>>`,
  `AppState::show_window: Option<Arc<tokio::sync::Notify>>`,
  `app_window::request_show(port: u16) -> bool` (blocking; call from `spawn_blocking` or a non-async context).

- [ ] **Step 1: Write the failing integration tests**

`crates/wiiu-connect/tests/app_show.rs`:
```rust
//! `POST /api/app/show`: a second start brings the running window to the front; web pages can't.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use wiiu_connect::{start, Running, ServerConfig};

async fn server(show: Option<Arc<tokio::sync::Notify>>) -> (Running, tokio::sync::watch::Sender<bool>) {
    let (tx, rx) = tokio::sync::watch::channel(false);
    let (running, _task) = start(
        ServerConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            dsu_bind: "127.0.0.1:0".parse().unwrap(),
            token: Some("test-pairing-token-not-secret".into()),
            web_dir: None,
            embedded_web: false,
            setup: None,
            allowed_origins: vec![],
            public_host: None,
            https: None,
            show_window: show,
        },
        rx,
    )
    .await
    .unwrap();
    (running, tx)
}

async fn post(addr: SocketAddr, headers: &[(&str, String)]) -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    let mut req = format!(
        "POST /api/app/show HTTP/1.1\r\nHost: localhost:{}\r\nConnection: close\r\nContent-Length: 2\r\n",
        addr.port()
    );
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n{}");
    s.write_all(req.as_bytes()).await.unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).await.unwrap();
    out[9..12].parse().unwrap()
}

fn same_origin(addr: SocketAddr) -> Vec<(&'static str, String)> {
    vec![
        ("Content-Type", "application/json".into()),
        ("Origin", format!("http://localhost:{}", addr.port())),
    ]
}

#[tokio::test]
async fn the_second_copy_raises_the_window() {
    let show = Arc::new(tokio::sync::Notify::new());
    let (running, _stop) = server(Some(show.clone())).await;
    assert_eq!(post(running.http_addr, &same_origin(running.http_addr)).await, 204);
    tokio::time::timeout(Duration::from_secs(1), show.notified())
        .await
        .expect("window asked to show");
}

#[tokio::test]
async fn web_pages_cannot_raise_the_window() {
    let show = Arc::new(tokio::sync::Notify::new());
    let (running, _stop) = server(Some(show.clone())).await;
    let addr = running.http_addr;
    // A cross-site fetch: foreign Origin.
    let foreign = vec![
        ("Content-Type", "application/json".to_string()),
        ("Origin", "https://evil.example".to_string()),
    ];
    assert_eq!(post(addr, &foreign).await, 403);
    // A simple form post: no JSON content type.
    let form = vec![
        ("Content-Type", "text/plain".to_string()),
        ("Origin", format!("http://localhost:{}", addr.port())),
    ];
    assert_eq!(post(addr, &form).await, 403);
    assert_eq!(post(addr, &[]).await, 403);
    assert!(tokio::time::timeout(Duration::from_millis(200), show.notified()).await.is_err());
}

#[tokio::test]
async fn without_a_window_the_endpoint_is_404() {
    let (running, _stop) = server(None).await;
    assert_eq!(post(running.http_addr, &same_origin(running.http_addr)).await, 404);
}

#[tokio::test]
async fn request_show_reports_success_and_older_copies() {
    let show = Arc::new(tokio::sync::Notify::new());
    let (with_window, _a) = server(Some(show.clone())).await;
    let (without, _b) = server(None).await;
    let (p1, p2) = (with_window.http_addr.port(), without.http_addr.port());
    let ok = tokio::task::spawn_blocking(move || wiiu_connect::app_window::request_show(p1)).await.unwrap();
    assert!(ok);
    tokio::time::timeout(Duration::from_secs(1), show.notified()).await.unwrap();
    // An older copy (or one without a window) answers 404: the caller falls back to the browser.
    let ok = tokio::task::spawn_blocking(move || wiiu_connect::app_window::request_show(p2)).await.unwrap();
    assert!(!ok);
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p wiiu-connect --test app_show`
Expected: compile error, `struct ServerConfig has no field named show_window`.

- [ ] **Step 3: Implement**

`crates/wiiu-connect/src/lib.rs`, in `ServerConfig` after `https`:
```rust
    /// Set when the desktop window runs: `POST /api/app/show` notifies it to come to the front.
    pub show_window: Option<std::sync::Arc<tokio::sync::Notify>>,
```
Where `start()` builds `AppState { … }`, add `show_window: config.show_window.clone(),` (follow how `setup` is
passed; if `config` is moved earlier, clone the field before the move).

`crates/wiiu-connect/src/http.rs`, in `AppState`:
```rust
    /// The desktop window's "come to the front" signal; `None` without a window.
    pub show_window: Option<Arc<tokio::sync::Notify>>,
```
In `router()` add `.route("/api/app/show", post(app_show))` beside the other routes (import `axum::routing::post`
if needed), and the handler:
```rust
/// A second start asks this copy to bring its window to the front (ADR-0003). Same guard as the setup actions,
/// so a web page can't keep pulling the window forward.
async fn app_show(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Some(r) = crate::setup_http::check_post(peer, &headers) {
        return r;
    }
    match &state.show_window {
        Some(show) => {
            show.notify_one();
            StatusCode::NO_CONTENT.into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
```
`crates/wiiu-connect/src/setup_http.rs:43`: `fn check_post` → `pub(crate) fn check_post`.

`crates/wiiu-connect/src/app_window/mod.rs`, add:
```rust
/// Asks the Wii U Connect already running on `port` to bring its window to the front. False when it can't: no
/// answer, or an older version (or a copy started without a window) answering 404.
pub fn request_show(port: u16) -> bool {
    let origin = format!("http://localhost:{port}");
    ureq::post(&format!("{origin}/api/app/show"))
        .set("Origin", &origin)
        .set("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(2))
        .send_string("{}")
        .is_ok_and(|r| r.status() == 204)
}
```
Add `show_window: None,` to the `ServerConfig` literals in `tests/end_to_end.rs` (both) and `tests/setup.rs`, and
temporarily `show_window: None,` in `main.rs` (Task 4 replaces it).

- [ ] **Step 4: Run the tests**

Run: `cargo test -p wiiu-connect --test app_show && cargo test --workspace`
Expected: 4 new tests pass; the rest of the workspace still passes.

- [ ] **Step 5: Commit**

```bash
git add crates/wiiu-connect
git commit -m "feat(desktop): POST /api/app/show behind the setup CSRF guard, and request_show for a second start"
```

---

### Task 4: The window, the GUI subsystem and the new start-up flow

**Files:**
- Create: `crates/wiiu-connect/src/app_window/window.rs` (Windows and macOS only)
- Modify: `crates/wiiu-connect/src/app_window/mod.rs` (`#[cfg(any(windows, target_os = "macos"))] pub mod window;`)
- Modify: `crates/wiiu-connect/Cargo.toml` (`wry`, `tao` for Windows and macOS)
- Modify: `crates/wiiu-connect/src/main.rs` (subsystem, console attach, CLI parsed before the runtime, `--no-window`,
  hidden `--window-self-test`, split `serve` into start + UI)

**Interfaces:**
- Consumes: `app_window::{nav, ui_mode, UiMode, WINDOW_SUPPORTED, request_show, message}`,
  `ServerConfig::show_window`, `wiiu_connect::setup::open_browser(&str)`.
- Produces: `app_window::window::run(opts: WindowOptions, show: Arc<Notify>, rt: &tokio::runtime::Handle,
  on_exit: impl FnOnce(Outcome) -> i32 + 'static) -> !`,
  `pub struct WindowOptions { pub url: String, pub port: u16, pub self_test: bool }`,
  `pub enum Outcome { Closed, SelfTestOk(String), SelfTestFailed(String), WebViewUnavailable(String) }`.
  The hidden CLI flag `--window-self-test` prints `window ok <url>` (exit 0), `window fallback: <reason>` (exit 0,
  when the web view can't be created) or `window self-test failed: <reason>` (exit 1). With the environment variable
  `WIIU_CONNECT_TEST_NO_WEBVIEW=1`, the web view is treated as unavailable (for testing the fallback).

- [ ] **Step 1: Add the dependencies**

`crates/wiiu-connect/Cargo.toml`:
```toml
[target.'cfg(any(windows, target_os = "macos"))'.dependencies]
wry = { version = "0.57", default-features = false, features = ["os-webview"] }
tao = "0.37"
```
Check the icon id: open `winresource`'s `set_icon` source (`~/.cargo/registry/src/*/winresource-*/src/lib.rs`) and
confirm the id it uses; use that number in `window_icon()` below.
Run: `cargo tree -p wiiu-connect --target x86_64-pc-windows-msvc -i webview2-com-sys` and confirm it appears; run
`cargo tree -p wiiu-connect -i wry` on Linux and confirm *no* match (Linux stays free of WebKitGTK).

- [ ] **Step 2: Write the window**

`crates/wiiu-connect/src/app_window/window.rs`:
```rust
//! The app window: a `tao` window with a `wry` web view showing this server's setup page (ADR-0003).

use super::nav;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tao::dpi::LogicalSize;
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::WindowBuilder;
use tokio::sync::Notify;
use wry::{NewWindowResponse, PageLoadEvent, WebViewBuilder};

pub struct WindowOptions {
    /// `http://localhost:<port>/setup` (or `/pair` without setup).
    pub url: String,
    pub port: u16,
    /// `--window-self-test`: exit after the first page load of `url`, or fail after 30 s.
    pub self_test: bool,
}

#[derive(Debug)]
pub enum Outcome {
    Closed,
    SelfTestOk(String),
    SelfTestFailed(String),
    /// The web view could not be created (e.g. no WebView2 runtime).
    WebViewUnavailable(String),
}

enum UserEvent {
    Show,
    PageLoaded(String),
}

const SELF_TEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Windows: the icon `build.rs` embeds with `winresource` (resource id 1). macOS takes the bundle's `.icns`.
fn window_icon() -> Option<tao::window::Icon> {
    #[cfg(windows)]
    {
        use tao::platform::windows::IconExtWindows;
        tao::window::Icon::from_resource(1, None).ok()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn open_external(url: &str) {
    if nav::opens_in_browser(url) {
        crate::setup::open_browser(url);
    } else {
        tracing::warn!("blocked a link the window can't open: {url}");
    }
}

/// Runs the window on the calling (main) thread until it closes. `on_exit` runs exactly once with the outcome; it
/// stops the server and returns the process exit code.
pub fn run(
    opts: WindowOptions,
    show: Arc<Notify>,
    rt: &tokio::runtime::Handle,
    on_exit: impl FnOnce(Outcome) -> i32 + 'static,
) -> ! {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    {
        let proxy = proxy.clone();
        rt.spawn(async move {
            loop {
                show.notified().await;
                if proxy.send_event(UserEvent::Show).is_err() {
                    break;
                }
            }
        });
    }

    fn unavailable(reason: String, on_exit: Box<dyn FnOnce(Outcome) -> i32>) -> ! {
        std::process::exit(on_exit(Outcome::WebViewUnavailable(reason)))
    }
    let on_exit: Box<dyn FnOnce(Outcome) -> i32> = Box::new(on_exit);
    if std::env::var_os("WIIU_CONNECT_TEST_NO_WEBVIEW").is_some_and(|v| v == "1") {
        unavailable("forced by WIIU_CONNECT_TEST_NO_WEBVIEW".into(), on_exit);
    }

    let window = match WindowBuilder::new()
        .with_title("Wii U Connect")
        .with_inner_size(LogicalSize::new(1100.0, 800.0))
        .with_window_icon(window_icon())
        .build(&event_loop)
    {
        Ok(w) => w,
        Err(e) => unavailable(e.to_string(), on_exit),
    };

    let port = opts.port;
    let load_proxy = proxy.clone();
    let webview = WebViewBuilder::new()
        .with_url(&opts.url)
        .with_navigation_handler(move |url| {
            if nav::stays_in_app(&url, port) {
                true
            } else {
                open_external(&url);
                false
            }
        })
        .with_new_window_req_handler(|url, _features| {
            open_external(&url);
            NewWindowResponse::Deny
        })
        .with_on_page_load_handler(move |event, url| {
            if matches!(event, PageLoadEvent::Finished) {
                let _ = load_proxy.send_event(UserEvent::PageLoaded(url));
            }
        })
        .build(&window);
    let webview = match webview {
        Ok(v) => v,
        Err(e) => {
            drop(window);
            unavailable(e.to_string(), on_exit)
        }
    };

    let deadline = Instant::now() + SELF_TEST_TIMEOUT;
    let mut on_exit = Some(on_exit);
    event_loop.run(move |event, _, control_flow| {
        let _keep_alive = &webview;
        if !matches!(*control_flow, ControlFlow::ExitWithCode(_)) {
            *control_flow = if opts.self_test { ControlFlow::WaitUntil(deadline) } else { ControlFlow::Wait };
        }
        let outcome = match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => Some(Outcome::Closed),
            Event::UserEvent(UserEvent::Show) => {
                window.set_minimized(false);
                window.set_visible(true);
                window.set_focus();
                None
            }
            Event::UserEvent(UserEvent::PageLoaded(url)) if opts.self_test && url.starts_with(&opts.url) => {
                Some(Outcome::SelfTestOk(url))
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) if opts.self_test => Some(
                Outcome::SelfTestFailed(format!("{} did not load within {SELF_TEST_TIMEOUT:?}", opts.url)),
            ),
            _ => None,
        };
        if let (Some(outcome), Some(f)) = (outcome, on_exit.take()) {
            window.set_visible(false);
            *control_flow = ControlFlow::ExitWithCode(f(outcome));
        }
    })
}
```
Add to `app_window/mod.rs`:
```rust
#[cfg(any(windows, target_os = "macos"))]
pub mod window;
```

- [ ] **Step 3: Restructure `main.rs`**

1. First line of `crates/wiiu-connect/src/main.rs`:
```rust
// No console window on double-click; `main` re-attaches to the terminal it was started from (wiiu-desktop-sys).
#![cfg_attr(windows, windows_subsystem = "windows")]
```
2. Add to `ServeArgs`:
```rust
    /// Keep the console and open the setup page in the browser instead of the app window.
    #[arg(long)]
    no_window: bool,
    /// Test only: open the window, wait until the setup page has loaded, print the result and exit.
    #[arg(long, hide = true)]
    window_self_test: bool,
```
   and change the `no_browser` doc to `/// Open no UI on start (no app window, no browser).`
3. Replace `fn main` and `async fn run` with:
```rust
fn main() -> std::process::ExitCode {
    #[cfg(windows)]
    wiiu_desktop_sys::attach_parent_console();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,wiiu_dsu=info".into()),
        )
        .init();
    let cli = Cli::parse();
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let result = match cli.command.unwrap_or(Command::Serve(cli.serve)) {
        Command::Serve(args) => serve(runtime, args),
        Command::CemuProfile(args) => cemu_profile(args),
        Command::Probe(args) => runtime.block_on(probe(args)),
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("\nError: {e:#}");
            std::process::ExitCode::FAILURE
        }
    }
}
```
   (The old "Press Enter" wait is gone: a double-clicked GUI-subsystem exe has no console. Window-mode start-up
   errors show a message instead, below.)
4. Split today's `async fn serve(args)` into:
   - `async fn start_server(args: &ServeArgs, show: Option<Arc<Notify>>) -> anyhow::Result<Started>` with
     `enum Started { Fresh { running: Running, task: tokio::task::JoinHandle<()>, stop_tx: watch::Sender<bool> },
     AlreadyRunning { port: u16 } }`. It holds today's body from `let (stop_tx, stop_rx)` through the
     `AddrInUse` / `already_running` handling, but returns `AlreadyRunning { port }` instead of opening the browser.
     Use the real type of today's `task` (check `wiiu_connect::start`'s signature) in place of `JoinHandle<()>`.
   - `fn print_banner(running: &Running, setup: bool)`: today's `println!` block and terminal QR code, unchanged.
   - `fn serve(runtime: tokio::runtime::Runtime, args: ServeArgs) -> anyhow::Result<()>`:
```rust
fn serve(runtime: tokio::runtime::Runtime, args: ServeArgs) -> anyhow::Result<()> {
    use wiiu_connect::app_window::{ui_mode, UiMode, WINDOW_SUPPORTED};
    let mode = if args.window_self_test {
        UiMode::Window
    } else {
        ui_mode(WINDOW_SUPPORTED, args.no_browser, args.no_window)
    };
    let show = (mode == UiMode::Window).then(|| std::sync::Arc::new(tokio::sync::Notify::new()));
    let started = match runtime.block_on(start_server(&args, show.clone())) {
        Ok(s) => s,
        Err(e) if mode == UiMode::Window && !args.window_self_test => {
            wiiu_connect::app_window::message::show(
                "Wii U Connect",
                &format!("Wii U Connect could not start.\n\n{e:#}"),
                wiiu_connect::app_window::message::MessageKind::Error,
            );
            return Err(e);
        }
        Err(e) => return Err(e),
    };
    let (running, task, stop_tx) = match started {
        Started::AlreadyRunning { port } => {
            println!("Wii U Connect is already running.");
            if mode != UiMode::None {
                let shown = mode == UiMode::Window && wiiu_connect::app_window::request_show(port);
                if !shown {
                    wiiu_connect::setup::open_browser(&format!("http://localhost:{port}/setup"));
                }
            }
            return Ok(());
        }
        Started::Fresh { running, task, stop_tx } => (running, task, stop_tx),
    };
    print_banner(&running, !args.no_setup);
    let port = running.http_addr.port();
    let page = if args.no_setup { "pair" } else { "setup" };
    let local = format!("http://localhost:{port}/{page}");
    match mode {
        UiMode::Window => run_window(runtime, args.window_self_test, local, port, show.expect("set for Window"), task, stop_tx),
        UiMode::Browser | UiMode::None => {
            if mode == UiMode::Browser {
                wiiu_connect::setup::open_browser(&local);
            }
            runtime.block_on(async {
                tokio::signal::ctrl_c().await.context("waiting for Ctrl+C")?;
                tracing::info!("shutting down");
                let _ = stop_tx.send(true);
                let _ = tokio::time::timeout(Duration::from_secs(3), task).await;
                anyhow::Ok(())
            })
        }
    }
}
```
   - `run_window` (keep its parameter types matching `Started::Fresh`):
```rust
#[cfg(any(windows, target_os = "macos"))]
fn run_window(
    runtime: tokio::runtime::Runtime,
    self_test: bool,
    url: String,
    port: u16,
    show: std::sync::Arc<tokio::sync::Notify>,
    task: tokio::task::JoinHandle<()>,
    stop_tx: tokio::sync::watch::Sender<bool>,
) -> anyhow::Result<()> {
    use wiiu_connect::app_window::message::{show as message, MessageKind};
    use wiiu_connect::app_window::window::{run, Outcome, WindowOptions};
    let handle = runtime.handle().clone();
    let fallback_url = url.clone();
    run(WindowOptions { url, port, self_test }, show, &handle, move |outcome| {
        let code = match outcome {
            Outcome::Closed => 0,
            Outcome::SelfTestOk(url) => {
                println!("window ok {url}");
                0
            }
            Outcome::SelfTestFailed(reason) => {
                println!("window self-test failed: {reason}");
                1
            }
            Outcome::WebViewUnavailable(reason) if self_test => {
                println!("window fallback: {reason}");
                0
            }
            Outcome::WebViewUnavailable(reason) => {
                tracing::warn!("app window unavailable: {reason}");
                wiiu_connect::setup::open_browser(&fallback_url);
                message(
                    "Wii U Connect",
                    &format!(
                        "Wii U Connect couldn't open its own window, so setup opened in your browser instead.\n\n\
                         For the app window, install Microsoft's free WebView2 Runtime:\n\
                         https://developer.microsoft.com/microsoft-edge/webview2/\n\n\
                         Wii U Connect keeps running until you click OK.\n\n({reason})"
                    ),
                    MessageKind::Info,
                );
                0
            }
        };
        let _ = stop_tx.send(true);
        runtime.block_on(async {
            let _ = tokio::time::timeout(Duration::from_secs(3), task).await;
        });
        code
    })
}

#[cfg(not(any(windows, target_os = "macos")))]
fn run_window(
    _runtime: tokio::runtime::Runtime,
    _self_test: bool,
    _url: String,
    _port: u16,
    _show: std::sync::Arc<tokio::sync::Notify>,
    _task: tokio::task::JoinHandle<()>,
    _stop_tx: tokio::sync::watch::Sender<bool>,
) -> anyhow::Result<()> {
    anyhow::bail!("the app window is only available on Windows and macOS (use --no-window)")
}
```
   - In `start_server`, pass `show_window: show` into `ServerConfig` (replaces the Task 3 placeholder).
   - Keep `already_running()` and `is_addr_in_use()` as they are.

- [ ] **Step 4: Check that Linux behaviour is unchanged**

Run:
```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cargo run -p wiiu-connect -- --version
timeout 5 cargo run -p wiiu-connect -- --no-browser --no-https --no-auto-setup --bind 127.0.0.1:0 --data-dir /tmp/wiiu-plan-check; test $? -eq 124
cargo run -p wiiu-connect -- --window-self-test --bind 127.0.0.1:0 --no-https --no-auto-setup --data-dir /tmp/wiiu-plan-check; test $? -ne 0
```
Expected: checks pass. `--version` prints `wiiu-connect <version>`. The headless server runs until `timeout` stops it
(exit 124). The self-test on Linux fails with "only available on Windows and macOS".

- [ ] **Step 5: Commit**

```bash
git add crates/wiiu-connect Cargo.lock
git commit -m "feat(desktop): open the setup page in an app window on Windows and macOS"
```

---

### Task 5: CI and release checks, macOS app bundle

**Files:**
- Modify: `.github/workflows/ci.yml` (window self-tests in the Windows and macOS Rust jobs)
- Modify: `.github/workflows/release.yml` (dumpbin rule; window self-tests; macOS `.app`)
- Create: `crates/wiiu-connect/assets/Info.plist`
- Create: `scripts/make-icns.sh`
- Modify: `docs/release-notes-footer.md` (macOS instructions)

**Interfaces:**
- Consumes: the hidden `--window-self-test` flag and `WIIU_CONNECT_TEST_NO_WEBVIEW=1` (Task 4).

- [ ] **Step 1: CI self-tests**

In `.github/workflows/ci.yml`, in the "Rust (Windows build + test)" job after its test step, add:
```yaml
      - name: App window self-test (WebView2)
        shell: pwsh
        run: |
          $data = Join-Path $env:RUNNER_TEMP 'window-test'
          $out = cargo run -q -p wiiu-connect -- --window-self-test --bind 127.0.0.1:0 --no-https --no-auto-setup --data-dir $data | Out-String
          $out
          if ($out -notmatch 'window ok http://localhost:\d+/setup') { throw "window self-test: $out" }
          $env:WIIU_CONNECT_TEST_NO_WEBVIEW = '1'
          $out = cargo run -q -p wiiu-connect -- --window-self-test --bind 127.0.0.1:0 --no-https --no-auto-setup --data-dir $data | Out-String
          if ($out -notmatch 'window fallback: forced') { throw "fallback self-test: $out" }
```
In the macOS Rust job add:
```yaml
      - name: App window self-test (WKWebView)
        run: |
          out=$(cargo run -q -p wiiu-connect -- --window-self-test --bind 127.0.0.1:0 --no-https --no-auto-setup --data-dir "$RUNNER_TEMP/window-test")
          echo "$out"
          echo "$out" | grep -E 'window ok http://localhost:[0-9]+/setup'
```

- [ ] **Step 2: Release workflow, Windows**

In `.github/workflows/release.yml`, change the dumpbin rule to:
```powershell
          if ($deps -match '(?i)vcruntime|msvcp|api-ms-win-crt|ucrtbase') { throw 'the exe needs the Visual C++ runtime DLLs' }
          if ($deps -match '(?i)webview2loader') { throw 'the exe needs WebView2Loader.dll: the static loader must be linked' }
```
After the "Smoke test (the exe alone in an empty folder)" step add:
```yaml
      - name: Smoke test (app window opens the setup page)
        shell: pwsh
        run: |
          $exe = 'target/release/wiiu-connect.exe'
          $data = Join-Path $env:RUNNER_TEMP 'window-data'
          $out = & $exe --window-self-test --bind 127.0.0.1:0 --no-https --no-auto-setup --data-dir $data | Out-String
          $out
          if ($out -notmatch 'window ok http://localhost:\d+/setup') { throw "window self-test: $out" }
```
The existing smoke tests keep `--no-browser`, which now means no window either.

- [ ] **Step 3: macOS bundle files**

`crates/wiiu-connect/assets/Info.plist`:
```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Wii U Connect</string>
  <key>CFBundleDisplayName</key><string>Wii U Connect</string>
  <key>CFBundleIdentifier</key><string>io.github.alexianjjohnston-ai.wiiuconnect</string>
  <key>CFBundleExecutable</key><string>wiiu-connect</string>
  <key>CFBundleIconFile</key><string>wiiu-connect</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>__SHORT_VERSION__</string>
  <key>CFBundleVersion</key><string>__SHORT_VERSION__</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSLocalNetworkUsageDescription</key><string>Phones on your network connect to Wii U Connect as controllers.</string>
</dict>
</plist>
```
`scripts/make-icns.sh` (macOS only; uses the 256 px PNG inside the existing `.ico`):
```bash
#!/usr/bin/env bash
# Builds the macOS app icon from the Windows .ico (its entries are PNGs). Usage: scripts/make-icns.sh <out.icns>
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
out="$1"
work="$(mktemp -d)"
python3 - "$root/crates/wiiu-connect/assets/wiiu-connect.ico" "$work/icon_256.png" <<'PY'
import struct, sys
data = open(sys.argv[1], 'rb').read()
count = struct.unpack('<H', data[4:6])[0]
best = None
for i in range(count):
    w, h, _, _, _, _, size, off = struct.unpack('<BBBBHHII', data[6 + 16 * i:22 + 16 * i])
    w = w or 256
    if best is None or w > best[0]:
        best = (w, data[off:off + size])
assert best[1][:8] == b'\x89PNG\r\n\x1a\n', 'expected PNG entries in the .ico'
open(sys.argv[2], 'wb').write(best[1])
PY
set="$work/wiiu-connect.iconset"
mkdir "$set"
for s in 16 32 128 256; do
  sips -z "$s" "$s" "$work/icon_256.png" --out "$set/icon_${s}x${s}.png" >/dev/null
  d=$((s * 2)); [ "$d" -le 256 ] && sips -z "$d" "$d" "$work/icon_256.png" --out "$set/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns -o "$out" "$set"
```
Make it executable: `chmod +x scripts/make-icns.sh`.

- [ ] **Step 4: Release workflow, macOS**

Replace the macOS steps after "Universal binary" so the package is the `.app`:
```yaml
      - name: App bundle
        run: |
          app="package/Wii U Connect.app"
          mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
          mv package/wiiu-connect "$app/Contents/MacOS/wiiu-connect"
          short="${VERSION%%-*}"
          sed "s/__SHORT_VERSION__/$short/g" crates/wiiu-connect/assets/Info.plist > "$app/Contents/Info.plist"
          plutil -lint "$app/Contents/Info.plist"
          scripts/make-icns.sh "$app/Contents/Resources/wiiu-connect.icns"
          # Ad-hoc signature: required for arm64; not a Developer ID (Gatekeeper still asks once).
          codesign --force --deep -s - "$app"
          codesign --verify --deep "$app"
      - name: Smoke test
        run: |
          bin="package/Wii U Connect.app/Contents/MacOS/wiiu-connect"
          test "$("$bin" --version)" = "wiiu-connect $VERSION"
          out=$("$bin" --window-self-test --bind 127.0.0.1:0 --no-https --no-auto-setup --data-dir "$RUNNER_TEMP/window-test")
          echo "$out"
          echo "$out" | grep -E 'window ok http://localhost:[0-9]+/setup'
```
Keep today's existing smoke checks of the macOS step, pointing them at `$bin`, and remove the old ad-hoc
`codesign` of `package/wiiu-connect` (now done on the bundle). The "Package" step keeps zipping `package/`
with `ditto`, so the zip contains `Wii U Connect.app` plus the docs.

- [ ] **Step 5: Release notes footer**

In `docs/release-notes-footer.md`, replace the macOS bullet with:
```markdown
- **macOS (Apple Silicon and Intel):** `WiiUConnect-<version>-macos-universal.zip`. Unzip it and open
  **Wii U Connect.app**. It is not notarised yet, so the first time, right-click it and choose **Open**, then
  **Open** again (or run `xattr -dr com.apple.quarantine "Wii U Connect.app"` once in Terminal).
```
and in the Windows bullet replace "opens the setup page in your browser" with "opens its own window".

- [ ] **Step 6: Validate the workflow files and push a test branch**

Run: `python3 -c "import yaml; [yaml.safe_load(open(f)) for f in ['.github/workflows/ci.yml', '.github/workflows/release.yml']]; print('ok')"`
Commit, then push the branch and the same commit as `test/app-window` (runs the full Release workflow on Windows and
macOS):
```bash
git add .github/workflows crates/wiiu-connect/assets/Info.plist scripts/make-icns.sh docs/release-notes-footer.md
git commit -m "ci: app window self-tests, WebView2 loader check and the macOS app bundle"
git push -u origin feat/app-window && git push origin HEAD:refs/heads/test/app-window
```
Expected: CI and the Release run on `test/app-window` are green; the Windows log shows `window ok …/setup` and
`window fallback: forced…`; the macOS log shows `window ok …/setup`. A red run is fixed before the PR.

---

### Task 6: Documentation

**Files:**
- Modify: `CHANGELOG.md` (`[Unreleased]` → Added/Changed)
- Modify: `docs/progress.md` (new entry at the top)
- Modify: `docs/compatibility.md` (Platforms table: app window rows, unverified on real PCs)
- Modify: `docs/releasing.md` (what a release contains; the macOS `.app`; the window self-test and loader checks)
- Modify: `README.md` and `TESTING.md` (double-click opens a window; `--no-window`)
- Modify: `THIRD_PARTY_NOTICES.md` (wry, tao: Apache-2.0 OR MIT / Apache-2.0; Microsoft WebView2 loader, statically
  linked through `webview2-com-sys`)
- Modify: `CLAUDE.md` (Layout table: `crates/wiiu-desktop-sys`)
- Modify: `docs/adr/0003-desktop-app-window.md` (Status → Accepted; note the `--no-browser` meaning and that errors
  show a native message on both platforms)

- [ ] **Step 1: Write the entries.** CHANGELOG under `[Unreleased]`:
```markdown
### Changed
- **Wii U Connect opens its own window** on Windows and macOS instead of a console window and a browser tab. It
  shows the setup page and the phone QR code; closing it stops Wii U Connect. A second start brings the window to
  the front. Without Microsoft's WebView2 Runtime (Windows), setup opens in the browser with an explanation.
  `--no-window` keeps the old console behaviour; `--no-browser` opens no UI at all. Linux is unchanged.
- The macOS download is now **Wii U Connect.app**.
```
Progress entry: what was built, the tests (unit, `app_show.rs`, CI self-tests), what is unverified (real Windows
PCs without WebView2, Gatekeeper on a real Mac, the window icon on Windows).

- [ ] **Step 2: Run the full checks and commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add CHANGELOG.md docs README.md TESTING.md THIRD_PARTY_NOTICES.md CLAUDE.md
git commit -m "docs: the desktop app window"
git push origin feat/app-window
```
Then open the PR (release automation and a new FFI crate: owner approval needed) and subscribe to it.
