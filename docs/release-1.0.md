# Release 1.0 plan

Started 2026-10-12. The repository owner decided that 1.0 includes streaming the GamePad screen to phone 1.

## What 1.0 means

One download for Windows: start it, and the setup page gets Cemu ready.
- Phone 1 is a Wii U GamePad: buttons, sticks, touchscreen, motion, and **the GamePad picture**.
- Phones 2–4 are Wii Remotes: buttons, camera pointing, and motion.

A feature counts as done only after it has been verified on real hardware and recorded in
[compatibility.md](compatibility.md). Automated tests alone are not enough.

What comes next, including what 1.0 still needs (the marker border during real play) and the features after 1.0,
is in [roadmap.md](roadmap.md).

## Work packages

| # | Package | State |
|---|---------|-------|
| 1 | Setup page and single-file exe (Cemu download, controller profiles, own games, launch) | built; download verified on a Windows runner; **picker and launch unverified** |
| 2 | GamePad controls on the phone: ZL/L/R/ZR, both sticks, D-pad, ABXY, −/HOME/+, L3/R3, 16:9 touchscreen | built (`feat/gamepad-controller`); emulator tests only |
| 3 | Phone motion → DSU for the GamePad and Wii Remotes (engine-aware axes, Cemu's convention) | built; **axis directions unverified in Cemu** |
| 4 | Setup assigns Controller 1 to the phone GamePad (DSU slot 0) | built |
| 5 | **GamePad screen streaming** (below) | not started |
| 6 | Pointer calibration (tap Calibrate, aim at the centre cross during a 3 s countdown; stored on the phone) | built (`feat/pointer-calibration`); end-to-end test with rendered camera frames; **unverified on real phones** |
| 7 | Hardware verification pass (checklist below) | owner |
| 8 | Versioning, release notes, tag `v1.0.0` → GitHub Release (Windows zip, macOS zip) | not started |

## Package 5: GamePad screen streaming (design: [ADR-0002](adr/0002-gamepad-screen-streaming.md), proposed)

Cemu can show the GamePad image in a separate window (*View → Separate GamePad view*). The plan:
1. **Capture** that window on Windows with Windows.Graphics.Capture. It works while the window is covered and
   costs little CPU. The window is found by Cemu's process and the window title. This needs a source check of the
   title in Cemu.
2. **Encode** to H.264 at 854×480 and 30–60 fps with low latency. The first choice is the Media Foundation hardware
   encoder; a software encoder is the fallback.
3. **Transport** over the existing authenticated WebSocket as binary frames, and decode on the phone with
   WebCodecs `VideoDecoder`. This needs no WebRTC stack or STUN, and uses the same pairing and TLS. If a phone
   lacks WebCodecs H.264, the server falls back to JPEG frames, which every browser decodes.
4. **Display** in the GamePad's 16:9 touch area, so a touch lands where the user sees the picture.
5. Measure **latency** from glass to glass (target < 100 ms on 5 GHz Wi-Fi) and **bandwidth** (target
   ≤ 4 Mbit/s).

Open questions:
- Capture permission prompts on Windows 11.
- Behaviour when Cemu's GamePad window is minimised.
- Which iOS version supports WebCodecs H.264 decoding.
- Whether TCP head-of-line blocking hurts on busy Wi-Fi. The fallback would be WebRTC.

None of this can be verified in the Linux development container. Every step needs a Windows test by the owner.

## Hardware verification checklist (owner, Windows + phones)

Record each result in [compatibility.md](compatibility.md):
1. **Setup:** download Cemu, set up the controllers, add a game folder, and launch a game from the page.
2. **GamePad buttons and sticks:**
   - A/B/X/Y, the D-pad, L/R/ZL/ZR and both sticks move things in a game, in the right directions.
   - HOME opens the HOME menu.
3. **GamePad touchscreen:** a tap lands at the same spot in Cemu's GamePad view, including the corners.
4. **GamePad motion:** tilting the phone away tilts the view the same way in a gyro game. Check left/right and
   up/down.
5. **Wii Remote:** buttons, camera pointing in a pointer game (Wii Party U or Pikmin 3), and motion (shake,
   twist).
6. **Four phones at once:** one GamePad and three Wii Remotes, with no slot mix-ups.
7. **Safety:** lock a phone mid-press. All of its input is released in Cemu within 1 s.
8. **Vibration (Android):** presses, a stick at its rim and connecting buzz; Off stops it, Strong is stronger.
   On an iPhone the setting says the browser can't vibrate.
