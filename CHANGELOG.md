# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- `wiiu-dsu`: Cemuhook DSU protocol v1001 server, byte-compatible with Cemu's DSU client. Includes a Cemu-like
  probe client.
- `wiiu-controller`: Wii Remote and GamePad button tables, phone protocol v1 validation, input → DSU
  conversion (pointer → touch point 1920×942), and a monotonic motion clock.
- `wiiu-cemu`: Cemu controller profile generation for DSU-bound Wii Remotes.
- `wiiu-connect`: session server with token pairing, QR pairing page, slot assignment, reconnect, staleness
  watchdog, mouse pointer test page, and `cemu-profile` / `probe` commands.
- macOS support: Cemu profile folder detection, macOS CI job, universal (arm64 + x86_64) release package.
- Release workflow producing Windows and macOS zips; `v*` tags create a draft GitHub release.
- CI for Rust (Linux and Windows), the phone app, and security scanning.
- Documentation: architecture, protocol, research, tracking design, tools, compatibility, progress.
