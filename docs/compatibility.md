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
| Motion (accel/gyro) pass-through | protocol, mapping and timestamp handling done | unit tests only; phone sensors not yet wired |
| GamePad buttons, sticks and touch | protocol and mapping done | unit tests only; no phone UI or video yet |
| Cemu profile generation | done | schema tests against Cemu's loader; **not yet loaded in Cemu** |

## Phones

| Date | PC | Phone | Build | What was tested | Result |
|------|----|-------|-------|-----------------|--------|
| 2026-10-12 | Windows | iPhone (Safari) | `test/all-features` release package (commit `4298194`) | Server start, QR pairing over HTTPS (self-signed certificate accepted), Wii Remote connect, LED 1 lit | **works** (reported by the repository owner) |

Still unverified on real phones: buttons reaching a game in Cemu, multi-touch, haptics, camera pointing, motion
sensors and Android. Everything else is tested with Playwright mobile emulation (Pixel 7, iPhone 13 viewports),
which cannot verify real multi-touch, haptics, sensors or cameras.
