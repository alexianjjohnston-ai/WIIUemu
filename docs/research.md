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

- **Axis convention, derived** (Cemu commit `21878f1`): `SDLControllerProvider.cpp` and `integrate_motion` feed
  the same `MotionHandler`.
  - SDL path: accel arrives as `(-a/9.81)` and is passed as `(x, -y, -z)`; gyro arrives as `(x, -y, -z)` in rad/s.
  - DSU path: accel is passed as `(x, -y, -z)`; gyro is passed as `(x, y, z)` after converting deg/s to rad/s.
  - Equating the two gives DSU accel = −(SDL accel)/g and DSU gyro = (x, −y, −z) of the SDL rate, in deg/s.
  - SDL's frame for a hand-held controller has x right, y up out of the face, and z toward the player. A DSU
    server built this way therefore behaves like an SDL controller, which Cemu users bind for motion.

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

## 5c. Setting up a Cemu install (source-verified, Cemu commit `21878f1`)

Used by the setup page (`crates/wiiu-cemu/src/install.rs`, `crates/wiiu-connect/src/setup.rs`):

- **Active controller per player:** `InputManager::load(player_index)` reads
  `controllerProfiles/controller{N}.xml`, where N = 0 is "Controller 1". This is the same XML format as named
  profiles (src/input/InputManager.cpp). Wii U Connect writes its Wii Remote profiles to `controller1..3.xml`,
  after backing up different existing files.
- **Game folders:** `CemuConfig::Load` / `Save` keep them in `settings.xml` under
  `<content><GamePaths><Entry>…</Entry></GamePaths>` (src/config/CemuConfig.cpp). Missing settings use their
  defaults. A missing `settings.xml` triggers the first-start dialog (`isFirstStart` in
  src/gui/wxgui/CemuApp.cpp), and an existing one makes Cemu create the default MLC files
  (`InitializeExistingMLCOrFail`).
- **Command line:** `-g <path>` launches a game, `-f` fullscreen (src/config/LaunchSettings.cpp).
- **Release files:** `cemu-<ver>-windows-x64.zip` (one top folder containing `Cemu.exe`),
  `Cemu-<ver>-x86_64.AppImage` and `cemu-<ver>-macos-12-<arch>.dmg` (.github/workflows/deploy_release.yml).
  The setup page installs the Windows zip and the AppImage automatically; macOS users install the dmg themselves
  and locate it.

## 6. GamePad touchscreen via DSU (source-verified, groundwork for Milestone 5)

- `VPADController` (`src/input/emulated/VPADController.cpp`): when the bound controller has a position, it sets
  `tpData.x = pos.x * 3883 + 92` and `tpData.y = 4095 - pos.y * 3694 - 254`, using the same DSU touch point. The
  phone GamePad's touchscreen can therefore use the same path as the Wii Remote pointer.

## 6b. Phone motion sensors differ by browser engine (source-verified, WebKit `main` on 2026-10-12)

- **WebKit (every iOS browser):** `Source/WebCore/platform/ios/WebCoreMotionManager.mm` passes CoreMotion
  `userAcceleration + gravity` (× 9.80665) and `rotationRate` (rad → deg) unchanged to
  `DeviceMotionClientIOS::motionChanged`. That function builds `DeviceMotionData::RotationRate::create(x, y, z)`,
  and the parameters of `create` are `(alpha, beta, gamma)` (`Source/WebCore/dom/DeviceMotionData.h`). On iOS
  therefore:
  - `rotationRate.alpha` / `beta` / `gamma` are the rates about device **x / y / z**;
  - `accelerationIncludingGravity` points **down**, because CoreMotion reports gravity as −1 g on z when the phone
    lies flat, face up.
- **W3C spec (Chromium, Firefox):**
  - `beta` / `gamma` / `alpha` are the rates about x / y / z;
  - `accelerationIncludingGravity` points up: z = +9.81 when the phone lies flat, face up.
- The phone app detects WebKit with `DeviceMotionEvent.requestPermission`, which only WebKit has, or with an iOS
  user agent, and maps both conventions to the W3C device frame (`apps/phone/src/motion/convert.ts`).
- **Consequence for camera pointing:** before this fix, iPhone gyro axes were permuted. The first iPhone test
  reported the pointer moving the wrong way. This is likely the cause, but it is **unverified** until the next
  device test.

## 7. Related upstream work

- **Cemu PR #1971** "Add mouse to wiimote pointer feature" (open, ready for review since 2026-07-05): adds a
  position-only Mouse controller. Once merged, it would also allow desktop mouse aiming, but it provides one
  shared OS cursor, not independent per-player pointers. Our DSU approach does not depend on it.
- **Cemu issue #232** (open since 2022): a request for IR/cursor input from non-Wiimote sources. The thread
  predates widespread knowledge of the DSU touch path; no project maintainer has documented that path there.

## 8. Not yet investigated

- Running the Wii U Menu with user-dumped system files in the current Cemu build (Milestone 6).
- GamePad window capture on Windows: the window facts (title, `open_pad`, the letterboxed image area) are in
  ADR-0002. The capture method itself is still to be prototyped.
- Whether games read the `dist` and `horizon` fields of the DPD data, which DSU cannot provide.
