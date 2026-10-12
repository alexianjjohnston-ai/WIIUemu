# Architecture

```
 Phone 1 (GamePad, landscape) ─┐                         ┌──────────── Windows PC ──────────────────────┐
 Phone 2 (Wii Remote P1) ──────┤  Wi-Fi                  │                                              │
 Phone 3 (Wii Remote P2) ──────┼──► WebSocket /ws ──────►│ wiiu-connect                                 │
 Phone 4 (Wii Remote P3) ──────┘   (JSON, protocol v1)   │  ├─ http.rs     pairing, PWA, PC-only pages  │
                                                         │  ├─ ws.rs       per-phone connection loop    │
       ◄── GamePad video (WebRTC, Milestone 5) ──────────│  ├─ session.rs  auth, slots, safety rules    │
                                                         │  └─ wiiu-dsu    DSU server ──UDP 26760──►Cemu│
                                                         │                                (loopback)    │
                                                         │  TV: Cemu game + AprilTag border (M2)        │
                                                         └──────────────────────────────────────────────┘
```

## Components and boundaries

| Component | Responsibility | Tested by |
|-----------|----------------|-----------|
| `wiiu-dsu` | DSU wire format, UDP server, Cemu-like probe client | byte-layout unit tests, loopback UDP integration tests |
| `wiiu-controller` | Button tables, protocol parsing/validation, input → `PadData`, motion clock | pure unit tests |
| `wiiu-cemu` | Cemu profile XML | schema tests (parsed back with an XML parser) |
| `wiiu-connect` | Sessions, WebSocket, HTTP, CLI | `SessionManager` unit tests with explicit clock; end-to-end tests phone → DSU |
| `apps/phone` | Controller UIs, input engine, sensors, camera (M2) | Vitest units, Playwright (mobile emulation) |

Each layer can change without the others. For example, a future Cemu patch could replace DSU behind `wiiu-dsu`,
and a binary WebRTC data channel could replace JSON over WebSocket behind `ws.rs`.

## Why DSU instead of a Cemu patch

Cemu's DSU client already maps touch point 1 to the Wii Remote IR pointer and the GamePad touchscreen, plus
buttons, sticks and motion (see `docs/research.md`). That covers Milestones 1, 3, 4 and most of 5 with an
unmodified Cemu. A patch remains the fallback for missing fields (DPD `dist`, rumble back-channel).

## Slot model

DSU has 4 slots per server, which matches the four phones: slot 0 is the GamePad and slots 1–3 are Wii Remotes
P1–P3. Slots always report "connected" with neutral input, so Cemu's bindings survive phone reconnects.

## Security model

- Phones authenticate with a 128-bit pairing token (QR code). Comparison is constant-time, and a failed attempt
  waits 300 ms before the server answers.
- The DSU server binds to loopback only. The HTTP server binds to the LAN, because phones must reach it.
- Token-revealing pages (`/pair`, `/dev/pointer`, `/api/status`) require a loopback peer **and** a loopback
  `Host` header, which blocks DNS rebinding.
- `/ws` rejects cross-origin browser upgrades unless the origin is explicitly allowed (`--allow-origin`).
- Inbound frames are text only, at most 4 KiB, rate-limited to 500/s, and strictly validated (no clamping).

## HTTPS for phones

Browsers expose the camera, and on iOS the motion sensors, only to secure contexts. `wiiu-connect` therefore
also listens on `https://<lan-ip>:8738` with a self-signed certificate (`src/tls.rs`). It creates the
certificate once in the data directory (`%APPDATA%\WiiUConnect` on Windows), reuses it while the LAN address
stays the same, and stores the private key owner-only. The QR code points at the HTTPS URL; each phone accepts
the browser warning once. Plain HTTP on 8737 stays available for PC-only pages, and `--no-https` disables the
TLS listener. Whether every phone browser grants camera and sensor access after a certificate warning is
**not yet verified on devices**.

## Planned (not built yet)

- **Tauri desktop shell** (Milestone 6): a pairing/diagnostics window that wraps the same `wiiu-connect` library.
  It is deferred because the server must work first and Tauri needs Windows WebView2 tooling; the current CI
  container has none.
- **GamePad streaming** (Milestone 5): capture Cemu's GamePad window, hardware H.264 encode, WebRTC to phone 1.
- **AprilTag tracking** (Milestone 2): see `docs/tracking.md`.
