# ADR-0003: Show the PC side in its own app window, not a browser tab

**Status:** Proposed (approved in conversation 2026-10-13; this written design awaits the owner's review)
**Date:** 2026-10-13
**Deciders:** repository owner; lead engineer

## Context

Today the exe is a console program. It starts the server, prints addresses and a QR code in the console, and opens
`http://localhost:8737/setup` in the default browser. The owner asked to "move away from using a localhost website
to just all in an exe or mac". The phones are not part of this: they must keep opening the controller in their
browser, because a store app needs Apple and Google developer accounts and reviews (roadmap, after 1.0).

Forces:

- **One file, nothing to install** (the release promise, `docs/releasing.md`). The release workflow fails if the exe
  depends on runtime DLLs (`dumpbin` check).
- **The setup UI already exists** (`web/setup.html`, `pair.html`, `tracking.html`), served by the same process, and
  protected by `reject_remote` and a CSRF check. Rewriting it natively would duplicate it.
- **Facts checked 2026-10-13:**
  - `wry` 0.57 (Apache-2.0 OR MIT) shows web content in a native window: WebView2 on Windows, WKWebView on macOS,
    WebKitGTK on Linux. `tao` 0.37 (Apache-2.0) creates the window and runs the event loop.
  - On MSVC, `webview2-com-sys` 0.39.1 links `WebView2LoaderStatic.lib`
    (`#[cfg_attr(target_env = "msvc", link(name = "WebView2LoaderStatic", kind = "static"))]`), so no
    `WebView2Loader.dll` ships next to the exe.
  - The WebView2 *runtime* is part of Windows 11 and is installed on almost all Windows 10 PCs through Edge
    updates. It can be missing on a PC that never received them.
  - `tao` requires its event loop on the main thread on macOS.
- **Unverified until built:** that `WebView2LoaderStatic.lib` links cleanly with our static C runtime
  (`.cargo/config.toml`, `+crt-static`), and how much the exe grows.

## Decision

**On Windows and macOS, Wii U Connect opens its own window titled "Wii U Connect" that shows the setup page in an
embedded web view (`wry` + `tao`). No browser tab opens and no console window shows. On macOS the download becomes
`Wii U Connect.app`.** Linux keeps today's behaviour, so Linux builds need no WebKitGTK.

## Options Considered

| Option | For | Against |
|--------|-----|---------|
| **A. Embedded web view (chosen)** | Reuses the existing pages and their security checks; small change; native window on both platforms | Needs the WebView2 runtime on Windows; adds `wry`/`tao` and their dependencies |
| B. Native UI toolkit (egui, iced, Slint) | No web view at all | Rebuilds every setup page; two UIs to keep in sync with the phone app's look; weeks of work |
| C. Tauri | Full app framework with bundler | Brings its own build system, config and IPC; much more than one window needs; harder to keep the one-file exe |

## Design

### Process and threads

- `main` builds the tokio runtime as today, but the server runs on runtime worker threads (`runtime.spawn`). The
  main thread runs the `tao` event loop (required on macOS).
- **Closing the window quits Wii U Connect:** the server stops and every phone's input is released, exactly like
  Ctrl+C today. The window title says "Wii U Connect — running"; closing it is the only way to stop, as with the
  console window today.
- A second start finds the first copy (today's `already_running` check) and asks it to bring its window to the front
  (`POST /api/app/show`, loopback only, through `reject_remote`), then exits.

### Window and pages

- The web view loads `http://localhost:<port>/setup`. Loopback peer and Host, so `reject_remote` and the CSRF check
  apply unchanged. The pairing QR code is already on that page.
- Links that open new windows (`target="_blank"`, `window.open`), and any navigation away from
  `http://localhost:<port>`, open in the default browser instead (`with_new_window_req_handler`,
  `with_navigation_handler`). The `/tracking` TV page therefore still opens in the browser until ADR-0004 replaces
  it with the native TV border.
- Size 1100×800, resizable, remembers nothing (YAGNI). The app icon is the existing `.ico`.

### Windows console

- The exe switches to the Windows GUI subsystem (`#![windows_subsystem = "windows"]`), so double-clicking shows no
  console.
- Command-line use keeps working: on start the exe calls `AttachConsole(ATTACH_PARENT_PROCESS)`, so `--version`,
  `probe`, `cemu-profile` and `--no-window` print to the terminal they were started from. Output redirected to a
  file or pipe (the release smoke tests) works through the inherited handles.
- Startup errors that today wait for Enter in the console show a native message box instead.

### Fallbacks and flags

- If the web view can't be created (WebView2 runtime missing, or an error), Wii U Connect opens the setup page in the
  browser, as today, and shows a message box that explains this and links Microsoft's WebView2 runtime download.
- `--no-window`: today's console behaviour (server, console output, browser unless `--no-browser`). Used on Linux
  always, and by CI.

### macOS app bundle

The release workflow packages `Wii U Connect.app` (`Contents/MacOS/wiiu-connect`, `Info.plist` with bundle id
`io.github.alexianjjohnston-ai.wiiuconnect`, `LSMinimumSystemVersion` 11.0, an `.icns` made from the existing icon)
and zips it. Without notarisation, Gatekeeper asks once: the release notes explain right-click → Open.

## Testing

- **Unit:** the navigation rule (which URLs stay in the window, which go to the browser) is a pure function with
  tests. The `--no-window` / Linux choice is tested through argument parsing.
- **Windows CI and release:** a hidden `--window-self-test` flag starts the app, waits for the web view's
  page-load event of `/setup`, prints `window ok <url>` and exits. The release smoke test runs it on the Windows
  runner, which has the WebView2 runtime. The existing `dumpbin` check proves no `WebView2Loader.dll` dependency.
- **macOS CI and release:** the same self-test on the macOS runner, plus a check of the bundle layout and
  `Info.plist`.
- **Owner:** double-click on a real Windows PC and Mac; recorded in `docs/compatibility.md`.

## Consequences

- No console window and no browser tab: one window, like a normal app.
- The exe grows (estimated a few MB; measured in the PR).
- PCs without the WebView2 runtime fall back to the browser with an explanation.
- Linux is unchanged.
- The phones are unchanged.
