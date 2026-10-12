# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

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
- `apps/phone`: Wii Remote PWA (React + TypeScript). Authentic and comfort layouts, left-handed mode, button
  size, multi-touch, haptics where supported, reconnect with backoff, release on hide/blur/disconnect, and
  Playwright e2e tests against a mocked server.
- Camera pointing (protocol v2): `wiiu-tracking` (pointing math, gyro fusion, tag rendering), vendored AprilTag
  detector, a server tracking worker per Wii Remote, HTTPS for phones, the `/tracking` TV border page, and the
  phone app's camera and gyro streaming with on-screen tracking feedback. Not yet tested on real phones.
- CI for Rust (Linux and Windows), the phone app, and security scanning.
- Documentation: architecture, protocol, research, tracking design, tools, compatibility, progress.
