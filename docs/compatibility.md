# Compatibility

## Tested games

None yet. A game is listed here only after someone has played it with Wii U Connect, recording the Cemu version,
the phone model and the result.

| Game | Cemu version | Controller | Feature | Result | Date / tester |
|------|--------------|------------|---------|--------|---------------|
| –    | –            | –          | –       | –      | –             |

Good first candidates for pointer verification: Wii Party U, Mario Party 10 and Pikmin 3 (Wii Remote pointer).

## Input features (as of this branch)

| Feature | Implementation | Verification |
|---------|----------------|--------------|
| Wii Remote buttons via DSU | done | unit + end-to-end tests to the DSU wire; **not yet tested in Cemu** |
| Wii Remote IR pointer via DSU touch | done | unit + end-to-end tests; Cemu path source-verified; **not yet tested in a game** |
| Three independent Wii Remotes | done (slots 1–3) | end-to-end test with three simulated phones |
| Nunchuk stick and C/Z | protocol and mapping done | unit tests only; no phone UI yet |
| Motion (accel/gyro) | phone sensors → DSU for the GamePad and Wii Remotes (opt-in button) | unit + e2e tests with synthetic DeviceMotion events in both engine conventions; **axis directions not yet checked in Cemu or on a real phone** |
| GamePad buttons, sticks and touch | phone GamePad screen, Cemu GamePad profile assigned by setup | unit + 12 e2e tests (Pixel 7 / iPhone 13 emulation, landscape); **not yet tested in Cemu**; no video yet |
| Haptic feedback (phone only) | Vibration API (Android); Off / Light / Strong. **iPhone: not possible** from a web page (no Vibration API; WebKit 313638@main blocks the switch-click workaround) | unit + e2e tests with a recorded `navigator.vibrate`; **not yet felt on a real Android phone**. Game rumble can't reach DSU controllers (Cemu `DSUMessages.h`: `Rumble // TODO`) |
| Cemu profile generation | done | schema tests against Cemu's loader; **not yet loaded in Cemu** |

## Platforms

| Platform | Build | Verified with Cemu |
|----------|-------|--------------------|
| Windows x64 | CI build + tests (`windows-latest`) | not yet |
| macOS universal (arm64 + x86_64) | CI build + tests (`macos-latest`); release packaging | not yet |
| Linux x64 | CI build + tests (development platform) | not yet |

Cemu on macOS is officially available but less mature than on Windows (Vulkan via MoltenVK). Its DSU client is the
same code as on Windows, so the input path should behave identically. This is **unverified**.

## Phones

| Date | PC | Phone | Build | What was tested | Result |
|------|----|-------|-------|-----------------|--------|
| 2026-10-12 | Windows | iPhone (Safari) | `test/all-features` release package (commit `4298194`) | Server start, QR pairing over HTTPS (self-signed certificate accepted), Wii Remote connect, LED 1 lit | **works** (reported by the repository owner) |

**TV border during play (Windows, PR #29):** tested in Windows CI against a stand-in window: placing the client
area, hiding the frame and menu, restoring them, and the border window. Cemu's real window, a real TV and phones
aiming at the border during a game are **unverified**.

Still unverified on real phones: buttons reaching a game in Cemu, multi-touch, haptics, camera pointing, motion
sensors and Android. Everything else is tested with Playwright mobile emulation (Pixel 7, iPhone 13 viewports),
which cannot verify real multi-touch, haptics, sensors or cameras.
