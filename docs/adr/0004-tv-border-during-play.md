# ADR-0004: Draw the TV marker border around Cemu's game window

**Status:** Accepted (owner approved 2026-10-13); built on Windows, see "As built" at the end
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
  tags 20 % of the game height, a gap of 3 % (1 % when this was written; see "As built"), 4 tags per horizontal
  edge and 3 per vertical edge, coordinates normalised to the game area. The phones' trackers already use this
  layout.
- With that layout and the tags' one-cell white quiet zone, the full border is 1.51 × the game height. On a
  1920×1080 screen, with the `/tracking` page's 2 % margin, the game area is 1221×687 px (64 % of the height).
- Cemu's game picture is drawn in its main window below the menu bar. The window class and child layout are
  **unverified**, so the design measures the client area instead of assuming a class name.
- Window titles (Cemu `main`, 2026-10-13):
  - The main window's title always starts with `BUILD_VERSION_WITH_NAME_STRING`, which is `"Cemu "` followed by
    the version (`src/Common/version.h`). The FPS and game name are appended after it
    (`src/gui/wxgui/wxWindowSystem.cpp`, `windowText = BUILD_VERSION_WITH_NAME_STRING`).
  - The separate GamePad window is titled with the translated `_("GamePad View")`, plus `" - FPS: …"` while a game
    runs (`src/gui/wxgui/PadViewFrame.cpp`, same `wxWindowSystem.cpp`).
  - Cemu hides and recreates its own menu bar in some situations (`MainWindow::SetMenuVisible`, `RecreateMenu`).

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
   - **Find Cemu:** among the visible top-level windows of the Cemu process (`Cemu.exe` path from setup), the one
     whose title starts with `"Cemu "`. That is the main window in any UI language and at any size. The GamePad
     window's title is the translated "GamePad View" and is never chosen. If no title matches, the border stays off
     and the setup page says "Waiting for Cemu's game window".
   - **Border window:** `WS_POPUP`, not topmost, covering the monitor that holds most of Cemu's window
     (`MonitorFromWindow`). It paints black, then the tags rendered by `wiiu_tracking::tags` into one bitmap
     (`StretchDIBits` with nearest-neighbour, so tag edges stay sharp).
   - **Place Cemu:** save its style, menu and placement. Remove `WS_CAPTION | WS_THICKFRAME`, hide the menu
     (`SetMenu(hwnd, null)`), then `SetWindowPos` so that its **client area** equals the game rectangle (computed
     with `AdjustWindowRectEx` on the new style, then checked with `GetClientRect`/`ClientToScreen`). Z-order:
     Cemu's main window directly above the border window. Cemu's GamePad window, if on the same monitor, stays above
     both: it may hide a tag or two, and tracking needs only two.
   - **Keep in place:** while the border shows, a 1 s check re-applies the style, the hidden menu and the
     position if Cemu changed them, for example when it recreates its menu or the player toggles its full screen.
   - **Restore:** put back the saved style, menu and placement. It runs when the border hides, from the shutdown
     path, and from a panic hook. If Cemu exits first, the border just closes.
   - **Recovery after a crash or a Task Manager kill:** before changing Cemu's window, Wii U Connect writes
     `tv-border-restore.json` to its data folder: Cemu's process id and start time, the window handle, the saved
     style, extended style and placement, and the menu handle. `SetMenu(hwnd, null)` detaches the menu without
     destroying it, so that handle stays valid while Cemu runs. A clean restore deletes the file. On the next start,
     if the file exists, the same Cemu process still runs (pid and start time match) and the window handle still
     belongs to it, the saved style and placement are put back, and the menu too if `IsMenu` still accepts its
     handle. Then the file is deleted. If anything doesn't match, the file is just deleted, and restarting Cemu
     gives it a normal window again.
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

- Unit: `tv_layout` (above), the controller state machine, the window choice by title (Cemu titles in English,
  German and Japanese, with and without the FPS suffix), the save/restore bookkeeping behind a trait with a fake,
  and the recovery file (written before a change, deleted after a restore, applied only to the same process).
- **Windows CI:** an integration test opens a stand-in "Cemu" window (title bar, menu, thick frame) in the test
  process. It shows the border, checks that the stand-in's client area equals the game rectangle on the runner's
  monitor, hides the border, and checks that style, menu and rectangle are back to the originals. A second case
  kills the process that applied the border (a child process) and checks that the next start restores the
  stand-in from the recovery file.
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

## As built (2026-10-13, PR #29)

Differences from the design above, each decided while building it:

- **Gap 0.03, not 0.01.** With a 1 % gap, the tags' white quiet zone (one tag cell, 2.5 % of the game height)
  covered the edge of the game picture. A test now keeps every quiet zone outside the game
  (`markers::tests::quiet_zones_stay_outside_the_game`). The phones' trackers use the same layout, so nothing
  else changed. Game area on 1920×1080: 1221×687.
- **Pixel layout shared with the page.** `wiiu_tracking::screen_layout` is a port of the `/tracking` page's
  math (2 % margin, quiet zones), so the Windows border and the page draw the same border.
- **Painting:** the border window fills each white cell with `FillRect` on black. It isn't a stretched bitmap, but
  edges stay just as sharp.
- **Finding Cemu:** the window must belong to a process whose exe is named `Cemu.exe` (any case) and its title must
  start with `"Cemu "`. The exe name is used instead of the setup's Cemu path, so a Cemu the player started from
  elsewhere also works.
- **Keep in place** runs on every 250 ms tick, not every 1 s.
- **Setup page:** the status reads "On: game 1221×687" (no display name), and on macOS and Linux "Windows only for
  now".
- **No panic hook.** Restoring from a panic hook would run Win32 calls in an unknown state. The recovery file covers
  crashes, kills and panics alike: it is written before Cemu's window changes, and the next start repairs the
  window (`tv_border::driver::Driver::recover`). A clean stop restores at once.
- **Tests:** the driver (apply, keep, undo, Cemu closing, shutdown, recovery) runs against a fake desktop on every
  platform. Windows CI runs the Win32 layer against a real stand-in window: listing, client placement and full
  restore, the border window opening on the monitor and closing, and a closed window reported as gone
  (`crates/wiiu-desktop-sys/tests/tv_windows.rs`). The child-process kill case is covered by the recovery tests
  with the fake, not by a real killed process.
- **Deferred:** the release-workflow check with the real built-in Cemu. Behaviour with real Cemu and a real TV is
  **unverified** until it is tested (`docs/compatibility.md`).
- **Phones:** `tracking` carries `border: "game" | "page"` (docs/controller-protocol.md). During calibration the
  phone says "Aim at the middle of the game" when it is `game`.
