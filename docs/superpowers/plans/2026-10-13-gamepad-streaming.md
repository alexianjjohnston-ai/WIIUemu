# GamePad Screen Streaming Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Phone 1 (GamePad role) shows Cemu's GamePad picture in its touch area, live, and touches keep landing at
the same spot in the game.

**Architecture:** On Windows, a capture thread grabs Cemu's "GamePad View" window with Windows.Graphics.Capture
(the `windows-capture` crate). It crops the 16:9 picture, scales it to at most 854×480 and encodes it as JPEG.
The newest frame is sent as a binary `video_frame` on the GamePad phone's existing WebSocket. The phone decodes it
with `createImageBitmap` and draws it on a canvas under its touch layer. Flow control is by acknowledgement: at
most two frames are in flight, so a slow link lowers the frame rate instead of adding delay. A test-pattern source
runs the same pipeline on every platform, so everything except the Windows capture is tested on Linux CI and in
Playwright.

**Tech Stack:**
- Rust: tokio, axum WebSocket, `jpeg-encoder` 0.6 (already a dev-dependency) and `windows-capture` 2.0
  (MIT, Windows only).
- Phone: React + TypeScript, `createImageBitmap`, Vitest, Playwright.

**Spec:** `docs/adr/0002-gamepad-screen-streaming.md` (proposed), plus the owner's go-ahead on 2026-10-13
("yes work on all 3"). This plan changes the ADR in three places, recorded in Task 7:
1. **JPEG first.** H.264 and WebCodecs become a later step if bandwidth proves a problem. JPEG decodes on every
   phone, including iPhones without WebCodecs H.264, needs no hardware-encoder code that can't be tested in CI,
   and adds no decoder buffering.
2. **Acknowledgement flow control instead of keyframe requests**, because every JPEG frame is a keyframe.
3. **No codec negotiation message yet.** The frame header carries a format byte, and a later version adds an
   optional `codecs` field.

## Global Constraints

- Protocol changes go to `docs/controller-protocol.md` first, then `wiiu-controller` (Rust) and `apps/phone` (TS)
  in the same PR (CLAUDE.md).
- Validate every phone field; reject, don't clamp (CLAUDE.md).
- Video goes only to the socket that holds the GamePad slot, on protocol ≥ 3. The capture is limited to Cemu's own
  GamePad window, never the desktop (ADR-0002, Security).
- `unsafe_code = "forbid"` in the workspace. Raw Win32 calls live only in `crates/wiiu-desktop-sys`.
- Output size at most 854×480, at most 30 frames per second, JPEG quality 70.
- Disconnect or 1 s of silence still releases all input (the safety invariants are unchanged; video must never
  delay input handling).
- Emulator behaviour claims need a Cemu source reference (`docs/research.md`) or a real test. Capture with real
  Cemu stays **unverified** until the owner tests it.
- Branch `feat/gamepad-stream`, stacked on `feat/tv-border` (it reuses `tv_border::cemu`). Conventional Commits.
  Commit footer:
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_01SL88jiHNEW8Xkspj2km5vZ`.
- Checks before every commit:
  `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, and
  in `apps/phone`: `npm run typecheck && npm run lint && npm test`. Never run prettier. After Playwright,
  `git checkout -- apps/phone/screenshots`.

## Review Focus

1. **Phone on a slow or stalled link:** with no acks, the server must stop at two frames in flight and must still
   answer pings and apply input. Task 4 tests that input reaches DSU while video is blocked on acks.
2. **GamePad window closed, minimised or missing (or Cemu not running):** the phone must say what to do, not show
   a frozen picture forever. Task 3 tests the status path; Task 5 tests the hint text.
3. **Page hidden or phone locked:** streaming must stop (battery, data) and resume when visible. Task 5 tests that
   `video off` goes out on hide and `video on` on show.
4. **Reconnect or replace:** a new socket for the GamePad slot must start a fresh stream, and the old capture
   thread must end. Task 4 asserts the stream handle is dropped on disconnect, using a counter of live streams.
5. **Hostile phone:** acks for seqs never sent, video from a Wii Remote or a protocol-2 socket, and floods of
   `video on`/`off`. These are rejected or rate-limited, never crashes. Task 1 covers parsing, and Task 4 covers
   role, protocol and seq checks.

---

### Task 1: Protocol 3 in the spec and in Rust

**Files:**
- Modify: `docs/controller-protocol.md`: new section "Protocol version 3: GamePad screen", after the version 2
  sections.
- Modify: `crates/wiiu-controller/src/protocol.rs`.
- Test: in-file tests in `crates/wiiu-controller/src/protocol.rs`.

**Interfaces:**
- Produces:
  - `PROTOCOL_VERSION: u32 = 3`;
  - `ClientMessage::Video { on: bool }` and `ClientMessage::VideoAck { seq: u32 }`;
  - `ServerMessage::VideoStatus { state: VideoState }`;
  - `pub enum VideoState { Streaming, NoWindow, Minimized, Unsupported, Error }` (serde snake_case);
  - `pub struct VideoFrameHeader { pub format: u8, pub width: u16, pub height: u16, pub seq: u32 }`;
  - `pub const VIDEO_FRAME_HEADER_BYTES: usize = 12`, `pub const MAX_VIDEO_PAYLOAD_BYTES: usize = 262_144`,
    `pub const VIDEO_FORMAT_JPEG: u8 = 1`;
  - `pub fn encode_video_frame(h: &VideoFrameHeader, payload: &[u8]) -> Vec<u8>`;
  - `pub fn parse_video_frame(bytes: &[u8]) -> Result<(VideoFrameHeader, &[u8]), ProtocolError>` (used by tests
    and docs examples).

- [ ] **Step 1: Write the spec section** in `docs/controller-protocol.md`:

```markdown
## Protocol version 3: GamePad screen (GamePad role)

Version 3 is version 2 plus the GamePad picture. A phone sends `"protocol": 3`. The server accepts 1, 2 and 3 and
answers with the version it accepted. A phone that gets `unsupported_protocol` says hello again with one version
lower. Only the `gamepad` role on a version 3 connection may send these messages; anything else closes the
connection with `invalid_message`.

### `video` (text, client → server)

`{"type":"video","on":true}` starts the picture; `{"type":"video","on":false}` stops it. The phone sends `on` after
`welcome` while its GamePad screen is visible, and `off` when the page is hidden. At most 10 per second.

### `video_ack` (text, client → server)

`{"type":"video_ack","seq":17}` after the phone has drawn frame 17. `seq` must be a frame the server sent on this
connection and greater than the last ack; otherwise the connection closes. The server keeps at most 2 frames
unacknowledged and always sends the newest frame next, so a slow link lowers the frame rate instead of adding delay.

### `video_status` (text, server → client)

`{"type":"video_status","state":"streaming"}`. States:
- `streaming`: frames follow;
- `no_window`: Cemu's GamePad window isn't open (*View → Separate GamePad view*), or Cemu isn't running;
- `minimized`: the window is minimised, so Windows delivers no frames;
- `unsupported`: no capture on this PC (macOS and Linux for now);
- `error`: capture failed.

Sent when streaming starts and whenever the state changes. Phones treat unknown states like `error`.

### `video_frame` (binary, server → client)

| offset | type | field | rules |
|--------|------|-------|-------|
| 0 | u8 | kind | `2` = video frame |
| 1 | u8 | format | `1` = JPEG (baseline) |
| 2 | u16 | width | 160–1280 |
| 4 | u16 | height | 90–720 |
| 6 | u16 | reserved | `0` |
| 8 | u32 | seq | strictly increasing per connection, from 1 |
| 12 | … | payload | 1–262 144 bytes |

Little-endian. The picture is Cemu's 16:9 GamePad image (the same area DSU touch coordinates map to), so the phone
draws it over its whole touch area and touches stay 1:1. Phones drop frames they can't parse.
```

- [ ] **Step 2: Write failing tests** in `protocol.rs` `mod tests`:

```rust
#[test]
fn video_messages_parse_and_serialize() {
    assert_eq!(parse_client_message(r#"{"type":"video","on":true}"#).unwrap(), ClientMessage::Video { on: true });
    assert_eq!(parse_client_message(r#"{"type":"video_ack","seq":7}"#).unwrap(), ClientMessage::VideoAck { seq: 7 });
    for bad in [
        r#"{"type":"video"}"#,
        r#"{"type":"video","on":1}"#,
        r#"{"type":"video","on":true,"extra":1}"#,
        r#"{"type":"video_ack","seq":-1}"#,
        r#"{"type":"video_ack","seq":4294967296}"#,
        r#"{"type":"video_ack"}"#,
    ] {
        assert!(parse_client_message(bad).is_err(), "{bad}");
    }
    assert_eq!(
        ServerMessage::VideoStatus { state: VideoState::NoWindow }.to_json(),
        r#"{"type":"video_status","state":"no_window"}"#
    );
}

#[test]
fn video_frame_round_trip_and_rejection() {
    let h = VideoFrameHeader { format: VIDEO_FORMAT_JPEG, width: 854, height: 480, seq: 3 };
    let bytes = encode_video_frame(&h, &[0xFF, 0xD8, 0xFF]);
    assert_eq!(&bytes[..12], &[2, 1, 0x56, 0x03, 0xE0, 0x01, 0, 0, 3, 0, 0, 0]);
    let (back, payload) = parse_video_frame(&bytes).unwrap();
    assert_eq!(back, h);
    assert_eq!(payload, &[0xFF, 0xD8, 0xFF]);
    let mut wrong_kind = bytes.clone();
    wrong_kind[0] = 1;
    assert!(parse_video_frame(&wrong_kind).is_err());
    assert!(parse_video_frame(&bytes[..12]).is_err(), "empty payload");
    let tiny = encode_video_frame(&VideoFrameHeader { width: 100, ..h }, &[1]);
    assert!(parse_video_frame(&tiny).is_err(), "width below 160");
}

#[test]
fn protocol_three_is_accepted() {
    let hello = r#"{"type":"hello","protocol":3,"token":"abcdefgh","role":"gamepad","deviceId":"device-0001"}"#;
    assert!(matches!(parse_client_message(hello), Ok(ClientMessage::Hello(_))));
}
```

(Match the existing hello test's field rules: copy a valid hello from `parses_hello` and change only
`protocol`.)

- [ ] **Step 3: Run them to verify they fail.**
  Run: `cargo test -p wiiu-controller video_ protocol_three`. Expected: compile errors (`Video`, `VideoState`,
  `encode_video_frame` not found).
- [ ] **Step 4: Implement.**
  - Set `PROTOCOL_VERSION` to 3.
  - Add the variants with `#[serde(deny_unknown_fields)]`, the same way as the other client messages.
    `seq: u32`, so serde rejects negative and too-large values.
  - Add `VideoState` and `ServerMessage::VideoStatus`.
  - Add the header type and functions. In `parse_video_frame`:
    - check the length ≥ 13;
    - kind = 2, format = 1;
    - width in 160..=1280, height in 90..=720;
    - reserved = 0;
    - payload length ≤ `MAX_VIDEO_PAYLOAD_BYTES`.
    - Return `ProtocolError::InvalidMessage` with a short reason, like `parse_camera_frame`.
- [ ] **Step 5: Run `cargo test -p wiiu-controller`.** Expected: PASS. Then the full check command.
- [ ] **Step 6: Commit** `feat(protocol): protocol 3 video messages and frame format`.

### Task 2: Phone protocol types, validation and connection

**Files:**
- Modify: `apps/phone/src/protocol/types.ts`, `apps/phone/src/protocol/validate.ts`,
  `apps/phone/src/net/connection.ts`.
- Test: `apps/phone/src/protocol/validate.test.ts`, `apps/phone/src/net/connection.test.ts`.

**Interfaces:**
- Consumes: the spec from Task 1.
- Produces:
  - `PROTOCOL_VERSION = 3`;
  - types `VideoMessage = {type:'video'; on:boolean}`, `VideoAckMessage = {type:'video_ack'; seq:number}` and
    `VideoStatusMessage = {type:'video_status'; state: VideoState}`, with
    `VideoState = 'streaming'|'no_window'|'minimized'|'unsupported'|'error'`;
  - `interface VideoFrame { width:number; height:number; seq:number; jpeg: Uint8Array }`;
  - `parseVideoFrame(buf: ArrayBuffer): VideoFrame | null`;
  - `encodeVideoFrame(f: VideoFrame): Uint8Array` (for the mock server);
  - `Connection` events `onVideoFrame?(f: VideoFrame)` and `onVideoStatus?(s: VideoState)`;
  - `Connection.supportsVideo` (true when the welcome's protocol ≥ 3).

- [ ] **Step 1: Failing tests.** In `validate.test.ts`:

```ts
it('parses video frames and rejects malformed ones', () => {
  const f = { width: 854, height: 480, seq: 3, jpeg: new Uint8Array([0xff, 0xd8, 0xff]) };
  const bytes = encodeVideoFrame(f);
  expect(Array.from(bytes.slice(0, 12))).toEqual([2, 1, 0x56, 0x03, 0xe0, 0x01, 0, 0, 3, 0, 0, 0]);
  expect(parseVideoFrame(bytes.buffer)).toEqual(f);
  const wrongKind = bytes.slice();
  wrongKind[0] = 1;
  expect(parseVideoFrame(wrongKind.buffer)).toBeNull();
  expect(parseVideoFrame(bytes.slice(0, 12).buffer)).toBeNull();
  expect(parseVideoFrame(encodeVideoFrame({ ...f, width: 100 }).buffer)).toBeNull();
});

it('reads video status, keeping unknown states as errors', () => {
  expect(parseServerMessage('{"type":"video_status","state":"no_window"}')).toEqual({
    type: 'video_status',
    state: 'no_window',
  });
  expect(parseServerMessage('{"type":"video_status","state":"weird"}')).toEqual({ type: 'video_status', state: 'error' });
  expect(parseServerMessage('{"type":"video_status"}')).toBeNull();
});

it('validates video and video_ack from the gamepad only', () => {
  expect(validateClientFrame('{"type":"video","on":true}', 'gamepad')).toEqual([]);
  expect(validateClientFrame('{"type":"video_ack","seq":4}', 'gamepad')).toEqual([]);
  expect(validateClientFrame('{"type":"video","on":true}', 'wiimote')).not.toEqual([]);
  expect(validateClientFrame('{"type":"video_ack","seq":-1}', 'gamepad')).not.toEqual([]);
});
```

  In `connection.test.ts`, add tests next to the protocol 2 one. They use its `setup()`, `WELCOME` and
  `serverSend` helpers. If the fake socket lacks a binary send, add `serverSendBinary(buf: ArrayBuffer)` to it.

```ts
it('protocol 3: forwards video frames and status, and steps down one version at a time', () => {
  const { conn, last, events, sockets } = setup();
  conn.start();
  last().serverOpen();
  expect(last().messages()[0]).toMatchObject({ type: 'hello', protocol: 3 });
  last().serverSend({ type: 'error', code: 'unsupported_protocol', message: 'protocol must be 1 or 2' });
  last().serverClose();
  last().serverOpen();
  expect(last().messages()[0]).toMatchObject({ type: 'hello', protocol: 2 });
  expect(sockets).toHaveLength(2);
});

it('passes video frames and status to the session', () => {
  const { conn, last, events } = setup();
  conn.start();
  last().serverOpen();
  last().serverSend({ ...WELCOME, protocol: 3 });
  expect(conn.supportsVideo).toBe(true);
  const f = { width: 854, height: 480, seq: 1, jpeg: new Uint8Array([0xff, 0xd8]) };
  last().serverSendBinary(encodeVideoFrame(f).buffer);
  expect(events.onVideoFrame).toHaveBeenCalledWith(f);
  last().serverSend({ type: 'video_status', state: 'minimized' });
  expect(events.onVideoStatus).toHaveBeenCalledWith('minimized');
});
```

  `setup()` must add `onVideoFrame: vi.fn()` and `onVideoStatus: vi.fn()` to `events`.
- [ ] **Step 2: Run `npx vitest run src/protocol src/net`.** Expected: FAIL (missing exports).
- [ ] **Step 3: Implement.**
  - `types.ts`: the constants (`VIDEO_FRAME_HEADER_BYTES = 12`, `MAX_VIDEO_PAYLOAD_BYTES = 262144`,
    `VIDEO_WIDTH_RANGE = [160, 1280]`, `VIDEO_HEIGHT_RANGE = [90, 720]`) and the types. Add the new client messages
    to the client union and `VideoStatusMessage` to `ServerMessage`.
  - `validate.ts`:
    - `parseVideoFrame` uses `DataView` little-endian with the same checks as Rust;
    - `encodeVideoFrame`;
    - a `video_status` case mapping unknown or non-string states to `'error'` (a missing `state` returns null);
    - `video`/`video_ack` in `validateClientFrame`, with `on` boolean only and `seq` an integer in
      1..2^32−1, role `gamepad` only.
  - `connection.ts`:
    - `onMessage`: if `data instanceof ArrayBuffer`, refresh `lastHeardAt`; while ready, call
      `events.onVideoFrame?.(parsed)` for frames that parse;
    - for `video_status`, call `events.onVideoStatus?.(msg.state)`;
    - on `unsupported_protocol` before ready, set `helloProtocol -= 1` while it is above `MIN_PROTOCOL_VERSION`
      (it used to jump to the minimum);
    - add a `supportsVideo` getter like `supportsCamera`, with `>= 3`.
- [ ] **Step 4: Run the tests.** Expected: PASS. Then run `npm run typecheck && npm run lint && npm test`. Fix
  existing tests that expected the jump to protocol 1: an old protocol-1 PC now needs two retries. Keep the
  old test, and expect hellos 3, 2, 1.
- [ ] **Step 5: Update the e2e mock server** (`apps/phone/e2e/fixtures.ts`):
  - add `video: { on: boolean; conn: number }[]` and `acks: number[]`;
  - record `video` and `video_ack` from parsed text frames, and push a violation for them on protocol < 3 or role
    `wiimote`;
  - add `sendVideoFrame(f: VideoFrame)` (sends `encodeVideoFrame(f)` as binary) and `sendVideoStatus(state)`;
  - default `protocol` stays 2. Tests opt in with `server.protocol = 3`.
- [ ] **Step 6: Commit** `feat(phone): protocol 3 types, video frame parsing and version step-down`.

### Task 3: Video pipeline with a test-pattern source

**Files:**
- Create: `crates/wiiu-connect/src/video/mod.rs` (public API, `VideoSourceKind`, `spawn_stream`),
  `crates/wiiu-connect/src/video/image.rs` (crop, scale, encode),
  `crates/wiiu-connect/src/video/source.rs` (`VideoSource` trait, `TestPattern`, `Unsupported`),
  `crates/wiiu-connect/src/video/stream.rs` (the capture thread loop).
- Modify: `crates/wiiu-connect/src/lib.rs` (`mod video;`), and `crates/wiiu-connect/Cargo.toml` (move
  `jpeg-encoder = "0.6"` from dev-dependencies to dependencies).
- Test: in-file tests in `image.rs`, `source.rs` and `stream.rs`.

**Interfaces:**
- Consumes: `wiiu_controller::protocol::VideoState`.
- Produces:
  - `pub struct Bgra { pub width: u32, pub height: u32, pub stride: u32, pub data: Vec<u8> }` (top-down, 4 bytes
    per pixel, `stride` in bytes);
  - `pub fn game_area(width: u32, height: u32) -> [u32; 4]`: the centred 16:9 rectangle (left, top, right,
    bottom) that Cemu letterboxes the GamePad image into (`KeepAspectRatio`, research.md);
  - `pub fn output_size(w: u32, h: u32) -> (u32, u32)`: fits in 854×480, keeps the aspect, never upscales, even
    dimensions, at least 160×90;
  - `pub fn scale(src: &Bgra, area: [u32; 4], out_w: u32, out_h: u32) -> Bgra`: box filter;
  - `pub fn encode_jpeg(img: &Bgra, quality: u8) -> Vec<u8>`;
  - `pub trait VideoSource: Send { fn next(&mut self, timeout: Duration) -> SourceEvent; }`, with
    `pub enum SourceEvent { Frame(Bgra), Status(VideoState), Idle }`;
  - `pub struct TestPattern` (854×480, 30 fps):
    - quadrant colours top-left red `(255,0,0)`, top-right green `(0,255,0)`, bottom-left blue `(0,0,255)` and
      bottom-right white;
    - a 16 px black vertical bar moving 8 px per frame.
  - `pub struct Unsupported` emits `Status(Unsupported)` once, then `Idle`.
  - `pub enum VideoSourceKind { Cemu, TestPattern }`;
  - `pub struct EncodedFrame { pub width: u16, pub height: u16, pub jpeg: Vec<u8> }`;
  - `pub enum VideoUpdate { Frame(Arc<EncodedFrame>), Status(VideoState) }`;
  - `pub struct StreamHandle` (Drop stops and joins the thread);
  - `pub fn spawn_stream(kind: VideoSourceKind) -> (StreamHandle, tokio::sync::watch::Receiver<Option<VideoUpdate>>)`;
  - `pub fn live_streams() -> usize`: a global `AtomicUsize`, raised on spawn and lowered on thread exit, for the
    tests.

- [ ] **Step 1: Failing tests for `image.rs`.**

```rust
#[test]
fn game_area_is_centred_16_9() {
    assert_eq!(game_area(854, 480), [0, 0, 854, 480]);
    assert_eq!(game_area(1000, 480), [73, 0, 927, 480]); // pillarbox
    assert_eq!(game_area(854, 600), [0, 60, 854, 540]); // letterbox
}

#[test]
fn output_size_fits_and_never_upscales() {
    assert_eq!(output_size(1708, 960), (854, 480));
    assert_eq!(output_size(640, 360), (640, 360));
    assert_eq!(output_size(855, 481), (854, 480));
    assert_eq!(output_size(100, 56), (160, 90));
}

#[test]
fn scale_keeps_quadrants_and_jpeg_round_trips() {
    let src = TestPattern::frame_at(0); // 854×480, bar at x 0..16
    let out = scale(&src, game_area(854, 480), 428, 240);
    let px = |x: u32, y: u32| {
        let i = (y * out.stride + x * 4) as usize;
        [out.data[i + 2], out.data[i + 1], out.data[i]] // BGRA → RGB
    };
    assert_eq!(px(200, 60), [255, 0, 0]);
    assert_eq!(px(300, 60), [0, 255, 0]);
    assert_eq!(px(200, 180), [0, 0, 255]);
    assert_eq!(px(300, 180), [255, 255, 255]);
    let jpeg = encode_jpeg(&out, 70);
    assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);
    let gray = wiiu_tracking::detect::decode_jpeg_gray(&jpeg).unwrap();
    assert_eq!((gray.width, gray.height), (428, 240));
}
```

  `decode_jpeg_gray` needs the `detector` feature of `wiiu-tracking`, which `wiiu-connect` already enables. Check
  its return type and adjust the field names.
- [ ] **Step 2: Run `cargo test -p wiiu-connect video::`.** Expected: FAIL (the module doesn't exist).
- [ ] **Step 3: Implement `image.rs` and `TestPattern::frame_at(n: u32) -> Bgra`.**
  - `game_area`: if `w * 9 > h * 16`, then `gw = h * 16 / 9`, `left = (w - gw) / 2`; otherwise
    `gh = w * 9 / 16`, `top = (h - gh) / 2`.
  - `scale`: for each output pixel, average the source pixels in its box, with integer box edges
    `x0 = left + ox * aw / out_w`, `x1 = max(x0 + 1, left + (ox + 1) * aw / out_w)`.
  - `encode_jpeg`: `jpeg_encoder::Encoder::new(&mut buf, quality).encode(&img.data_without_stride(),
    w as u16, h as u16, ColorType::Bgra)`. Copy rows into a tight buffer when `stride != width * 4`.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Failing tests for `stream.rs`.**

```rust
#[test]
fn test_pattern_stream_publishes_frames_and_stops_on_drop() {
    let before = live_streams();
    let (handle, mut rx) = spawn_stream(VideoSourceKind::TestPattern);
    let rt = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap();
    let mut frames = 0;
    let mut saw_streaming = false;
    rt.block_on(async {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        while frames < 5 && tokio::time::Instant::now() < deadline {
            if tokio::time::timeout(Duration::from_millis(500), rx.changed()).await.is_err() { continue; }
            match rx.borrow_and_update().clone() {
                Some(VideoUpdate::Frame(f)) => { assert_eq!((f.width, f.height), (854, 480)); frames += 1; }
                Some(VideoUpdate::Status(VideoState::Streaming)) => saw_streaming = true,
                _ => {}
            }
        }
    });
    assert!(saw_streaming || frames > 0);
    assert!(frames >= 5, "got {frames} frames");
    assert_eq!(live_streams(), before + 1);
    drop(handle);
    assert_eq!(live_streams(), before);
}

#[test]
fn unsupported_source_reports_once() {
    let (_h, mut rx) = spawn_stream_with(Box::new(Unsupported::default()));
    let rt = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap();
    rt.block_on(async { tokio::time::timeout(Duration::from_secs(2), rx.changed()).await.unwrap().unwrap() });
    assert!(matches!(*rx.borrow(), Some(VideoUpdate::Status(VideoState::Unsupported))));
}
```

  The watch channel keeps only the newest value, so the test may miss the `Streaming` status when the first
  frame overwrites it. That's why the assertion allows either. The ws layer (Task 4) sends status separately (see
  there).
- [ ] **Step 6: Run.** Expected: FAIL.
- [ ] **Step 7: Implement `stream.rs`.**
  - `spawn_stream(kind)` picks the source:
    - `TestPattern`: `TestPattern::default()`;
    - `Cemu` on Windows: `crate::video::windows::CemuSource::new()` (Task 6);
    - `Cemu` elsewhere: `Unsupported`.
    It then calls `spawn_stream_with(source)`.
  - The thread loop:
    - `stop: Arc<AtomicBool>`; loop until set;
    - `source.next(Duration::from_millis(100))`;
    - on `Frame`: skip if less than 33 ms passed since the last published frame. Otherwise run `game_area`,
      `output_size`, `scale` and `encode_jpeg` at quality 70, then `send_replace(Some(Frame(Arc::new(..))))`;
    - on `Status(s)`: if it differs from the last status, `send_replace(Some(Status(s)))`;
    - before the first frame, publish `Status(Streaming)`.
  - `StreamHandle::drop` sets `stop` and joins. A guard struct in the thread decrements `LIVE` on exit, panics
    included.
  - `TestPattern::next` sleeps until the next 33 ms tick (at most `timeout`) and returns
    `Frame(frame_at(n))`.
- [ ] **Step 8: Run all `wiiu-connect` tests, then the full check.** Expected: PASS.
- [ ] **Step 9: Commit** `feat(video): frame pipeline with crop, scale, JPEG and a test pattern`.

### Task 4: Stream to the GamePad socket

**Files:**
- Modify:
  - `crates/wiiu-connect/src/lib.rs` (`ServerConfig.video: VideoSourceKind`, passed into `AppState`);
  - `crates/wiiu-connect/src/http.rs` (`AppState.video_source`, passed to `ws::handle`);
  - `crates/wiiu-connect/src/ws.rs`;
  - `crates/wiiu-connect/src/main.rs` (default `VideoSourceKind::Cemu`; hidden flag `--video-test-pattern`);
  - every `ServerConfig { .. }` literal in `crates/wiiu-connect/tests/*.rs` (add `video: VideoSourceKind::TestPattern`).
- Test: `crates/wiiu-connect/tests/end_to_end.rs`.

**Interfaces:**
- Consumes: Task 1 messages and frame encoding; Task 3 `spawn_stream`, `VideoUpdate`, `live_streams` (make it
  `pub` through `wiiu_connect::video::live_streams` for tests).
- Produces: the behaviour in the spec section.

- [ ] **Step 1: Failing end-to-end tests.** Use the existing helpers (`server`, `connect`, `hello`, `recv`). Write a
  `hello_v3_gamepad` helper that sends a protocol-3 gamepad hello and waits for `welcome`, plus a `recv_any`
  helper that returns `Message::Binary` or parsed text.

```rust
#[tokio::test]
async fn gamepad_receives_the_picture_with_at_most_two_frames_in_flight() {
    let (r, _stop) = server().await;
    let mut ws = connect(&r).await;
    hello_v3_gamepad(&mut ws, "device-7000").await;
    send_text(&mut ws, r#"{"type":"video","on":true}"#).await;
    // Without acks: exactly two frames arrive within a second.
    let frames = collect_frames(&mut ws, Duration::from_secs(1)).await;
    assert_eq!(frames.len(), 2, "{frames:?}");
    let (h, jpeg) = &frames[0];
    assert_eq!((h.width, h.height, h.seq), (854, 480, 1));
    let gray = wiiu_tracking::detect::decode_jpeg_gray(jpeg).unwrap();
    // Test pattern: top-left red, bottom-right white; luma tells them apart.
    assert!(gray.pixel(200, 100) < 120 && gray.pixel(650, 380) > 230);
    // Input still flows while video waits for acks.
    send_text(&mut ws, r#"{"type":"input","seq":1,"buttons":["A"],"pointer":null}"#).await;
    // (assert on the DSU client like `wiimote_buttons_and_pointer_reach_cemu_client`, slot 0)
    // Acks release more frames, newest first: seqs keep increasing.
    send_text(&mut ws, &format!(r#"{{"type":"video_ack","seq":{}}}"#, frames[1].0.seq)).await;
    let more = collect_frames(&mut ws, Duration::from_millis(500)).await;
    assert!(!more.is_empty() && more[0].0.seq > frames[1].0.seq);
}

#[tokio::test]
async fn video_stops_on_off_and_on_disconnect() {
    let (r, _stop) = server().await;
    let before = wiiu_connect::video::live_streams();
    let mut ws = connect(&r).await;
    hello_v3_gamepad(&mut ws, "device-7001").await;
    send_text(&mut ws, r#"{"type":"video","on":true}"#).await;
    wait_until(|| wiiu_connect::video::live_streams() == before + 1).await;
    send_text(&mut ws, r#"{"type":"video","on":false}"#).await;
    wait_until(|| wiiu_connect::video::live_streams() == before).await;
    send_text(&mut ws, r#"{"type":"video","on":true}"#).await;
    wait_until(|| wiiu_connect::video::live_streams() == before + 1).await;
    drop(ws);
    wait_until(|| wiiu_connect::video::live_streams() == before).await;
}

#[tokio::test]
async fn video_is_refused_for_wiimotes_old_protocols_and_bad_acks() {
    let (r, _stop) = server().await;
    // Wii Remote on protocol 3.
    let mut ws = connect(&r).await;
    hello_v3(&mut ws, "device-7002", "wiimote").await;
    send_text(&mut ws, r#"{"type":"video","on":true}"#).await;
    assert_closed_with(&mut ws, "invalid_message").await;
    // GamePad on protocol 2.
    let mut ws = connect(&r).await;
    hello_gamepad(&mut ws, "device-7003", 2).await;
    send_text(&mut ws, r#"{"type":"video","on":true}"#).await;
    assert_closed_with(&mut ws, "invalid_message").await;
    // Ack for a frame never sent.
    let mut ws = connect(&r).await;
    hello_v3_gamepad(&mut ws, "device-7004").await;
    send_text(&mut ws, r#"{"type":"video_ack","seq":5}"#).await;
    assert_closed_with(&mut ws, "invalid_message").await;
}
```

  `wait_until` polls every 20 ms for up to 3 s and panics on timeout. `live_streams` is process-global and tests
  run in parallel, so these two tests must hold a `static VIDEO_TESTS: tokio::sync::Mutex<()>` guard.
- [ ] **Step 2: Run `cargo test -p wiiu-connect --test end_to_end video`.** Expected: FAIL to compile or fail.
- [ ] **Step 3: Implement in `ws.rs` `run_session`.**
  - Set `let video_allowed = ctx.protocol >= 3 && ctx.role == Role::Gamepad;`.
  - State:
    - `let mut stream: Option<(StreamHandle, watch::Receiver<Option<VideoUpdate>>)> = None;`
    - `let mut sent_seq: u32 = 0; let mut acked_seq: u32 = 0;`
    - `let mut pending: Option<Arc<EncodedFrame>> = None;`
    - `let mut video_toggles = RateWindow::new();`
  - New `select!` branch, placed before `next_message`:

```rust
Some(update) = next_video(&mut stream) => {
    match update {
        VideoUpdate::Status(s) => { if !send(socket, &ServerMessage::VideoStatus { state: s }).await { return; } }
        VideoUpdate::Frame(f) => pending = Some(f),
    }
    if sent_seq - acked_seq < MAX_VIDEO_IN_FLIGHT {
        if let Some(f) = pending.take() {
            sent_seq += 1;
            if !send_video(socket, &f, sent_seq).await { return; }
        }
    }
    continue;
}
```

  - `next_video` is an `async fn` returning `Option<VideoUpdate>`:
    - with no stream it waits forever (`std::future::pending().await`);
    - otherwise `rx.changed().await.ok()?` then `rx.borrow_and_update().clone()`.
  - `send_video` builds `encode_video_frame(&VideoFrameHeader { format: VIDEO_FORMAT_JPEG, width, height, seq },
    &f.jpeg)` and sends `Message::Binary` with the same `SEND_TIMEOUT` as `send`.
  - `MAX_VIDEO_IN_FLIGHT = 2`.
  - **Status vs frames:** the watch keeps only the newest value, so a `Streaming` status can be overwritten by the
    first frame. When the first frame arrives and no status was sent yet, send `VideoStatus{Streaming}` before
    it. Also send `Streaming` whenever a frame follows a non-streaming status.
  - Messages:
    - `ClientMessage::Video { on }`: if `!video_allowed`, fail with `invalid_message` ("video requires protocol 3
      and the gamepad role"). If `!video_toggles.allow(now, 10)`, fail with "video toggled too often".
      - `on` and no stream: `stream = Some(spawn_stream(ctx.video_source))`, and reset `pending` and
        `acked_seq = sent_seq`, so old in-flight frames don't count.
      - `!on`: set `stream = None` (the drop stops the thread) and `pending = None`.
    - `ClientMessage::VideoAck { seq }`: if not allowed, or `seq <= acked_seq || seq > sent_seq`, fail with
      `invalid_message` ("video_ack for a frame not in flight"). Otherwise set `acked_seq = seq`, then send
      `pending` if any (same code as above).
  - On return the stream drops with the function's locals; Task 3's handle joins the thread. The join is quick
    (≤ 100 ms source timeout), but it runs on a tokio worker. Wrap it: `impl Drop for StreamHandle` only sets
    the flag and detaches; the thread exits by itself within 100 ms. Change Task 3's test to
    `wait_until(live_streams() == before)` instead of an immediate assert.
  - `SessionCtx` gains `video_source: VideoSourceKind` (`Copy`).
- [ ] **Step 4: Run the end-to-end tests, then the full check.** Expected: PASS. Watch that the existing
  `rejects_bad_token_and_invalid_messages` still passes: it may send messages that are now valid.
- [ ] **Step 5: Commit** `feat(video): stream the GamePad picture to the GamePad phone`.

### Task 5: Phone shows the picture

**Files:**
- Create: `apps/phone/src/video/VideoPlayer.ts` and `apps/phone/src/video/VideoPlayer.test.ts`.
- Modify:
  - `apps/phone/src/session/GamepadSession.ts`;
  - `apps/phone/src/components/gamepad/GamepadScreen.tsx`;
  - `apps/phone/src/components/gamepad/TouchScreen.tsx` (render children under the dot, no change in
    handlers);
  - `apps/phone/src/styles/gamepad.css`.
- Test: `apps/phone/e2e/gamepad-video.spec.ts` (new).

**Interfaces:**
- Consumes: Task 2 `Connection` events, `supportsVideo`, `VideoFrame`.
- Produces:
  - `class VideoPlayer { constructor(deps: { decode(jpeg: Uint8Array): Promise<ImageBitmap | null>; draw(img:
    ImageBitmap): void; ack(seq: number): void }); push(f: VideoFrame): void; reset(): void }`;
  - `GamepadSession.video`: a store with `subscribe` and `getState(): VideoView`, where
    `type VideoView = 'off' | 'waiting' | VideoState`;
  - `GamepadSession.attachCanvas(c: HTMLCanvasElement | null)`.

- [ ] **Step 1: Failing unit tests (`VideoPlayer.test.ts`).**

```ts
it('draws frames in order, keeps only the newest while decoding, and acks each drawn frame', async () => {
  const decodes: ((v: ImageBitmap | null) => void)[] = [];
  const drawn: number[] = [];
  const acks: number[] = [];
  const player = new VideoPlayer({
    decode: () => new Promise((res) => decodes.push(res)),
    draw: (img) => drawn.push((img as unknown as { seq: number }).seq),
    ack: (seq) => acks.push(seq),
  });
  const frame = (seq: number) => ({ width: 854, height: 480, seq, jpeg: new Uint8Array([seq]) });
  player.push(frame(1));
  player.push(frame(2)); // waits
  player.push(frame(3)); // replaces 2
  decodes[0]({ seq: 1 } as unknown as ImageBitmap);
  await Promise.resolve(); await Promise.resolve();
  expect(drawn).toEqual([1]);
  expect(acks).toEqual([1]);
  expect(decodes).toHaveLength(2); // frame 3 is decoding, frame 2 was skipped
  decodes[1]({ seq: 3 } as unknown as ImageBitmap);
  await Promise.resolve(); await Promise.resolve();
  expect(drawn).toEqual([1, 3]);
  expect(acks).toEqual([1, 3]);
});

it('acks frames that fail to decode, so the PC keeps sending', async () => {
  const acks: number[] = [];
  const player = new VideoPlayer({ decode: async () => null, draw: () => {}, ack: (s) => acks.push(s) });
  player.push({ width: 854, height: 480, seq: 4, jpeg: new Uint8Array([0]) });
  await new Promise((r) => setTimeout(r, 0));
  expect(acks).toEqual([4]);
});
```

  Skipped frames are never acked. The server's ack check (seq > last ack) accepts acking 3 after 1, so skipping
  is fine, and acking the newest frees both slots.
- [ ] **Step 2: Run `npx vitest run src/video`.** Expected: FAIL.
- [ ] **Step 3: Implement `VideoPlayer`.**
  - It keeps a `busy` flag and a `next: VideoFrame | null`.
  - `push(f)`: if busy, set `next = f`; otherwise call `run(f)`.
  - `run`: set `busy = true`; `await decode(f.jpeg)`; if the result isn't null, `draw` it and then `close()` it;
    `ack(f.seq)`; set `busy = false`; if `next` is set, take it and run it.
  - `reset()` clears `next` (used on `off`). Results from an old run are ignored by a generation counter.
- [ ] **Step 4: Wire `GamepadSession`.**
  - `decode` uses `createImageBitmap(new Blob([jpeg], { type: 'image/jpeg' }))`, with a catch returning null.
  - `draw` sizes `canvas.width/height` to the frame once, then `drawImage(img, 0, 0, canvas.width,
    canvas.height)`.
  - `ack` calls `connection.send({ type: 'video_ack', seq })`.
  - `onReady`: if `connection.supportsVideo` and the page is visible, send `{type:'video', on:true}` and set the
    state to `'waiting'`.
  - `onVideoStatus(s)`: set the state to `s`.
  - `onVideoFrame(f)`: if the state isn't `'streaming'`, set it to `'streaming'`; then `player.push(f)`.
  - `onDown`: `player.reset()`, state `'off'`.
  - Lifecycle `onHidden`: send `on:false`, `player.reset()`, state `'off'`. `onVisible`: if ready and
    `supportsVideo`, send `on:true`.
  - The store follows the `MotionSensor` subscribe/getStatus pattern.
- [ ] **Step 5: Wire the screen.**
  - In `GamepadScreen.tsx`:
    - `const video = useSyncExternalStore(session.video.subscribe, session.video.getState)`;
    - inside `<TouchScreen>`, render `<canvas className="gp-video" ref={(c) => session.attachCanvas(c)}
      data-testid="gamepad-video" data-state={video} aria-hidden="true" />`;
    - replace the static hint with `videoHint(video)`. It returns null for `'streaming'`. For the others:
      - `'no_window'`: "Open Cemu's GamePad window: View → Separate GamePad view";
      - `'minimized'`: "Cemu's GamePad window is minimised. Restore it to see the picture";
      - `'waiting'`: "Waiting for the picture…";
      - `'unsupported'`, `'off'`: the current hint text;
      - `'error'`: "The PC can't capture the GamePad window".
    - Keep the `<strong>GamePad touchscreen</strong>` title in all of them.
  - CSS:

```css
.gp-video {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  border-radius: inherit;
  pointer-events: none;
}
.gp-touch[data-video='streaming'] .gp-screen-hint { display: none; }
```

  Also add `data-video={video}` on `.gp-touch`: `TouchScreen` gains an optional `video?: string` prop for that
  attribute.
- [ ] **Step 6: E2E (`gamepad-video.spec.ts`).** It reuses the helpers the GamePad specs use to connect as
  GamePad. Make a JPEG in the page: `canvas.toDataURL('image/jpeg')` of a 160×90 canvas, left half red and right
  half blue, turned into bytes in the test.

```ts
test('shows the GamePad picture, acks frames, and keeps touches 1:1', async ({ page, server }) => {
  server.protocol = 3;
  await connectAsGamepad(page);
  await expect.poll(() => server.video.at(-1)?.on).toBe(true);
  const jpeg = await makeJpeg(page, 160, 90); // left half red, right half blue
  server.sendVideoFrame({ width: 160, height: 90, seq: 1, jpeg });
  await expect(page.getByTestId('gamepad-video')).toHaveAttribute('data-state', 'streaming');
  await expect.poll(() => server.acks).toEqual([1]);
  const [left, right] = await page.getByTestId('gamepad-video').evaluate((c: HTMLCanvasElement) => {
    const g = c.getContext('2d')!;
    return [Array.from(g.getImageData(20, 45, 1, 1).data), Array.from(g.getImageData(140, 45, 1, 1).data)];
  });
  expect(left[0]).toBeGreaterThan(200); expect(left[2]).toBeLessThan(60);
  expect(right[2]).toBeGreaterThan(200); expect(right[0]).toBeLessThan(60);
  // The canvas doesn't eat touches: a tap at 25 %/50 % still reports that position.
  await tapTouchscreen(page, 0.25, 0.5);
  await expect.poll(() => server.lastPointer()).toEqual({ x: 0.25, y: 0.5 }); // tolerance as in the touch tests
  expect(server.violations).toEqual([]);
});

test('says what to do when the GamePad window is missing, and stops while hidden', async ({ page, server }) => {
  server.protocol = 3;
  await connectAsGamepad(page);
  server.sendVideoStatus('no_window');
  await expect(page.getByTestId('touchscreen')).toContainText('Separate GamePad view');
  await page.evaluate(() => {
    Object.defineProperty(document, 'visibilityState', { value: 'hidden', configurable: true });
    document.dispatchEvent(new Event('visibilitychange'));
  });
  await expect.poll(() => server.video.at(-1)?.on).toBe(false);
});

test('a protocol 2 PC gets no video messages', async ({ page, server }) => {
  server.protocol = 2;
  await connectAsGamepad(page);
  await page.waitForTimeout(300);
  expect(server.video).toEqual([]);
  await expect(page.getByTestId('touchscreen')).toContainText("Cemu's GamePad window");
});
```

  Use the existing GamePad spec's connect, tap and pointer helpers under their real names. Look in
  `e2e/gamepad*.spec.ts` and `e2e/fixtures.ts`, and use the touch tests' tolerance.
- [ ] **Step 7: Run the checks:**
  `npm run typecheck && npm run lint && npm test && npm run build && npx playwright test e2e/gamepad`. Then
  `git checkout -- apps/phone/screenshots`. Expected: PASS.
- [ ] **Step 8: Commit** `feat(phone): show the streamed GamePad picture under the touchscreen`.

### Task 6: Windows capture of Cemu's GamePad window

**Files:**
- Modify:
  - `crates/wiiu-connect/src/tv_border/cemu.rs`: add `choose_gamepad_window`. Keep the module where it is; add a
    `pub(crate) use` in `video` if needed.
  - `crates/wiiu-desktop-sys/src/tv.rs`: add `capture_client_area(handle) -> Option<[i32; 4]>` and
    `is_minimized(handle) -> bool`.
  - `crates/wiiu-desktop-sys/Cargo.toml`: windows-sys feature `Win32_Graphics_Dwm`.
  - `crates/wiiu-desktop-sys/src/test_support.rs`: `StandIn::with_color(title, rect, rgb)`, which paints its
    client area that colour.
  - `crates/wiiu-connect/Cargo.toml`: under `[target.'cfg(windows)'.dependencies]`, add
    `windows-capture = "2.0"`.
- Create: `crates/wiiu-connect/src/video/windows.rs` (cfg windows): `CemuSource`, and
  `WindowSource::for_window(handle)` for tests.
- Test:
  - `crates/wiiu-connect/src/tv_border/cemu.rs` tests (all platforms);
  - `crates/wiiu-connect/tests/video_windows.rs` (`#![cfg(windows)]`);
  - `crates/wiiu-desktop-sys/tests/tv_windows.rs`.

**Interfaces:**
- Consumes: `tv::top_level_windows`, `WindowInfo`, and Task 3 `VideoSource`, `SourceEvent`, `Bgra`.
- Produces:
  - `pub fn choose_gamepad_window(windows: &[WindowInfo]) -> Option<&WindowInfo>`;
  - `tv::capture_client_area(handle: u64) -> Option<[i32; 4]>`: the client rectangle relative to the top-left of
    what Windows.Graphics.Capture delivers for the window. WGC captures the DWM extended frame bounds, so the
    result is the client rect in screen coordinates (`GetClientRect` + `ClientToScreen`) minus
    `DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS)`'s left and top;
  - `tv::is_minimized(handle: u64) -> bool` (`IsIconic`).

- [ ] **Step 1: Failing test for the window choice** (in `cemu.rs`, next to the main-window tests, same
  `WindowInfo` builder):

```rust
#[test]
fn picks_the_gamepad_window_in_any_language_while_a_game_runs() {
    let w = |title: &str| WindowInfo { handle: title.len() as u64, pid: 1, title: title.into(), exe_name: "Cemu.exe".into() };
    let main = w("Cemu 2.6 - FPS: 59.94 Vulkan [TitleId: 00050000-101c9400] Zelda");
    assert_eq!(choose_gamepad_window(&[main.clone(), w("GamePad View")]).unwrap().title, "GamePad View");
    assert_eq!(choose_gamepad_window(&[main.clone(), w("GamePad-Ansicht - FPS: 59.94")]).unwrap().title,
               "GamePad-Ansicht - FPS: 59.94");
    // Other Cemu windows (settings, game list) and other programs are never chosen.
    assert!(choose_gamepad_window(&[main.clone(), w("General settings")]).is_none());
    let other = WindowInfo { exe_name: "notepad.exe".into(), ..w("GamePad View") };
    assert!(choose_gamepad_window(&[other]).is_none());
}
```

  Rule, from Cemu main `2c212b5`:
  - `PadViewFrame.cpp` creates `wxFrame(nullptr, …, _("GamePad View"))`;
  - `wxWindowSystem.cpp` retitles it `"{GamePad View} - FPS: {:.02f}"` every FPS update;
  - the main window's title starts with `"Cemu "`.
  So the GamePad window is a window of `Cemu.exe` (any case) whose title doesn't start with `"Cemu "`, and is
  either exactly `"GamePad View"` or contains `" - FPS: "`. Add these refs to `docs/research.md`.
- [ ] **Step 2: Run, watch it fail, then implement and pass.** Run `cargo test -p wiiu-connect tv_border::cemu`.
- [ ] **Step 3: Implement the desktop-sys functions.** Use `windows-sys` `DwmGetWindowAttribute(hwnd,
  DWMWA_EXTENDED_FRAME_BOUNDS, &mut rect as *mut _ as *mut c_void, size_of::<RECT>() as u32)`,
  `GetClientRect` and `ClientToScreen`, with `// SAFETY:` comments as in the rest of `tv.rs`. Type-check with
  `cargo check -p wiiu-desktop-sys --target x86_64-pc-windows-msvc --tests`.
- [ ] **Step 4: Add the `StandIn` colour.** Register a second window class with `hbrBackground =
  CreateSolidBrush(rgb)`, so `DefWindowProc` paints the client area that colour.
- [ ] **Step 5: Implement `video/windows.rs`.**
  - `WindowSource::for_window(handle)` starts a `windows-capture` capture:
    - `Window::from_raw_hwnd(handle as usize as *mut c_void)`;
    - `Settings::new(window, CursorCaptureSettings::WithoutCursor, DrawBorderSettings::WithoutBorder,
      SecondaryWindowSettings::Default, MinimumUpdateIntervalSettings::Custom(Duration::from_millis(33)),
      DirtyRegionSettings::Default, ColorFormat::Bgra8, flags)`. Check the 2.0 signature in
      `~/.cargo/registry/src/*/windows-capture-2.0.1/src/settings.rs`.
  - Its `GraphicsCaptureApiHandler::on_frame_arrived` copies `frame.buffer()?.as_nopadding_buffer(&mut tmp)` into
    a `Bgra`, cropped to `capture_client_area(handle)` clamped to the frame. It stores the frame in an
    `Arc<Mutex<Option<Bgra>>>` and notifies a `Condvar`.
  - `next(timeout)` waits on the condvar. It returns `Frame` when one arrived. Otherwise it returns
    `Status(Minimized)` if `is_minimized(handle)`, `Status(NoWindow)` if the window is gone (`window_owner_pid`
    is None), and `Idle` otherwise.
  - The capture is stopped on drop (`CaptureControl::stop`).
  - `DrawBorderSettings::WithoutBorder` can fail on Windows 10. On a settings error, retry once with
    `DrawBorderSettings::Default`.
  - `CemuSource` wraps it. Every 1 s without a capture, it lists windows (`tv::top_level_windows` mapped to
    `WindowInfo` as `windows_desktop.rs` does) and runs `choose_gamepad_window`. It starts a `WindowSource` when
    found, and returns `Status(NoWindow)` while not found. If the window disappears, it drops the capture and
    searches again.
  - Type-check through the scratch crate as for the TV border (see docs/progress.md, 2026-10-13 TV border).
- [ ] **Step 6: Windows CI test (`tests/video_windows.rs`).**

```rust
#![cfg(windows)]
use std::time::{Duration, Instant};
use wiiu_connect::video::{SourceEvent, VideoSource, WindowSource};
use wiiu_desktop_sys::test_support::StandIn;

#[test]
fn captures_only_the_client_area_of_a_window() {
    wiiu_desktop_sys::tv::make_dpi_aware();
    let w = StandIn::with_color("GamePad View", [100, 100, 100 + 870, 100 + 520], (200, 30, 160));
    let mut src = WindowSource::for_window(w.handle()).expect("capture starts");
    let deadline = Instant::now() + Duration::from_secs(10);
    let frame = loop {
        assert!(Instant::now() < deadline, "no frame within 10 s");
        if let SourceEvent::Frame(f) = src.next(Duration::from_millis(200)) { break f; }
    };
    let client = wiiu_desktop_sys::tv::client_rect(w.handle()).unwrap();
    assert_eq!((frame.width as i32, frame.height as i32), (client[2] - client[0], client[3] - client[1]));
    for (x, y) in [(2, 2), (frame.width - 3, 2), (2, frame.height - 3), (frame.width / 2, frame.height / 2)] {
        let i = (y * frame.stride + x * 4) as usize;
        let (b, g, r) = (frame.data[i], frame.data[i + 1], frame.data[i + 2]);
        assert!(r.abs_diff(200) < 12 && g.abs_diff(30) < 12 && b.abs_diff(160) < 12,
                "pixel ({x},{y}) = {r},{g},{b}: the crop includes the frame or title bar");
    }
}
```

  This makes `video` and `WindowSource` public from the library crate: `pub mod video` in `lib.rs`, and
  `pub use windows::WindowSource` under `cfg(windows)`.
  **Decision point:** if Windows.Graphics.Capture can't start on the GitHub runner (no capture support on
  Windows Server), the test fails with the error. Then record the error in `docs/progress.md`, ask the owner, and
  don't mark the test ignored without that decision.
- [ ] **Step 7: Push and read the Windows CI job.** Expected: the new tests pass, or the decision point above.
- [ ] **Step 8: Commit** `feat(video): capture Cemu's GamePad window on Windows`.

### Task 7: Docs

**Files:**
- Modify: `docs/adr/0002-gamepad-screen-streaming.md` (status Accepted for the JPEG-first design, with the three
  changes listed in this plan's Spec line and the facts above); `docs/research.md` (the Cemu refs from Task 6);
  `docs/controller-protocol.md` (status line for version 3); `TESTING.md` (GamePad section: the picture should
  appear; report fps feel and lag); `docs/compatibility.md` (streaming unverified with real Cemu);
  `docs/roadmap.md` (1.3 status); `CHANGELOG.md` (Added); `docs/progress.md` (session entry, including measured
  JPEG sizes from the test pattern and what the owner should measure: lag, sharpness, Wi-Fi).
- [ ] **Step 1: Write the docs**, using plain words, and mark every real-Cemu claim "unverified".
- [ ] **Step 2: Commit** `docs: GamePad screen streaming`.
- [ ] **Step 3: Push, open a draft PR** stacked on `feat/tv-border` (base `feat/tv-border` until #29 merges, then
  `main`), subscribe to it, and run the final whole-branch review.
