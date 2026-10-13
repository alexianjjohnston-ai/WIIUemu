# Releasing

A release consists of:
- **Windows:** one self-contained exe, `WiiUConnect-<version>-windows-x64.exe`.
  - The phone app is compiled in (`build.rs`) and the C runtime is linked statically (`.cargo/config.toml`).
  - It has the app icon, version information and an `asInvoker` manifest.
  - Nothing needs installing.
- **macOS:** `WiiUConnect-<version>-macos-universal.zip`, holding one universal binary.
- `SHA256SUMS.txt` with the checksums of both files.

`.github/workflows/release.yml` builds and checks every file before anything is published. A release build
fails if:
- the phone app is not embedded (`WIIU_CONNECT_REQUIRE_WEB=1`);
- the exe depends on the Visual C++ runtime DLLs;
- the exe, started alone in an empty folder, does not serve the phone app over HTTP and HTTPS or reports the wrong
  version;
- the latest official Cemu can't be fetched, fails GitHub's SHA-256 digest, or its license is no longer MPL-2.0
  (the Windows exe includes it);
- on a Windows runner, the automatic setup does not install the built-in Cemu and configure it, or updating Cemu
  by download then fails (tags and `test/**` branches);
- the tag, `Cargo.toml` and `apps/phone/package.json` disagree on the version, or CHANGELOG.md has no section for
  it.

## Steps (repository owner)

1. Make sure `main` is green and contains everything for the release.
2. On a branch such as `chore/release-1.0.0`:
   - run `scripts/set-version.sh 1.0.0`;
   - rename the `## [Unreleased]` heading in CHANGELOG.md to `## [1.0.0] - <date>` and add a fresh empty
     `## [Unreleased]` above it;
   - open a PR and merge it once CI is green.
3. Tag the merge commit and push the tag:
   `git tag -a v1.0.0 -m "Wii U Connect 1.0.0" && git push origin v1.0.0`.
4. The Release workflow creates a **draft** release with notes taken from that CHANGELOG section. Download the
   files, check them on a real PC and phone (`docs/release-1.0.md`, hardware checklist), then press **Publish**.

For a release candidate, use a version like `1.0.0-rc.1`. The release is then marked as a pre-release.

To test the downloads without releasing, push any branch named `test/<something>`, or run the workflow by hand
(Actions → Release → Run workflow). The files are attached to the workflow run.

## Not done yet

- **Code signing (Windows Authenticode, Apple Developer ID and notarisation):** until then, SmartScreen and
  Gatekeeper warn on first start (the release notes explain how to continue). Signing needs a certificate that the
  owner buys; it would be stored as an encrypted repository secret, never in the repository.
- **Release builds for Linux:** they are not published. Linux users build from source.

## Included Cemu

The Windows exe contains the latest official Cemu release at build time, unchanged (`build.rs`,
`WIIU_CONNECT_BUNDLE_CEMU`). Cemu is MPL-2.0: shipping its unmodified binary is allowed as long as its license
and where to get its source come with it. The release notes name the version and link its source, and the exe writes
`CEMU-LICENSE.txt` next to the Cemu it unpacks. If Cemu ever changes its license, the workflow stops and the
bundling must be reviewed before the next release. To ship a newer Cemu, make a new release.
