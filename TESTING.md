# Testing Wii U Connect (developer preview)

Wii U Connect never downloads games, keys or Nintendo system files. Use dumps of games you own, for example made
with Dumpling on your own Wii U.

## 1. Start it

- **Windows:** double-click `WiiUConnect-<version>-windows-x64.exe`. It's a single file, with nothing to install
  or unzip.
  - If SmartScreen says "Windows protected your PC", click **More info → Run anyway** (the exe is not code-signed
    yet).
  - If Windows asks about network access, allow it on private networks.
  - Starting it a second time just opens the setup page of the copy that is already running.
- **macOS:** unzip `WiiUConnect-<version>-macos-universal.zip`, then in Terminal, in that folder:
  `xattr -d com.apple.quarantine wiiu-connect && ./wiiu-connect`. Allow incoming connections when asked.

Keep the window open. The **setup page** opens in your browser. If it doesn't, go to `http://localhost:8737`.

## 2. Setup page

Wii U Connect sets itself up when it starts. The setup page shows a checklist, "Setting everything up":
1. **Get Cemu:** it uses your Cemu if it finds one. Otherwise it downloads the official release from
   github.com/cemu-project/Cemu into its own folder. That takes about a minute the first time.
2. **Controllers:** Cemu's Controllers 2–4 become the phone Wii Remotes. Your previous settings are backed up first
   (`controllerN.xml.before-wiiu-connect`).
3. **GamePad window:** Cemu opens its separate GamePad window when it starts.
4. **Your games:** it looks for your own game dumps (`.wua`, `.wud`, `.wux`, `.iso` or unpacked games) in
   Downloads, Documents, Desktop, Games and Videos, and in game-named folders on other drives. It adds the folders
   it finds to Cemu.

If a step shows **!** or **✕**, the section below it on the page lets you do that step by hand:
- choose your Cemu folder;
- add the folder with your game dumps (encrypted `.wud`/`.wux` images also need your console's keys in Cemu's
  `keys.txt`).

Then scan the **QR code on the setup page** with each phone, and press **Play** next to a game.

To skip the automatic part, start with `--no-auto-setup`; every step stays available as a button.

## 3. Buttons and phones

1. Mouse test, no phone needed: on the PC, open `http://localhost:8737/dev/pointer`. The mouse aims, left click
   is A, right click is B.
2. Phone: put it on the same Wi-Fi, open `http://localhost:8737/pair` on the PC and scan the QR code.
3. The link uses `https://…:8738` with a self-signed certificate, so the phone warns you:
   - **iPhone:** tap Show Details, then "visit this website".
   - **Android:** tap Advanced, then Proceed.
4. Tap **Wii Remote → Connect**. LED 1 lights up. Play a Wii Remote game (e.g. Wii Party U).

## 4. Camera pointing (experimental)

1. Open `http://localhost:8737/tracking` on the TV or monitor and make it fullscreen (F11).
2. On the phone, tap the **camera icon** and allow camera and motion access.
3. Hold the phone upright with its back camera facing the screen. A ring should follow your aim, and the phone
   shows "Aiming at the TV · N tags · N fps".

There's no in-game overlay yet. For a rough in-game try, place Cemu's window over the dark middle of the
`/tracking` page without covering the markers.

## What to report

- Do the buttons work in-game? Does the pointer move the right way up and down?
- Camera: the fps shown, how far the ring is from where you aim, and how laggy it feels.
- Phone model and browser, and any error text from the phone or the server window.
