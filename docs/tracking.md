# Camera tracking and calibration

> **Status (2026-10-12):** pointing math, gyro fusion, tag rendering, the TV border page (`/tracking`) and HTTPS
> for phones are implemented and tested synthetically. Not built yet: on-phone detection (camera + AprilTag
> WASM), the calibration UI, and physical measurements.

## Important ergonomic constraint: camera orientation

A phone's rear camera looks out of the **back** of the phone, but a Wii Remote points along its **long axis**. If
a phone is held like a Wii Remote (top edge toward the TV, screen facing up), its camera looks at the floor and
cannot see the markers. There are two workable grips:

1. **"Photo grip" (no accessory):** the phone is held upright with its back facing the TV and the screen facing
   the player. The pointing direction is the camera axis. The buttons stay on screen under the thumbs. This needs
   no hardware and is the default the math assumes (aim image point = image centre).
2. **Periscope grip (3D-printed accessory):** a 45° mirror clip over the rear camera turns its view by 90°, so
   the phone can be held like a real Wii Remote. The mirror flips the image, which a homography handles. The
   calibration step absorbs the changed pointing direction (`calibrate_aim_point`).

This must be validated physically with real phones before the controller UI is finalised.

## Server pipeline (protocol v2)

The phone sends `camera_frame` (binary JPEG) and `gyro` messages. A worker thread per Wii Remote runs
decode → detect → `Tracker` and writes the pointer into that player's DSU slot, overriding the phone's own
`pointer` field for 2 s after each update. When frames arrive faster than they can be processed, only the newest
is kept. The phone receives `tracking` feedback at about 10 Hz. An end-to-end test drives this path with a real
JPEG over the WebSocket and checks the pointer Cemu would receive.

## Decision: detection runs on the PC

See [ADR-0001](adr/0001-apriltag-detection-on-pc.md). The native AprilTag detector found all 10 rendered tags (IDs
and corner order verified) in 1.13 ms per 640×360 frame in the dev container.

## Implemented (crates/wiiu-tracking)

| Module | What it does | Tests |
|--------|--------------|-------|
| `markers` | Lays out square `tag36h11` tags clockwise in a border just outside the game image, in game-normalised coordinates (same space as the protocol `pointer`). | ids unique, tags square in px, no overlap with the game |
| `homography` | Normalised DLT (Hartley-Zisserman 4.2) with a Jacobi eigen-solver; rejects degenerate input. | exact recovery, degeneracy rejection |
| `aim` | `estimate_aim`: image → game homography from all visible tag corners, aim = H(aim image point); rejects < 2 tags or reprojection RMS > 3 px. `calibrate_aim_point`: per-device pointing-axis calibration. | synthetic camera (below) |

| `fusion` | `PointerFusion`: between camera fixes, rotates the pointing ray by the gyro-integrated rotation and maps it through the last homography; a fix resets the drift. Tracking → GyroOnly after 0.3 s without fix → Lost after 2 s. | pure-rotation exactness, focal-length error, gyro bias with 30 fps fixes, state transitions |
| `pipeline` | `Tracker`: anchors each fix at the frame's capture time and replays the gyro samples recorded after it (latency compensation; exact in synthetic tests). | late-frame replay, out-of-order frames, bounded history |
| `detect` (feature `detector`) | JPEG → grayscale, then the vendored AprilTag C detector. | real-JPEG end-to-end, size-mismatch rejection |
| `tags` / `tag36h11` | Official tag36h11 code table and renderer (10×10 cells incl. quiet zone). | table integrity, Hamming distance ≥ 11 under rotation |

Synthetic results (`tests/synthetic.rs`, 1280×720 camera, f = 1000 px, 2–4 screen heights away, random
yaw/pitch/roll/offset):

- Noise-free: aim matches the ray-plane ground truth to < 1e-6 across ≥ 200 random poses.
- 0.5 px Gaussian corner noise: median error 0.023 % and 95th percentile 0.057 % of the screen width
  (300 poses). On a 1920 px wide TV, that is about 0.4 px median and 1.1 px p95.
- Rolling the phone around its pointing axis does not move the aim point, as on a real Wii Remote.
- Calibration of an offset pointing axis (8° / 5°) brings errors within 1 % of the screen.

- Gyro fusion: pure rotation for 0.25 s matches truth to < 1e-6. A 10 % focal-length error stays within 2 %
  after a 5° turn. A gyro bias of 0.02–0.03 rad/s with 30 fps camera fixes keeps the error below 0.5 % of the
  screen.

These are idealised numbers. Real detection adds corner bias, blur and rolling-shutter effects, so physical
measurements are still required.

## Goal

Each Wii Remote phone points its rear camera at the TV. AprilTag markers around the edge of the game image let
the phone compute where on the screen it is aiming, as normalised `(x, y)`. The phone sends that value as
`pointer` in the protocol, and the server forwards it to that player's DSU touch point. Each phone computes its
own pointer, so players never share a cursor.

## Plan

1. **Marker border.** Render `tag36h11` AprilTags (unique IDs per position) in a thin frame around the Cemu game
   image. The frame is an overlay window on the TV output. Marker size and count are configurable. Corner tags
   are the minimum; edge tags add robustness when part of the screen is out of view.
2. **Detection on the phone.** Run AprilTag detection on camera frames in a Web Worker (WASM build of the
   AprilTag C library), at a reduced resolution of about 640×360 to keep latency and battery use acceptable.
3. **Pose → pointer.** With known tag positions in screen coordinates, estimate a homography from camera image
   to screen plane from all detected tag corners (RANSAC when there are more than 4 points). The aim point is
   where the camera's optical axis, adjusted for the camera's offset from the phone's "pointing" axis, hits the
   screen plane.
4. **Fusion.** Between camera frames, and while markers are lost, integrate the gyroscope to propagate the aim
   point. Each camera fix corrects the drift (complementary filter first, EKF if needed).
5. **Calibration.** A one-time "aim at the centre and press A" step removes the camera-to-pointing-axis offset
   for each phone model and grip, and is stored per device.
6. **Fallbacks.** If markers stay lost for more than 300 ms, keep a gyro-only pointer for up to 2 s, then send
   `pointer: null`. Manual recentre: hold `B` and press `HOME`.

## Metrics to measure (per phone model)

- Aiming error in screen px at the centre and corners (target: < 1% of screen width).
- Motion-to-pointer latency (target: < 50 ms).
- Drift per minute while gyro-only.
- Time to recover after markers come back into view.

## Test strategy

- Deterministic tests with synthetic camera poses: project the tag corners with a known pose, run the
  estimator, and compare the aim point.
- Recorded image fixtures (real phone captures of the border) for regression tests of detection.
- Physical tests on real phones, recorded in `docs/compatibility.md`. Browser emulation cannot validate
  cameras or gyroscopes.
