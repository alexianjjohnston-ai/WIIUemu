# Camera tracking and calibration

> **Status: design only. Nothing in this document is implemented yet (Milestone 2).**

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
