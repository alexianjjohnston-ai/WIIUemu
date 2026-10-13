# ADR-0004: Draw the TV marker border around Cemu's game window

**Status:** Proposed (design approved in conversation 2026-10-13; this written design awaits the owner's review)
**Date:** 2026-10-13
**Deciders:** repository owner; lead engineer

## Context

Camera pointing needs the AprilTag border around the game picture (`docs/tracking.md`). Today the border exists only
on the `/tracking` browser page, so during real play the phones see no markers and the pointer is lost
(`docs/roadmap.md` 1.2). The owner chose option A, a smaller game picture inside a border, and asked for this before
GamePad streaming.

Owner answers (2026-10-13):
- Screens: "not sure / both". The border goes on the screen that shows Cemu's game window, whether that is the
  only screen (a TV) or a second one.
- When: **automatic**. The border is on while any Wii Remote phone has camera pointing on, plus a setup switch
  Auto / On / Off.

Facts:
- The marker layout is `wiiu_tracking::markers::MarkerLayout::border(&BorderConfig::default())`: 16:9 game area,
  tags 20 % of the game height, a gap of 1 %, 4 tags per horizontal edge and 3 per vertical edge, coordinates
  normalised to the game area. The phones' trackers already use this layout.
- With that layout the full border is 1.42 × the game height, so on a 1920×1080 screen the game area is
  1351×760 px (70 % of the height, about half the area).
- Cemu's game picture is drawn in its main window below the menu bar. The exact window class and child layout are
  **unverified**; the design finds the window by process and measures the client area instead of assuming a
  class name.

## Decision

**On Windows, Wii U Connect opens a borderless, black, full-screen "TV border" window on the monitor that shows
Cemu's main window, draws the tags into it, and moves Cemu's main window into the inner game area with its title
bar and menu hidden. It restores Cemu's window when the border turns off, when Cemu exits, and when Wii U Connect
quits.** macOS and Linux keep the `/tracking` page for now.

## Options Considered

| Option | For | Against |
|--------|-----|---------|
| **A1. Own border window + move Cemu's window (chosen)** | Exact pixel placement, no added latency, uses the existing layout | Changes another program's window styles; must restore them reliably; Cemu may react badly (to test) |
| A2. `/tracking` page full-screen in the browser + move Cemu over it | Reuses the page | Browser chrome and display scaling make the inner rectangle guesswork; the player must make the browser full-screen |
| A3. Capture Cemu's picture and redraw it inside the border | No window tricks | At least one frame of added latency on the TV; needs the capture work of ADR-0002 first |

## Design

### Units

1. **`tv_layout` (pure, all platforms).** Input: monitor rectangle in physical pixels, `BorderConfig`. Output: the
   game rectangle (largest 16:9 area whose border fits, centred, integer pixels) and each tag's pixel square. Tested
   for 1280×720, 1920×1080, 2560×1440, 3840×2160, 21:9 and 4:3 monitors. Each tag stays square and inside the
   monitor, and the game rectangle's aspect is within one pixel of 16:9.
2. **`tv_border` controller (pure, all platforms).** Inputs: setting (Auto / On / Off), number of Wii Remotes with
   camera pointing on, whether a Cemu main window exists, and the time. Output: show or hide. In Auto it shows while
   at least one camera is on, and hides 3 s after the last camera turns off, so a quick reconnect doesn't make the
   game jump. Tested as a state machine with a fake clock.
3. **`tv_window` (Windows only, `windows-sys`).**
   - **Find Cemu:** the visible top-level window of the Cemu process Wii U Connect started or found (`Cemu.exe`
     path from setup) with the largest area. That is the main window, not the "GamePad View" window.
   - **Border window:** `WS_POPUP`, not topmost, covering the monitor that holds most of Cemu's window
     (`MonitorFromWindow`). It paints black, then the tags rendered by `wiiu_tracking::tags` into one bitmap
     (`StretchDIBits` with nearest-neighbour, so tag edges stay sharp).
   - **Place Cemu:** save its style, menu and placement. Remove `WS_CAPTION | WS_THICKFRAME`, hide the menu
     (`SetMenu(hwnd, null)`), then `SetWindowPos` so that its **client area** equals the game rectangle (computed
     with `AdjustWindowRectEx` on the new style, then checked with `GetClientRect`/`ClientToScreen`). Z-order:
     Cemu's main window directly above the border window. Cemu's GamePad window, if on the same monitor, stays above
     both: it may hide a tag or two, and tracking needs only two.
   - **Restore:** put back the saved style, menu and placement. It runs when the border hides, from the shutdown
     path, and from a panic hook. If Cemu exits first, the border just closes.
4. **Wiring.** The tracking workers already know which Wii Remotes have camera pointing on. They report the count to
   the controller, and a small task applies the controller's output through `tv_window` (Windows) or does nothing
   (other platforms).

### Setup page

A "TV border" row: Auto / On / Off, and a status line: "Off", "On (game 1351×760 on Display 2)", "Waiting for Cemu's
game window", or an error. The setting is stored with the setup state. Changing it is a loopback-only, CSRF-checked
API call like the other setup actions.

### Phones

No protocol change. The calibration hint changes from "aim at the cross on the TV page" to "aim at the middle of
the game picture" when the border is on. The PC reports this in the existing `tracking` status as
`border: "game"` vs `"page"`, an additive optional field documented in `docs/controller-protocol.md` first.

## Testing

- Unit: `tv_layout` (above), the controller state machine, and the save/restore bookkeeping of `tv_window` behind
  a trait with a fake.
- **Windows CI:** an integration test opens a stand-in "Cemu" window (title bar, menu, thick frame) in the test
  process. It shows the border, checks that the stand-in's client area equals the game rectangle on the runner's
  monitor, hides the border, and checks that style, menu and rectangle are back to the originals.
- **Release workflow (Windows):** start the built-in Cemu, wait up to 20 s for its main window, show and hide the
  border, and check the same. If Cemu can't open a window on the runner (no GPU), the step reports that and is
  marked not verified instead of passing.
- **End to end:** the existing server e2e test renders the tags from the same layout, so the phones' pointer math
  stays matched.
- **Owner on the TV:** recorded in `docs/compatibility.md`, until then "unverified".

## Consequences

- During camera pointing the game picture is about half its full-screen area on a 16:9 TV. Smaller tags at short
  distances and corner-only layouts can reduce that later (roadmap).
- Wii U Connect changes another program's window. Every change is saved and restored, and the setup switch can turn
  it off.
- macOS and Linux keep the browser `/tracking` page until a later port.
