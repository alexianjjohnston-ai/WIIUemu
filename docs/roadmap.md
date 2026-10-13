# Roadmap after the first beta

Written 2026-10-13, after PRs #20–#22. It plans the features after the work in [release-1.0.md](release-1.0.md),
and says what 1.0 still needs. Sizes are relative: **S** is about one PR, **M** a few PRs, **L** a multi-week
feature with a prototype first. Every hardware claim follows the project rule: it counts as done only after a test
on real hardware, recorded in [compatibility.md](compatibility.md).

## Where things stand

| Area | State |
|------|-------|
| One-file Windows exe, Cemu built in, automatic setup | merged (#21); verified on a Windows CI runner |
| Phone GamePad controls, motion | merged (#20); emulator tests only |
| Wii Remote buttons, camera pointing, calibration | merged / #22; **never used in a game** |
| GamePad screen on phone 1 | designed (ADR-0002), not built |
| Releases | release workflow merged; **no version tagged yet** |
| Real-hardware results | one: pairing and Wii Remote connect on an iPhone (2026-10-12) |

The biggest risk is that almost nothing has run in a real game yet. Everything below is ordered so that real
feedback arrives early.

## Part 1: finishing 1.0

### 1.1 First beta and hardware pass (S, owner + me)

- I set the version to `0.9.0-beta.1`, write its CHANGELOG section and push the tag. The workflow builds the
  draft release, and the owner publishes it.
- The owner runs the checklist in `release-1.0.md` with the beta. A screenshot of `/tracking` with a phone
  connected is the most useful report for camera pointing.
- **Why first:** controls, motion directions, calibration and Cemu's real `settings.xml` are all unverified.
  Bugs found now are cheaper than bugs found after streaming lands on top.

### 1.2 Marker border during real play (M) — new, needed for camera pointing

**Problem:** camera pointing needs the AprilTag border around the game picture, and today that border only
exists on the `/tracking` browser page. During real play, the TV shows Cemu, so the phones see no markers and
the pointer is lost. TESTING.md's workaround is to put Cemu's window by hand over the middle of that page.

**Options** (to decide in an ADR-0003, after a test on the owner's TV):

| Option | How | Cost |
|--------|-----|------|
| A. Backdrop window | Wii U Connect opens a fullscreen black window with the tag border on the TV, and moves Cemu's game window into the middle (`SetWindowPos` on Cemu's HWND). | The game picture shrinks, because the tags need room around it. Today's tags are 20 % of the game height (needed beyond 1 m, see tracking.md). |
| B. Click-through overlay | A transparent, always-on-top, click-through window draws tags over the edges of a fullscreen Cemu. | Nothing shrinks, but the tags cover the edges of the game picture. Needs a check that the overlay shows above Cemu's fullscreen mode. |
| C. Motion-only pointer | No markers: the pointer follows the phone's gyro, like an air mouse, with a recentre button. | No screen space at all, but it drifts, and players must recentre often. |

**Proposal:** build A first (simplest and most robust), offer C as a setting for players who don't want the
border, and measure B only if A's smaller picture is a real problem. Smaller tags at short range and corner-only
tags can reduce A's cost later. All three need the owner's TV and distance to judge.

### 1.3 GamePad screen on phone 1 (L) — work package 5, ADR-0002

Built in steps, so every step except the Windows capture can be tested in CI:
1. **Protocol 3** (`docs/controller-protocol.md` first): `video_frame` (server → phone, binary) and `video`
   (phone → server: start, stop, supported codecs, keyframe request).
2. **Synthetic video source** (a moving test pattern) behind a `VideoSource` trait, plus the server's
   newest-frame-only sending and keyframe-on-drop flow control.
3. **Phone decoder:** WebCodecs H.264 into the 16:9 touch area, with a JPEG fallback. Playwright checks that
   frames appear and touches still map 1:1.
4. **Windows capture and encode:** Windows.Graphics.Capture on Cemu's "GamePad View" window, then the Media
   Foundation hardware H.264 encoder. The automatic setup already turns that window on (`open_pad`).
5. **Owner measurement:** glass-to-glass latency (target < 100 ms) and bitrate (target ≤ 4 Mbit/s), then accept
   ADR-0002 or switch to its WebRTC fallback.

Risks: Windows 11 capture prompts or a yellow border; iPhone WebCodecs H.264 support; TCP stalls on busy Wi-Fi.

### 1.4 Fixes from the beta, then `v1.0.0` (S–M)

Whatever 1.1 finds, then the 1.0 tag. **Code signing** (an Authenticode certificate, bought by the owner) would
remove the SmartScreen warning. It is optional for 1.0 but worth deciding before a public announcement.

## Part 2: after 1.0

Ordered by value for players against effort. Each is a separate PR with its own tests.

| # | Feature | Size | Why | Notes and facts |
|---|---------|------|-----|-----------------|
| 1 | **Pro Controller role** | M | Many Wii U games play best with a Pro Controller (Mario Kart 8, Smash Bros.). Today a phone can only be a GamePad or a Wii Remote. | Cemu emulates "Wii U Pro Controller" and "Wii U Classic Controller" (`EmulatedController.cpp`, Cemu `21878f1`). The phone layout can reuse the GamePad controls without the screen. Needs a new role in the protocol and setup's controller assignment. |
| 2 | **Nunchuk** on the Wii Remote phone | S–M | Games such as Pikmin 3 and Nintendo Land use Wii Remote + Nunchuk. | The protocol and DSU mapping already carry the Nunchuk stick and C/Z (compatibility.md). Missing: the phone UI (a stick and two buttons on the remote's side or the second hand) and a Cemu profile with the extension. |
| 3 | **Pick and launch games from the GamePad phone** | S–M | No need to walk to the PC: the GamePad phone shows the game list, as the real GamePad does. | Reuses the setup page's game scan and launch. Needs a phone-side list and a PC-side permission rule (only slot 0, only scanned games, as today). |
| 4 | **Gyro-only pointer mode** | S | Option C above, as a player setting, also useful when the room is too dark for the camera. | `PointerFusion` already propagates the pointer from the gyro; this adds recentre and drift handling without camera fixes. |
| 5 | **Android verification pass** | S (owner) | Only an iPhone has been tried. | Chrome's camera, motion and WebCodecs behave differently from Safari. One test day with the checklist. |
| 6 | **GamePad audio on phone 1** | M–L | The real GamePad has a speaker, and some games put sound there. | Cemu can send GamePad audio to a separate output device (`pad_device`, `pad_volume` in `CemuConfig.h`). Capturing just that device needs a virtual audio device or a WASAPI loopback test. Research first. |
| 7 | **macOS parity** | M | The Mac build runs but can't install Cemu or capture the GamePad window. | Cemu install from the official macOS release, then ScreenCaptureKit for streaming. |
| 8 | **Self-update** | S–M | Wii U Connect tells the player when a new release is out and offers to download it. | Uses the GitHub releases API and the existing `SHA256SUMS.txt`. Must not replace a running exe without asking. |
| 9 | **Multi-point calibration** | S | Only if the beta shows that centre-only calibration leaves the corners off. | Measure first (tracking.md lists the metrics). |

### Not feasible without changing Cemu

- **Game rumble on the phones.** Cemu's DSU client has a `Rumble` message type marked `// TODO`
  (`src/input/api/DSU/DSUMessages.h`, Cemu `21878f1`), so Cemu never sends rumble to DSU controllers. Short
  vibrations from the phone alone (presses, connecting, calibration) are built: Android uses the Vibration API, and iPhone (iOS 18+) a switch-control trick that is still unverified on a real phone.
  Real game rumble would need a Cemu contribution first.

### Never

Downloading games, system files or console keys. Players use their own dumps (README, CLAUDE.md).

## Decisions for the owner

1. **Beta:** cut `0.9.0-beta.1` now (after #22 merges)?
2. **Marker border during play:** is a smaller game picture (option A) acceptable, or should the motion-only
   pointer (C) be the default?
3. **Order after the beta:** marker border (1.2) before streaming (1.3)? I recommend yes. Without it, camera
   pointing doesn't work in real games, and it is smaller than streaming.
4. **Code signing:** buy a certificate for 1.0, or ship 1.0 with the SmartScreen notice?
5. **After 1.0:** is the order of Part 2 right? Pro Controller support is first because it widens the playable
   game list the most.
