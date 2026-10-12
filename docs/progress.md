# Progress log

Hand-off notes for the next session. Newest first.

## 2026-10-12: Milestone 0 + Milestone 1 (software side)

### Done
- Repository bootstrapped (`main`), feature branch `feat/virtual-wiimote`.
- **Key finding:** an unmodified Cemu can take an independent Wii Remote IR pointer per player from DSU touch
  point 1 (see `docs/research.md` §1). This removes the need for a Cemu patch for Milestones 1, 3 and 4.
- `wiiu-dsu`: DSU server, with layouts verified against Cemu's packed structs (g++ `offsetof` check).
- `wiiu-controller`: protocol v1 validation, button tables, pointer/motion conversion.
- `wiiu-cemu`: Cemu profile XML (`WiiUConnect_Wiimote{1,2,3}.xml`).
- `wiiu-connect`: server binary (WebSocket, pairing, safety rules, `/pair`, `/dev/pointer`, `probe`,
  `cemu-profile`).
- Live check in the container: Playwright drove `/dev/pointer` while `wiiu-connect probe` received the
  expected DSU data. Mouse at (0.25, 0.75) became touch (480, 707); A became Cross; H became L3; the slot went
  neutral after the page closed.
- Phone Wii Remote PWA: built by the mobile interface agent in a separate worktree. See the entry for its merge
  (below, once merged).

### Tests
- Rust: 63 tests (unit + loopback UDP + end-to-end WebSocket → DSU) pass; `clippy -D warnings` and `fmt` are
  clean.

### Blocked / unverified
- **No Cemu or Windows in the development container.** Nothing has been verified inside Cemu or a real game
  yet. Milestone 1 is not complete until someone on Windows with Cemu confirms in a pointer game (e.g. Wii Party
  U):
  1. `wiiu-connect cemu-profile` profiles load, and buttons respond.
  2. `/dev/pointer` moves the in-game pointer, and the Y direction is correct.
  3. Two pointer pages (two browser profiles) move two independent pointers.
- Physical phone behaviour (multi-touch, haptics) is unverified.

### Next
1. Merge the phone PWA branch after review; build it into `apps/phone/dist` so `wiiu-connect` serves it.
2. Windows + Cemu verification of the three checks above. Record the results in `docs/compatibility.md`.
3. Milestone 2: HTTPS for phones (needed for camera and motion), then the AprilTag border renderer and
   detection prototype (`docs/tracking.md`).
4. Wire phone motion sensors (DeviceMotion) to the protocol `motion` field, then verify the axis convention in
   Cemu.
