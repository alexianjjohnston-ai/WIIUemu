
---

## Download

- **Windows 10/11:** `WiiUConnect-<version>-windows-x64.exe`. It is one file with nothing to install. Double-click it,
  and the setup page opens in your browser. Windows SmartScreen may warn about an unknown publisher, because the
  exe is not code-signed yet: click **More info → Run anyway**. Allow access on private networks when the firewall
  asks, so that phones can connect.
- **macOS (Apple Silicon and Intel):** `WiiUConnect-<version>-macos-universal.zip`. It is not notarised: after
  unzipping, run `xattr -d com.apple.quarantine wiiu-connect` once in Terminal, then `./wiiu-connect`.
- `SHA256SUMS.txt` lists the checksums of both files.

Wii U Connect does not include, and will never download, Nintendo games, system files or console keys. Use your own
legally dumped games. See [TESTING.md](https://github.com/alexianjjohnston-ai/WIIUemu/blob/main/TESTING.md) for a
first-run walkthrough.
