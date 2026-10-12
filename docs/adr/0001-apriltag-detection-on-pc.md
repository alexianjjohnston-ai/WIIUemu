# ADR-0001: Run AprilTag detection on the PC, not on the phones

**Status:** Proposed (to be confirmed by latency measurements on real phones)
**Date:** 2026-10-12
**Deciders:** repository owner; lead engineer

## Context

Milestone 2 needs each Wii Remote phone to know where it is pointing on the TV. The phone's rear camera sees the
AprilTag border (`/tracking`), and the aim point follows from the detected tag corners. The pointing math
(homography, aim point, calibration) and the gyro fusion already exist in Rust (`crates/wiiu-tracking`) and are
tested with synthetic cameras.

Forces:

- **Latency target:** < 50 ms from motion to pointer. The gyro covers motion between camera fixes, so camera fixes
  can arrive late without lagging the pointer, as long as they are frequent (≥ 20–30 fps).
- **Up to 3 Wii Remote phones** at once, plus the GamePad phone streaming video (Milestone 5).
- **Phones run a browser PWA** (React/TypeScript). There is no native app.
- **Toolchain facts (verified 2026-10-12):** the `apriltag` 0.4 / `apriltag-sys` crates vendor the AprilTag C library
  and build with `cc` on Linux without system dependencies. The Windows (MSVC) build is **unverified**. There is no
  pure-Rust AprilTag detector on crates.io. Emscripten and wasi-sdk are not installed; `wasm32-unknown-unknown` is.
- **Measured:** the native detector found all 10 tags of our rendered border (IDs and corner order matched
  `wiiu_tracking::markers`) in **1.13 ms per 640×360 frame** on one thread of the dev container's CPU. That frame was
  clean and frontal; real frames will cost more.

## Decision

For the Milestone 2 prototype, **phones send low-resolution grayscale camera frames and gyro samples to the PC.
The PC runs detection (native AprilTag C via `apriltag-sys`), aim estimation and gyro fusion in Rust, and writes the
resulting pointer into that player's DSU slot.** The phone stays a thin client: camera capture, sensor
streaming, buttons, haptics.

## Options Considered

### Option A: Detect on the phone (WebAssembly in the browser)

| Dimension | Assessment |
|-----------|------------|
| Complexity | High. The AprilTag C library (malloc, pthreads, libm) needs emscripten or wasi-sdk with a libc shim, or the math has to be duplicated in TypeScript. Alternatively OpenCV.js with its AprilTag dictionary (about 8–10 MB download). |
| Network | Minimal: only the aim point and gyro (< 2 KB/s). |
| Latency | No network hop for frames, but detection runs on a mobile CPU in WASM (2–5× slower than native, unmeasured). |
| Testability | Split: detection in the browser (hard to test in CI); math either in Rust compiled to WASM, or duplicated in TS. |
| Battery | Higher: continuous detection on the phone CPU. |

**Pros:** lowest bandwidth; scales to any number of phones; works if the PC is slow.
**Cons:** a toolchain we can't build in CI today; the tracking logic is split across two languages or a WASM
bridge; phone heat and battery; slower iteration on the hardest part of the project.

### Option B: Detect on the PC (native Rust + AprilTag C) — chosen

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low to medium. One Rust pipeline per phone: decode → detect → `estimate_aim` → `PointerFusion` → DSU. The crates build today. |
| Network | ~15–30 KB per 640×360 grayscale JPEG at ~30 fps is about 0.5–1 MB/s per phone; 3 phones ≈ 3 MB/s on the LAN. |
| Latency | Encode (phone) + Wi-Fi + decode + detect (~1–5 ms) ≈ an estimated 15–35 ms. Gyro samples (small, ~100–200 Hz) cover the gap. |
| Testability | High. Recorded frames become CI fixtures; the whole pipeline runs in `cargo test`. |
| Battery | Moderate: the camera plus hardware JPEG encoding on the phone; no detection there. |

**Pros:** one language and one tested pipeline; native speed; recorded-fixture regression tests; the phone
code stays simple; tuning happens in one place.
**Cons:** depends on Wi-Fi quality; a C dependency on Windows/macOS that CI must prove; more bandwidth; the PC
does the work for all 3 phones (cheap at ~1–5 ms per frame).

### Option C: Hybrid (phone detects, PC refines)

Deferred. Worth revisiting only if Option B's measured latency or bandwidth fails on real networks.

## Trade-off Analysis

The decisive factors are **verifiability and iteration speed** on the riskiest component. Option B keeps
detection, pose math and fusion in one Rust pipeline that we can test with recorded frames in CI, and the
native detector is already measured at ~1 ms per frame. Option A's advantages (bandwidth, no network hop) matter
less on a home LAN, and the gyro hides fix latency. Its costs (unbuildable toolchain today, split logic) land
exactly where we most need fast, testable iteration. If on-phone detection becomes necessary later, the same
Rust crates can be compiled to WASM once a toolchain is set up; Option B doesn't block that path.

## Consequences

- **Easier:** CI regression tests with real image fixtures; one implementation of the tracking pipeline; simple
  phone code (`getUserMedia` → canvas → grayscale JPEG → WebSocket binary frame).
- **Harder:** the phone protocol needs binary frames for camera images. Protocol v1 rejects binary frames, so this
  needs a protocol revision documented in `docs/controller-protocol.md` first. A per-phone frame budget is needed so
  one phone can't starve the others.
- **Revisit:** if measured motion-to-pointer latency exceeds 50 ms on typical home Wi-Fi, or bandwidth with 3 phones
  plus GamePad video causes drops, reconsider Option A or C.

## Action Items

1. [x] Prove the detector builds on Windows (MSVC). **Finding:** `apriltag-sys` 0.3 bundles an old AprilTag snapshot
   that includes `pthread.h` and fails on MSVC (CI, 2026-10-12). Its pre-generated bindings don't match newer
   upstream structs, so pointing it at a newer source isn't safe. **Resolution:** `crates/wiiu-apriltag-sys` vendors
   pinned upstream AprilTag, which supports MSVC via `pthreads_cross`, and exposes it through a primitive-only C shim, so Rust never
   depends on C struct layouts. The pure-Rust `kornia-apriltag` 0.1.10 was also evaluated: it found 7 of 10 tags on the
   synthetic border, and its corner order differed from its own docs, so it was rejected for now.
2. [x] Protocol v2 draft: binary `camera_frame` messages (header: seq, capture timestamp, width, height, JPEG
   payload ≤ 64 KB) and `gyro` batches. Size limits and rate caps per slot.
3. [x] Server pipeline per Wii Remote slot: decode → detect → `estimate_aim` → `PointerFusion` → DSU touch point,
   with timing metrics exposed on `/api/status` (metrics still to do).
4. [x] Phone: camera capture worker (rear camera, 640×360, grayscale JPEG via `OffscreenCanvas.convertToBlob`),
   gyro streaming from `DeviceMotionEvent` (requires the HTTPS listener). Done in `apps/phone/src/tracking/`.
   The encode runs on the main thread via `OffscreenCanvas.convertToBlob` (a worker isn't needed so far). Frames
   are colour JPEG at 1280×720 capture, downscaled to ≤ 640 px.
5. [ ] Measure on real phones: fix rate, end-to-end latency, aiming error and battery. Record the results in
   `docs/tracking.md` and confirm or supersede this ADR.
