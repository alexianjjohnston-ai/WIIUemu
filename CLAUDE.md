# Wii U Connect: project instructions

Windows app that turns phones into Wii U controllers for Cemu: phone 1 is the GamePad, phones 2–4 are Wii
Remotes with camera-based pointing. Read `docs/progress.md` first. It is the hand-off log between sessions.

## Layout

| Path | What |
|------|------|
| `crates/wiiu-dsu` | Cemuhook DSU protocol + UDP server. Byte layout must match Cemu's `DSUMessages.h`. |
| `crates/wiiu-controller` | Pure logic: button tables, phone protocol validation, input → DSU pad. No I/O. |
| `crates/wiiu-cemu` | Cemu profile XML generation. |
| `crates/wiiu-connect` | Binary: WebSocket session server, pairing, DSU bridge, PC-only pages (`web/`). |
| `apps/phone` | React + TypeScript PWA (phone controllers). |
| `docs/` | Architecture, protocol, research, tracking, progress. |

## Commands

```sh
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cargo run -p wiiu-connect                      # server: http://localhost:8737/pair, /dev/pointer
cargo run -p wiiu-connect -- probe --slot 1    # watch DSU output like Cemu would
cargo run -p wiiu-connect -- cemu-profile --dir <cemu>/controllerProfiles
cd apps/phone && npm ci && npm run typecheck && npm run lint && npm test && npm run build && npm run test:e2e
```

## Rules

- **Verify, don't assume.** Emulator behaviour claims need a Cemu source reference (file + commit) in
  `docs/research.md` or a real-game test in `docs/compatibility.md`. Never mark hardware or in-game behaviour as
  working without running it; write "unverified" instead.
- **Protocol changes** go to `docs/controller-protocol.md` first, then `wiiu-controller` (Rust) and
  `apps/phone` (TS) in the same PR.
- **Safety invariants** (tested in `session.rs`): disconnect or 1 s of silence releases all input; a socket can
  only write its own slot; the DSU server stays on loopback by default.
- **Security:** pages that reveal the pairing token must use `reject_remote` (loopback peer + loopback Host).
  WebSocket upgrades check `Origin`. Validate every phone field; reject, don't clamp.
- **Legal:** never commit Nintendo games, system files, keys, or trademarked artwork (`.gitignore` blocks common
  dump formats).
- Keep tracking, streaming, networking, emulator input and UI separately testable.

## Git workflow

- Branch from `main`: `feat/…`, `fix/…`, `docs/…`, `chore/…`. Never push directly to `main`.
- Conventional Commits, one logical change each; run the checks above before every commit.
- Push after 2–4 verified commits or at a milestone. One PR per coherent feature; draft PRs early for large work.
- Squash-merge short-lived branches once CI is green and review findings are resolved. The repo owner approves
  changes to emulator integration, security code, release automation or permissions.
- Update `docs/progress.md` and `CHANGELOG.md` with every meaningful change.
