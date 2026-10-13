# ADR-0002: Streaming the GamePad screen to phone 1

- **Status:** proposed (2026-10-13). To be accepted after a Windows prototype measures latency.
- **Context:** release 1.0 includes the GamePad picture on phone 1 (owner decision, `docs/release-1.0.md`).

## Facts from Cemu's source (commit `21878f1`)

- **Separate window.** The GamePad image can be shown in its own top-level window.
  - `PadViewFrame` (`src/gui/wxgui/PadViewFrame.cpp`) is a `wxFrame` with no parent, titled `_("GamePad View")`
    (localised).
  - While a game runs, `wxWindowSystem.cpp` changes the title to `"<GamePad View> - FPS: <n>"`.
  - The window can therefore be found among the Cemu process's top-level windows, by a title that starts with the
    localised "GamePad View".
- **Opening it.** The window is toggled from the menu (`MainWindow::TogglePadView`). Its open state is saved as
  `open_pad` (`wxCemuConfig.cpp`), which Cemu reloads on start.
  - The setup page could set `open_pad` so the window opens automatically.
  - The value appears to be stored in the same `settings.xml` as the rest of the GUI config (`CemuApp.cpp` sets
    one config file). Confirm this against a real `settings.xml` before writing to it.
- **Where the image sits.** `LatteRenderTarget_getScreenImageArea` draws the GamePad image centred in the window's
  client area:
  - with `FullscreenScaling = KeepAspectRatio` (the default, `CemuConfig.cpp`), it is letterboxed to 16:9;
  - otherwise it is stretched.
  - The capture crops the same rectangle, so a touch on the phone maps 1:1 to the GamePad image. This is the same
    mapping as the DSU touch → `VPADController` path (research.md §6).

## Decision (proposed)

| Stage | Choice | Why | Fallback |
|-------|--------|-----|----------|
| Capture | Windows.Graphics.Capture on the `GamePad View` HWND | GPU-side capture that works while the window is covered, and works for Vulkan and OpenGL swapchains through DWM | GDI `PrintWindow` (slow, last resort) |
| Encode | H.264 Baseline/Constrained, 854×480, 30–60 fps, low-latency, no B-frames, through a Media Foundation hardware MFT | Every Windows GPU has one, latency is low, and CPU use is small | Software H.264 (openh264, BSD-licensed); otherwise JPEG frames |
| Transport | Binary frames on the existing authenticated `wss` connection of the GamePad phone | Reuses pairing, TLS and slot ownership; no ICE/STUN; works on any LAN | WebRTC data channel if TCP head-of-line stalls prove bad on real Wi-Fi |
| Decode | WebCodecs `VideoDecoder` → canvas in the 16:9 touch area | Hardware decode with no `<video>` buffering delay | JPEG frames → `createImageBitmap` |

Protocol: a new server → client binary message `video_frame` (header: codec, keyframe flag, sequence, capture time
in µs; then the payload) and a client → server `video` control message (start/stop, the codecs the phone supports,
keyframe request). It needs protocol version 3. Protocol 2 phones never receive video. The details go to
`docs/controller-protocol.md` first, as for any protocol change.

Flow control: the server sends only the newest frame and drops frames while the socket's send buffer is above a
threshold, as the phone already does for camera frames. Each drop is followed by a keyframe request, so a slow
link degrades to a lower frame rate, never to growing latency.

Security: video goes only to the socket that holds slot 0. The capture is limited to Cemu's own GamePad window,
never the desktop.

## Consequences

- The capture and encode code is Windows-only behind a `VideoSource` trait. A synthetic source (a test pattern)
  makes the pipeline, protocol and phone decoding testable on Linux CI and in Playwright.
- macOS capture (ScreenCaptureKit) is a follow-up. It is not needed for the Windows-first 1.0.
- Open risks to measure on the owner's PC:
  - glass-to-glass latency (target < 100 ms);
  - Windows 11's yellow capture border or prompts;
  - behaviour when the GamePad window is minimised: WGC stops delivering frames, so the phone shows "Restore Cemu's
    GamePad window";
  - WebCodecs H.264 support on the owner's iPhone.
