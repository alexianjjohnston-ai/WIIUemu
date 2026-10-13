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
1. **Get Cemu:** it uses your Cemu if it finds one. Otherwise it unpacks the official Cemu release that is built
   into the Windows exe into its own folder; no internet needed. (Builds without a built-in Cemu, such as
   `cargo run`, download it from github.com/cemu-project/Cemu instead.) **Update Cemu** on the setup page
   downloads the newest release later, keeping your settings.
2. **Controllers:** Cemu's Controller 1 becomes the phone GamePad and Controllers 2–4 the phone Wii Remotes. Your
   previous settings are backed up first (`controllerN.xml.before-wiiu-connect`).
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

### GamePad (phone 1)

1. Scan the QR code, choose **GamePad**, tap **Connect**, and turn the phone sideways.
2. In a game, check:
   - the buttons and both sticks, including their directions;
   - that a tap on the dark touchscreen area lands at the same spot in Cemu's GamePad view (*View → Separate
     GamePad view*). The picture itself is not streamed to the phone yet.
3. Tap the **motion button** (phone icon, top bar) and allow motion access. Tilt the phone in a game that uses the
   GamePad gyro and note whether up/down and left/right are correct.

The Wii Remote screen has the same motion button for shaking and twisting.

## 4. Camera pointing (experimental)

1. **Windows:** start a game in Cemu. The setup page's **TV border** card should be on **Auto**. When a phone turns
   its camera on, black-and-white squares appear around the game on the screen that shows Cemu, and the game
   shrinks to fit inside them. They go away about 3 s after the last camera turns off, and Cemu's window goes back to
   how it was. **On** keeps them up and **Off** never shows them.
   **macOS and Linux:** open `http://localhost:8737/tracking` on the TV or monitor and make it fullscreen (F11).
2. On the phone, tap the **camera icon** and allow camera and motion access.
3. Hold the phone upright with its back camera facing the screen. A ring should follow your aim, and the phone
   shows "Aiming at the TV · N tags · N fps".
4. **Calibrate once per phone:** tap **Calibrate pointer**, then aim at the middle of the game (or at the cross in
   the middle of the `/tracking` page) during the 3-second countdown and hold still. The phone shows
   "Calibrated ✓" and remembers it; the ring should now sit where you aim. **Recalibrate** or **Reset** appear
   while the PC sees the border.

With the TV border, also check: does Cemu's window come back exactly as it was (title bar, menu, size, full
screen) when the border goes away, when you close Wii U Connect, and after you end Wii U Connect in Task Manager and
start it again? Does the game picture stay inside the squares when you toggle Cemu's full screen?

## What to report

- Do the buttons work in-game? Does the pointer move the right way up and down?
- Camera: the fps shown, how far the ring is from where you aim before and after calibrating, and how laggy it
  feels.
- Phone model and browser, and any error text from the phone or the server window.
