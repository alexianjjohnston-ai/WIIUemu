# Progress log

Hand-off notes for the next session. Newest first.

## 2026-10-12: Milestone 2 (software side): camera pointing

Branch `feat/apriltag-tracking` (PR #3), stacked on PR #1.

### Done
- `wiiu-tracking`: homography, aim point, calibration, gyro fusion, tracker with latency replay, tag36h11
  rendering. `wiiu-apriltag-sys`: vendored AprilTag C behind a primitive-only shim (builds on Linux, Windows MSVC
  and macOS in CI).
- Server: protocol v2 (`camera_frame`, `gyro`, `tracking`), a tracking worker per Wii Remote slot, the HTTPS
  listener (self-signed, port 8738), the `/tracking` border page, and `/api/tracking/layout`.
- Phone: camera button, rear-camera JPEG streaming, gyro batches, tracking feedback line, camera released while
  hidden. Rust and TS camera-frame encoders are pinned to the same golden bytes.
- Simulated end-to-end check: a fake Chromium camera played the `/tracking` border into the real server, and all
  10 tags were found. Pointer (0.5, 0.5) → DSU touch (960, 471). See `docs/tracking.md`.

### Tests
- Phone: 107 unit tests and 77 Playwright e2e tests (including 4 camera tests with Chromium's fake camera).

### Unverified
- Everything on real phones: camera mode chosen, fps, latency, iOS motion permission, gyro axis signs, battery.
- Inside Cemu: the camera-driven pointer in a real game.

### Next
1. Real-phone run: open the https QR link, accept the certificate, turn on the camera, aim at `/tracking` on the
   TV. Record fps, aim error and latency in `docs/tracking.md`.
2. Calibration UI (aim at the centre ring → `calibrate_aim_point`).
3. Timing metrics on `/api/status` (ADR-0001 action item 3).

## 2026-10-12: Milestone 0 + Milestone 1 (software side)

### Done
- Repository bootstrapped (`main`), feature branch `feat/virtual-wiimote`.
- **Key finding:** an unmodified Cemu can take an independent Wii Remote IR pointer per player from DSU touch
  point 1 (see `docs/research.md` §1). This removes the need for a Cemu patch for Milestones 1, 3 and 4.
- `wiiu-dsu`: DSU server, with layouts verified against Cemu's packed structs (g++ `offsetof` check).
- `wiiu-controller`: protocol v1 validation, button tables, pointer/motion conversion.
- `wiiu-cemu`: Cemu profile XML (`WiiUConnect_Wiimote{1,2,3}.xml`).
- `wiiu-connect`: server binary (WebSocket, pairing, safety rules, `/pair`, `/dev/pointer`, `probe`,
  `cemu-profile`).
- Live check in the container: Playwright drove `/dev/pointer` while `wiiu-connect probe` received the
  expected DSU data. Mouse at (0.25, 0.75) became touch (480, 707); A became Cross; H became L3; the slot went
  neutral after the page closed.
- Phone Wii Remote PWA (`apps/phone`) merged. The agent that built it stopped before reporting; its uncommitted
  work was reviewed, committed and merged by the lead. Live check in the container: the real PWA (Playwright,
  Pixel 7 emulation) served by `wiiu-connect` paired, lit LED 1, and a touch on A showed up in `probe` as Cross
  (bit 14). The bit cleared on release; the slot freed when the page closed.
- Security fix from the security-guidance review: `?server=` is honoured only by the Vite dev server.

### Tests
- Phone: 88 unit tests (Vitest) and 69 Playwright e2e tests (Pixel 7 + iPhone 13 viewports, Chromium).
- Rust: 63 tests (unit + loopback UDP + end-to-end WebSocket → DSU) pass; `clippy -D warnings` and `fmt` are
  clean.

### Blocked / unverified
- **No Cemu or Windows in the development container.** Nothing has been verified inside Cemu or a real game
  yet. Milestone 1 is not complete until someone on Windows with Cemu confirms in a pointer game (e.g. Wii Party
  U):
  1. `wiiu-connect cemu-profile` profiles load, and buttons respond.
  2. `/dev/pointer` moves the in-game pointer, and the Y direction is correct.
  3. Two pointer pages (two browser profiles) move two independent pointers.
- Physical phone behaviour (multi-touch, haptics) is unverified.

### Next
1. Try the PWA on real phones (multi-touch, haptics, wake lock, iOS Safari).
2. Windows + Cemu verification of the three checks above. Record the results in `docs/compatibility.md`.
3. Milestone 2: HTTPS for phones (needed for camera and motion), then the AprilTag border renderer and
   detection prototype (`docs/tracking.md`).
4. Wire phone motion sensors (DeviceMotion) to the protocol `motion` field, then verify the axis convention in
   Cemu.
