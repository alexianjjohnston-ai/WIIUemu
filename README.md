# Wii U Connect

Turn up to four smartphones into Wii U controllers for the [Cemu](https://github.com/cemu-project/Cemu) emulator
on Windows:

- **Phone 1:** a Wii U GamePad (streamed GamePad screen, touch input, sticks, buttons, motion)
- **Phones 2–4:** Wii Remotes (original button layout, camera-based TV pointing, motion)

> Wii U Connect does not include, and will never distribute, Nintendo games, system files or console keys. You
> must supply your own legally dumped files.

## Status

Early development. See [`docs/progress.md`](docs/progress.md) for details and
[`docs/compatibility.md`](docs/compatibility.md) for what has been verified.

Working now, verified by automated tests but **not yet in a real game**:

- A Cemuhook/DSU server that Cemu connects to. Every packet layout was checked against Cemu's own structs.
- Virtual Wii Remotes 1–3 with all buttons and an **independent IR pointer per player**, using Cemu's DSU touch
  → pointer path (no Cemu patch required).
- A phone connection server with QR pairing, slot assignment and safety rules (buttons release on disconnect or
  after 1 s without input).
- A phone Wii Remote web app (`apps/phone`): authentic and comfort layouts, left-handed mode, button size,
  multi-touch, and release of all buttons when the page is hidden or the connection drops. It has been tested in
  emulated phone browsers only, not on real phones yet.
- A mouse-driven pointer test page for testing without phones.
- Generation of Cemu controller profiles, so you don't have to bind buttons by hand.

Not built yet: camera pointing (AprilTags), the GamePad video stream, motion from phone sensors, and the desktop
app/installer.

## Try it (developer preview)

Requires Rust 1.82+ and Cemu 2.x on Windows.

```sh
cargo run --release -p wiiu-connect
```

1. Write Cemu profiles once:
   `cargo run --release -p wiiu-connect -- cemu-profile --dir "<Cemu folder>\controllerProfiles"`
2. In Cemu, open **Options → Input settings**, select **Controller 2**, choose emulated controller **Wiimote**,
   and load the profile **WiiUConnect_Wiimote1**. Do the same for Controllers 3–4 with Wiimote2/3 if you want
   more players.
3. Open `http://localhost:8737/dev/pointer` on the PC and aim with the mouse. Left click is A, right click is B,
   arrows are the D-pad, and H is HOME.
4. Phones: build the phone app once (`cd apps/phone && npm ci && npm run build`), then open
   `http://localhost:8737/pair` on the PC and scan the QR code with the phone (same Wi-Fi). Pick **Wii Remote** and
   tap **Connect**.

Without Cemu, you can watch exactly what Cemu would receive:
`cargo run -p wiiu-connect -- probe --slot 1`.

## Documentation

- [Architecture](docs/architecture.md) · [Controller protocol](docs/controller-protocol.md) ·
  [Emulator research](docs/research.md) · [Tracking design](docs/tracking.md) ·
  [Development tools](docs/development-tools.md) · [Changelog](CHANGELOG.md)
