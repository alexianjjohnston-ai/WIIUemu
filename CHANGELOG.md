# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- Release workflow: a manual run on `main` with **Make the release** ticked tags the built commit and creates the
  GitHub release, for when a tag can't be pushed. Pre-releases are published at once; full versions stay drafts.

## [0.9.0-beta.1] - 2026-10-13

First beta. Every feature below is tested in CI and with simulated phones, but not yet on real phones, a real TV
and real games: please report what works and what doesn't (see TESTING.md). Camera pointing currently needs the
`/tracking` page on the TV, so it can't yet be used while a game is running (docs/roadmap.md, item 1.2).

### Added
- **Pointer calibration** for camera pointing. Tap **Calibrate pointer** and aim at the cross in the middle of the
  TV page during a 3-second countdown. The PC measures where the phone's camera looks when the player aims at the
  centre. The phone remembers it and sends it again on every connection; **Reset** goes back to the default.
  Protocol: `calibrate`, `aim` and `calibration` messages (protocol 2).
- **Phone GamePad** (phone 1, landscape):
  - shoulder buttons, both sticks, D-pad, ABXY, −/HOME/+ and L3/R3;
  - a 16:9 touchscreen that becomes Cemu's GamePad touch input.
- **Motion controls** for the GamePad and the Wii Remotes, from the phone's sensors, in Cemu's DSU axis convention.
- The setup page now also binds Cemu's Controller 1 to the phone GamePad.
- `docs/release-1.0.md`: 1.0 scope, work packages and the hardware checklist.
- **Automatic setup on start**, shown as a live checklist on the setup page. Each step can be turned off with
  `--no-auto-setup`, and every step remains a button.
  - Uses an existing Cemu (a located one, or one installed with Cemu's installer in `%LOCALAPPDATA%\Cemu`).
    Otherwise it installs the official Cemu release built into the Windows exe, or downloads it.
  - Points Cemu's controllers at the phones, backing up any existing settings.
  - Turns on Cemu's separate GamePad window.
  - Finds the folders that hold your own game dumps (Downloads, Documents, Desktop, Games, Videos, and game-named
    folders on other drives) and adds them to Cemu.
- The phone pairing QR code is now on the setup page itself.
- **The Windows release exe includes the official Cemu release** (unmodified, MPL-2.0; license and source link are
  written to `CEMU-LICENSE.txt` next to it). The first start needs no internet; **Update Cemu** still downloads the
  newest release. The release workflow fetches the latest Cemu at build time from Cemu's GitHub releases, checks its
  URL and size (and GitHub's SHA-256 digest when one is published; Cemu v2.6 has none) and logs its SHA-256.
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
- `wiiu-dsu`: Cemuhook DSU protocol v1001 server, byte-compatible with Cemu's DSU client. Includes a Cemu-like
  probe client.
- `wiiu-controller`: Wii Remote and GamePad button tables, phone protocol v1 validation, input → DSU
  conversion (pointer → touch point 1920×942), and a monotonic motion clock.
- `wiiu-cemu`: Cemu controller profile generation for DSU-bound Wii Remotes.
- `wiiu-connect`: session server with token pairing, QR pairing page, slot assignment, reconnect, staleness
  watchdog, mouse pointer test page, and `cemu-profile` / `probe` commands.
- macOS support: Cemu profile folder detection, macOS CI job, universal (arm64 + x86_64) release package.
- Release workflow; `v*` tags create a draft GitHub release.
- `apps/phone`: Wii Remote PWA (React + TypeScript). Authentic and comfort layouts, left-handed mode, button
  size, multi-touch, haptics where supported, reconnect with backoff, release on hide/blur/disconnect, and
  Playwright e2e tests against a mocked server.
- Camera pointing (protocol v2): `wiiu-tracking` (pointing math, gyro fusion, tag rendering), vendored AprilTag
  detector, a server tracking worker per Wii Remote, HTTPS for phones, the `/tracking` TV border page, and the
  phone app's camera and gyro streaming with on-screen tracking feedback. Not yet tested on real phones.
- CI for Rust (Linux and Windows), the phone app, and security scanning.
- Documentation: architecture, protocol, research, tracking design, tools, compatibility, progress.
- `docs/roadmap.md`: the plan for the rest of 1.0 and the features after it.

### Fixed
- In left-handed mode, the camera status line covered the top of the B button. The camera lines now sit on the
  right there and are capped at the space beside the B zone.
- iPhone gyro axes. WebKit reports DeviceMotion `rotationRate` in a different axis order from the spec, so camera
  pointing on iOS received permuted gyro data.
- Setup:
  - updating Cemu can no longer lose the `portable` folder when Cemu is running;
  - controller profiles use the actual DSU port;
  - the setup page no longer starts overlapping game-folder scans.

### Changed
- Camera pointing, after the first real-phone test:
  - Range: frames are now up to 960 px, tags 20% of the game height, and detection runs at 1.5× decimation on up to
    4 threads. Tags no longer vanish beyond about 1 m.
  - The gyro is checked online against the camera (corrects swapped or flipped axes and wrong scale, or is ignored
    when it doesn't help).
  - The pointer is smoothed with a One Euro filter.
  - `/tracking` shows tracking diagnostics per player.

### Security
- Release and CI workflows pin every third-party GitHub Action to a full commit SHA, so a moved tag can't change
  what a release build runs.
- Server (QA review fixes):
  - Pad updates are published under the session lock, so a tracking worker can't leave a button held after a
    disconnect or watchdog release.
  - Connections idle for 10 s are closed.
  - Gyro history is capped at 1024 samples, and gyro input at 1000 samples/s.
  - Camera frames whose JPEG size differs from the header are rejected.
  - `--token` must be 16–128 URL-safe characters.
- Phone app: production builds ignore the `?server=` URL override, so a crafted link can't send the pairing
  token to another host.
