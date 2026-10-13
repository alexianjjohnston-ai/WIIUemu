# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- **Automatic setup on start**, shown as a live checklist on the setup page. Each step can be turned off with
  `--no-auto-setup`, and every step remains a button.
  - Uses an existing Cemu (a located one, or one installed with Cemu's installer in `%LOCALAPPDATA%\Cemu`).
    Otherwise it downloads and installs the official release.
  - Points Cemu's controllers at the phones, backing up any existing settings.
  - Turns on Cemu's separate GamePad window.
  - Finds the folders that hold your own game dumps (Downloads, Documents, Desktop, Games, Videos, and game-named
    folders on other drives) and adds them to Cemu.
- The phone pairing QR code is now on the setup page itself.
- **Release packaging:**
  - The Windows download is one self-contained `WiiUConnect-<version>-windows-x64.exe` with the C runtime linked
    statically, an app icon, version information and an `asInvoker` manifest.
  - Each release build is checked: the phone app is embedded, the exe runs alone in an empty folder (HTTP and
    HTTPS), there are no runtime DLL dependencies, and the version matches the tag.
  - Draft GitHub releases get notes from CHANGELOG.md and SHA256 checksums.
  - `scripts/set-version.sh` and `docs/releasing.md`.
- Startup:
  - The version is shown in the console, on `--version` and on the setup page.
  - Starting a second copy opens the running copy's setup page instead of failing.
  - A port taken by another program gets a clear message.
  - When Wii U Connect is started by double-click, an error keeps the window open until Enter.
- **Setup page** (`/setup`, opens automatically on start; PC-only and CSRF-protected):
  - Downloads the official Cemu release (Windows zip, Linux AppImage) into a portable folder, or uses your existing
    Cemu.
  - Points Cemu's Controllers 2–4 at the phone Wii Remotes, backing up existing settings.
  - Adds your own game folders to Cemu, lists the games and launches them.
  - It never downloads games, keys or system files.
- Single-file release: the phone app is compiled into `wiiu-connect` (no `web` folder).

### Changed
- Camera pointing, after the first real-phone test:
  - Range: frames are now up to 960 px, tags 20% of the game height, and detection runs at 1.5× decimation on up to
    4 threads. Tags no longer vanish beyond about 1 m.
  - The gyro is checked online against the camera (corrects swapped or flipped axes and wrong scale, or is ignored
    when it doesn't help).
  - The pointer is smoothed with a One Euro filter.
  - `/tracking` shows tracking diagnostics per player.

### Security
- Server (QA review fixes):
  - Pad updates are published under the session lock, so a tracking worker can't leave a button held after a
    disconnect or watchdog release.
  - Connections idle for 10 s are closed.
  - Gyro history is capped at 1024 samples, and gyro input at 1000 samples/s.
  - Camera frames whose JPEG size differs from the header are rejected.
  - `--token` must be 16–128 URL-safe characters.
- Phone app: production builds ignore the `?server=` URL override, so a crafted link can't send the pairing
  token to another host.

### Added
- `wiiu-dsu`: Cemuhook DSU protocol v1001 server, byte-compatible with Cemu's DSU client. Includes a Cemu-like
  probe client.
- `wiiu-controller`: Wii Remote and GamePad button tables, phone protocol v1 validation, input → DSU
  conversion (pointer → touch point 1920×942), and a monotonic motion clock.
- `wiiu-cemu`: Cemu controller profile generation for DSU-bound Wii Remotes.
- `wiiu-connect`: session server with token pairing, QR pairing page, slot assignment, reconnect, staleness
  watchdog, mouse pointer test page, and `cemu-profile` / `probe` commands.
- macOS support: Cemu profile folder detection, macOS CI job, universal (arm64 + x86_64) release package.
- Release workflow producing Windows and macOS zips; `v*` tags create a draft GitHub release.
- `apps/phone`: Wii Remote PWA (React + TypeScript). Authentic and comfort layouts, left-handed mode, button
  size, multi-touch, haptics where supported, reconnect with backoff, release on hide/blur/disconnect, and
  Playwright e2e tests against a mocked server.
- Camera pointing (protocol v2): `wiiu-tracking` (pointing math, gyro fusion, tag rendering), vendored AprilTag
  detector, a server tracking worker per Wii Remote, HTTPS for phones, the `/tracking` TV border page, and the
  phone app's camera and gyro streaming with on-screen tracking feedback. Not yet tested on real phones.
- CI for Rust (Linux and Windows), the phone app, and security scanning.
- Documentation: architecture, protocol, research, tracking design, tools, compatibility, progress.
