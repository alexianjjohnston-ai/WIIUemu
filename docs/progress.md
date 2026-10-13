# Progress log

Hand-off notes for the next session. Newest first.

## 2026-10-13: Releases without a tag push (branch `ci/release-from-workflow`)

- Pushing `v0.9.0-beta.1` from the session failed (HTTP 403 on tag pushes), and the owner works from a phone. The
  owner asked me to make the release, so the Release workflow gained a manual **Make the release** input: on
  `main` it checks the version, the changelog section and that the tag is new, runs the full build and smoke
  tests, and creates the release on the built commit. Pre-releases are published by the workflow; full versions
  stay drafts for the owner's hardware check.
- **Owner decisions on the roadmap (2026-10-13):** beta yes; marker border during play with a smaller game
  picture (option A); border before streaming; no code signing unless it becomes necessary; after-1.0 order left
  to me (kept as in docs/roadmap.md).

## 2026-10-13: First beta, `v0.9.0-beta.1` (branch `chore/release-0.9.0-beta.1`)

- The owner approved merging #23 (roadmap) and #24 (pinned actions) and asked for a release version. The release
  is the beta proposed in docs/roadmap.md 1.1, not 1.0: real phones, a real TV and real games are still untested.
- Version set with `scripts/set-version.sh 0.9.0-beta.1`. The `[Unreleased]` changelog became the
  `[0.9.0-beta.1]` section (the never-released 0.1.0 list folded into its Added list), with a note on what is
  untested and that camera pointing in a running game waits for roadmap 1.2.
- After merge: tag `v0.9.0-beta.1` on the merge commit. The Release workflow makes a draft pre-release; the owner
  checks it with the hardware checklist (docs/release-1.0.md) and presses Publish.
- **Next:** the owner's hardware pass, then the roadmap decisions.

## 2026-10-13: Pointer calibration (branch `feat/pointer-calibration`, PR #22)

- **Protocol** (protocol 2, docs first): `calibrate` (phone → PC), `aim` with a stored point or `null`, and the
  `calibration` result with `no_markers` / `off_image` reasons.
- **Server:** the tracking worker takes the next fix after `calibrate` (3 s limit) and calls
  `calibrate_aim_point` for the game centre. It keeps the aim point as an image fraction, so it survives
  frame-size changes, and answers on a separate channel so the 10 Hz status can't overwrite the result.
- **Phone:** `CameraPointer` runs a 3 s countdown, then measures. It stores the point in `localStorage` and
  re-sends it when the camera starts and after every reconnect. A compact one-line row offers Calibrate,
  Recalibrate and Reset only while the PC sees the border.
- **Layout fix found by the new overlay test:** in left-handed mode the camera line covered the B button (an older
  bug). Camera lines are now right-aligned there.
- **Tests:** server e2e renders the border off-centre; calibration reports (0.6, 0.45) and the pointer then lands
  on the TV centre, also after a reconnect with `aim`. Other tests cover timeout, validation, phone unit tests
  with fake timers, a Playwright flow, and an overlay check in 6 layouts × 2 phones.
- **Unverified:** real phones (does one centre calibration fix the offset across the whole screen?).
- **Next:** GamePad screen streaming (work package 5, ADR-0002).

## 2026-10-13: One-file release that sets itself up (branch `feat/release-packaging`, PR #21)

- **The release exe is self-contained.** It has the phone app embedded, a static C runtime, an icon and version
  info, and the official Cemu release built in (Windows). Cemu is fetched at release build time; its URL and size
  are checked, plus GitHub's SHA-256 digest when one is published (none for v2.6). The MPL-2.0 notice is written as
  `CEMU-LICENSE.txt`.
- **Automatic setup on start.** It uses or installs Cemu (built-in first, then download), assigns controllers, turns
  on Cemu's GamePad window (`open_pad`) and finds game folders. The setup page shows a live checklist and the
  pairing QR code. `--no-auto-setup` turns it off.
- **Release workflow.** A tag produces a draft release with notes and checksums. `test/**` branches run the same
  builds and smoke tests on real Windows and macOS runners.
- **Verified on CI runners:** release run 38380958236 (`7d2edf2`) passed on Windows: no CRT DLLs, the exe alone
  in an empty folder, the automatic setup installing the built-in Cemu v2.6 with no download, then updating it by
  download.
- **Unverified:** the owner's PC (SmartScreen, firewall prompt, game discovery on real folders).
- **Merged** after #20: the automatic setup now assigns all four controllers (`assign_controllers`), including the
  phone GamePad on Controller 1.

## 2026-10-12/13: Release 1.0 started (branch `feat/gamepad-controller`)

PRs #1–#5 are merged into `main` with merge commits. The owner chose that 1.0 **includes GamePad screen streaming**.
The plan is in `docs/release-1.0.md`.

### Done
- **Phone GamePad:**
  - `components/gamepad/*` and `GamepadSession`;
  - the engine carries sticks, touch and motion, coalesced to 16 ms;
  - buttons and touch edges are sent at once.
- **Motion:**
  - `motion/convert.ts` and `MotionSensor`. The DSU convention comes from Cemu's motion handler (research.md §3).
  - Engine-aware axes: WebKit reports `rotationRate` as x/y/z, while the spec says z/x/y (research.md §6b). This
    also fixes the camera pointer's gyro on iPhones.
- **Setup** assigns `controller0.xml` = WiiUConnect_GamePad. Profiles follow the bound DSU address (review fix
  on #5).
- **Review fixes on #5:**
  - a Cemu update keeps `portable` safe when Cemu is running;
  - game scans are cached, and status polls are sequential.

### Tests
- 141 Rust tests. Phone: 141 unit tests and 101 Playwright e2e tests, including 12 GamePad tests and Wii Remote
  motion in both engine conventions.

### Unverified
- Everything on real hardware:
  - GamePad layout on real phones;
  - stick and motion directions in Cemu;
  - touch accuracy against Cemu's GamePad view;
  - iOS motion permission flow.
- The checklist is in `docs/release-1.0.md`.

### Next
1. Open the PR for `feat/gamepad-controller`; the owner tests with the checklist.
2. Work package 5, streaming:
   - ADR-0002;
   - a source check of Cemu's GamePad window title;
   - a Windows.Graphics.Capture prototype;
   - an H.264 encoder;
   - WebCodecs on the phone with a JPEG fallback.
3. Pointer calibration UI.

## 2026-10-12: Setup page and single-file release (branch `feat/setup-wizard`)

The owner asked for one exe that handles emulator and game setup, "integrating Cemu and a Wii U downloader".

**Declined:** the game downloader. Downloading Wii U games from Nintendo's servers with title keys is piracy, and
the project rules forbid obtaining or bundling games, keys or system files.

**Built instead:**
- **Setup page** `/setup`, opened automatically on start:
  - Downloads the official Cemu release (MPL-2.0) from GitHub into `<data dir>/Cemu` (portable).
  - Assigns the Wii Remote profiles to Controllers 2–4, with backups.
  - Adds the user's own game folders to `settings.xml`, scans them (`.wua`, `.wud`, `.wux`, `.iso` and unpacked
    `code/` + `meta/meta.xml`) and launches games with `Cemu -g`.
  - Only games from the scanned list can be launched.
- **Single exe:** `build.rs` embeds `apps/phone/dist`.

**Verified:**
- 135 Rust tests, including end-to-end API tests: the setup flow, CSRF and DNS-rebinding refusal, and that only
  scanned games launch.
- Live in the container with Playwright: auto-redirect, locate, controllers, games, and the download-error
  display. The release binary served the embedded phone app with no `web` folder.

**Not verified here:**
- The real Cemu download. GitHub's API is blocked by this container's network policy. The Windows release job
  now runs a smoke test that downloads and sets up the real Cemu on a GitHub Windows runner.
- The OS folder picker (PowerShell / osascript / zenity).
- Launching real Cemu.

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

### First hardware result (2026-10-12)
- The repository owner ran the `test/all-features` Windows package with an iPhone. Server start, QR pairing over
  HTTPS and Wii Remote connect (LED 1) work. See `docs/compatibility.md`.

### Unverified
- Everything else on real phones: camera mode chosen, fps, latency, iOS motion permission, gyro axis signs, battery.
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
