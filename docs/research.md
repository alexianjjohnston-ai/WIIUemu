# Emulator research (verified findings)

Each claim lists its source. **Source-verified** means read in Cemu's code. **Game-verified** means observed in a
real game. Nothing here is game-verified by this project yet.

Cemu sources: `cemu-project/Cemu` at commit `aa20e2f86e93` (2026-10-10).

## 1. Wii Remote IR pointer without a real Wii Remote: possible via DSU (source-verified)

- `src/input/api/DSU/DSUController.cpp`: `has_position()` is true when DSU touch point 1 or 2 is active.
  `get_position()` returns `(x / 1920, y / 942)`, and `GetPositionVisibility()` returns `FULL` while a touch is
  active.
- `src/input/emulated/EmulatedController.cpp`: `get_position()` returns the position of the first bound
  controller that has one.
- `src/input/emulated/WPADController.cpp` (`WPADRead`): when a position is available, it sets
  `status.dpd_valid_fg = 2`, `status.pos = position * 2 - 1`, and `vec`/`speed` from the previous position. This
  is the Wii Remote DPD (IR pointer) data games read through KPAD/WPAD.
- **Conclusion:** a DSU server that reports touch point 1 at `(x·1920, y·942)` drives the emulated Wii Remote's
  pointer. No Cemu patch is needed for a pointer, and each DSU slot has its own pointer, so each player aims
  independently.
- Independent confirmation: `hoshizorista/virtualwiimote` (README: "tested on Cemu 2.0.74 with Wii Party U")
  uses the same mechanism, with `tpad_w=1920`, `tpad_h=942` and no Y inversion. We read it as evidence only; its
  code is not used (it has no licence).

### Open questions (need a real game)

- Y orientation. The analysis above and virtualwiimote agree that touch y=0 is the top of the screen, but this
  project has not confirmed it in a game.
- `dpd_valid_fg = 2` means two IR dots are visible. Some games may also use `dist`/`horizon`, which Cemu does not
  fill from DSU. Twist (roll) comes from accelerometer motion only.

## 2. Buttons via DSU (source-verified)

- `DSUController::raw_state`: bits 0..7 of `state1` map to `kButton0..7` and bits 0..7 of `state2` map to
  `kButton8..15`. The `touch` byte maps to `kButton16`. The `ps` (home) byte is **ignored**.
- Cemu has **no default Wiimote mapping for DSU controllers** (`WiimoteController::set_default_mapping` only
  handles the native Wiimote API). Users must bind buttons by hand, so we generate a profile instead (§5).

## 3. Motion via DSU (source-verified)

- `DSUControllerProvider::integrate_motion` integrates a sample only when its µs timestamp is newer than the
  last one. A jump of more than 10 s backwards resets the reference. Gyro arrives in deg/s and Cemu converts it to
  rad/s. Accel arrives in g, and Cemu negates y and z.
- `WPADController::WPADRead` fills `acc`, `accVertical` and, with MotionPlus attached (`device_type` 5/6/7),
  `mpls` from the fused motion sample.
- Consequence: when a phone reconnects with a fresh clock, motion would stall. `wiiu-controller::state::MotionClock`
  rebases phone timestamps onto a monotonic server timeline.

## 4. DSU transport details (source-verified)

- Client: `DSUControllerProvider`. Up to **4 slots per server** (`kMaxClients = 4`), a 100-byte receive buffer,
  and CRC32 validation over `sizeof(struct)`: 24 bytes for the version response, 32 for port info and 100 for
  pad data.
- After every received port-info or data packet, Cemu sends a new data request for that slot.
- Packet layouts were checked against the packed C++ structs by compiling a copy with g++ and comparing
  `offsetof` values. All fields match `crates/wiiu-dsu` (header 16; packet number 32; buttons 36/37; sticks
  40–43; touch 56/62; timestamp 68; accel 76; gyro 88).
- Cemu binds the DSU client per profile (`<ip>`/`<port>` in the controller node, `DSUController::load`). The
  default port is 26760.

## 5. Controller profiles (source-verified)

- `InputManager::load` / `save` (`src/input/InputManager.cpp`): `<emulated_controller>` contains `<type>`
  (`Wiimote`, `Wii U GamePad`, …), an optional `<profile>`, emulated-controller settings (Wiimote:
  `<device_type>`), and one or more `<controller>` nodes. Each controller node holds `<api>DSUController</api>`,
  `<uuid>` (the DSU slot index, parsed by `ControllerFactory`), `<motion>`, `<axis>`/`<rotation>`/`<trigger>`,
  `<ip>`, `<port>` and `<mappings><entry><mapping/><button/></entry></mappings>`.
- `<mapping>` ids are `WiimoteController::ButtonId` values (A=1 … Home=17). `<button>` ids are
  `Buttons2` values (`kButton0..31`, then ZL=32 … `kAxisXP`=38, `kAxisYP`=39, `kAxisXN`=44, `kAxisYN`=45).
- Named profiles live in `controllerProfiles/<name>.xml`. The active per-player config is `controllerN.xml`, and
  `wiiu-connect cemu-profile` never overwrites it.

## 5b. Cemu config folders per platform (source-verified)

`CemuApp::DeterminePaths` (`src/gui/wxgui/CemuApp.cpp`):

- **Windows:** `portable/` next to `Cemu.exe`, else `%APPDATA%\Cemu`. Before 2.0-89, Cemu used the exe folder
  itself.
- **macOS:** `portable/` next to `Cemu.app` (only if built with `CEMU_ALLOW_PORTABLE`), else
  `wxStandardPaths::GetUserDataDir()`, which is `~/Library/Application Support/Cemu`.
- **Linux:** `portable/` next to the executable, else `$XDG_CONFIG_HOME/Cemu` (default `~/.config/Cemu`).

`wiiu_cemu::profile_dir_candidates_for` mirrors this.

## 6. GamePad touchscreen via DSU (source-verified, groundwork for Milestone 5)

- `VPADController` (`src/input/emulated/VPADController.cpp`): when the bound controller has a position, it sets
  `tpData.x = pos.x * 3883 + 92` and `tpData.y = 4095 - pos.y * 3694 - 254`, using the same DSU touch point. The
  phone GamePad's touchscreen can therefore use the same path as the Wii Remote pointer.

## 7. Related upstream work

- **Cemu PR #1971** "Add mouse to wiimote pointer feature" (open, ready for review since 2026-07-05): adds a
  position-only Mouse controller. Once merged, it would also allow desktop mouse aiming, but it provides one
  shared OS cursor, not independent per-player pointers. Our DSU approach does not depend on it.
- **Cemu issue #232** (open since 2022): a request for IR/cursor input from non-Wiimote sources. The thread
  predates widespread knowledge of the DSU touch path; no project maintainer has documented that path there.

## 8. Not yet investigated

- Running the Wii U Menu with user-dumped system files in the current Cemu build (Milestone 6).
- GamePad window capture options on Windows: Windows.Graphics.Capture or DXGI duplication of Cemu's separate
  GamePad window (Milestone 5).
- Whether games read the `dist` and `horizon` fields of the DPD data, which DSU cannot provide.
