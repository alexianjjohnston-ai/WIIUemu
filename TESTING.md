# Testing Wii U Connect (developer preview)

Wii U Connect never downloads games, keys or Nintendo system files. Use dumps of games you own, for example made
with Dumpling on your own Wii U.

## 1. Start it

- **Windows:** unzip `wiiu-connect-windows-x64.zip` and double-click `wiiu-connect.exe`. It's a single file. If
  Windows asks about network access, allow it on private networks.
- **macOS:** unzip `wiiu-connect-macos-universal.zip`, then in Terminal, in that folder:
  `xattr -d com.apple.quarantine wiiu-connect && ./wiiu-connect`. Allow incoming connections when asked.

Keep the window open. The **setup page** opens in your browser. If it doesn't, go to `http://localhost:8737`.

## 2. Setup page

1. **Cemu emulator:**
   - **Windows:** click **Download & install Cemu**. This fetches the official release from
     github.com/cemu-project/Cemu into Wii U Connect's own folder, and sets up the controllers automatically.
   - **Mac, or if you already have Cemu:** open "I already have Cemu" and click **Use this Cemu**. A folder picker
     opens, or you can type the path.
2. **Controllers:** click **Set up controllers** if it doesn't say "Ready". Cemu's Controller 1 becomes the phone
   GamePad and Controllers 2–4 become the phone Wii Remotes. Existing settings are backed up first
   (`controllerN.xml.before-wiiu-connect`).
3. **Your games:** click **Add game folder** and pick the folder with your own dumps (`.wua` files or unpacked
   games). Encrypted `.wud`/`.wux` images also need your console's keys in Cemu's `keys.txt`.
4. **Phones:** open the pairing page link and scan the QR code (step 3 below).
5. **Play:** click **Play** next to a game.

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

1. Open `http://localhost:8737/tracking` on the TV or monitor and make it fullscreen (F11).
2. On the phone, tap the **camera icon** and allow camera and motion access.
3. Hold the phone upright with its back camera facing the screen. A ring should follow your aim, and the phone
   shows "Aiming at the TV · N tags · N fps".
4. **Calibrate once per phone:** tap **Calibrate pointer**, then aim at the cross in the middle of the `/tracking`
   page during the 3-second countdown and hold still. The phone shows "Calibrated ✓" and remembers it; the ring
   should now sit where you aim. **Recalibrate** or **Reset** appear while the PC sees the border.

There's no in-game overlay yet. For a rough in-game try, place Cemu's window over the dark middle of the
`/tracking` page without covering the markers.

## What to report

- Do the buttons work in-game? Does the pointer move the right way up and down?
- Camera: the fps shown, how far the ring is from where you aim before and after calibrating, and how laggy it
  feels.
- Phone model and browser, and any error text from the phone or the server window.
