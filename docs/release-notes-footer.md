
---

## Download

- **Windows 10/11:** `WiiUConnect-<version>-windows-x64.exe`. It is one file with nothing to install, and the
  official Cemu <cemu-version> is built in. Double-click it: it sets up Cemu, the controllers and the GamePad
  window by itself, finds your games, and opens the setup page in your browser. The first start needs no internet. Windows SmartScreen may warn about an unknown publisher, because the
  exe is not code-signed yet: click **More info → Run anyway**. Allow access on private networks when the firewall
  asks, so that phones can connect.
- **macOS (Apple Silicon and Intel):** `WiiUConnect-<version>-macos-universal.zip`. It is not notarised: after
  unzipping, run `xattr -d com.apple.quarantine wiiu-connect` once in Terminal, then `./wiiu-connect`.
- `SHA256SUMS.txt` lists the checksums of both files.

The Windows exe contains the unmodified official release of [Cemu](https://github.com/cemu-project/Cemu)
<cemu-version>, which is free software under the Mozilla Public License 2.0. Its source code is at
https://github.com/cemu-project/Cemu/tree/<cemu-version>, and a copy of its license is written next to the unpacked
Cemu (`CEMU-LICENSE.txt`). Wii U Connect is not affiliated with the Cemu project.

Wii U Connect does not include, and will never download, Nintendo games, system files or console keys. Use your own
legally dumped games. See [TESTING.md](https://github.com/alexianjjohnston-ai/WIIUemu/blob/main/TESTING.md) for a
first-run walkthrough.
