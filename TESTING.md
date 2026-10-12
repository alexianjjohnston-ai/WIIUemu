# Testing Wii U Connect (developer preview)

Nothing here has been verified on real phones or in Cemu yet. That is what this test is for.

## 1. Start the server

- **Windows:** unzip `wiiu-connect-windows-x64.zip` and double-click `wiiu-connect.exe`. If Windows asks about
  network access, allow it on private networks.
- **macOS:** unzip `wiiu-connect-macos-universal.zip`, then in Terminal, in that folder:
  `xattr -d com.apple.quarantine wiiu-connect && ./wiiu-connect`. Allow incoming connections when asked.

Keep the window open. It shows a QR code and the addresses below.

## 2. Set up Cemu (once)

In a second terminal, in the same folder, run `.\wiiu-connect.exe cemu-profile` on Windows (PowerShell) or
`./wiiu-connect cemu-profile` on macOS. It writes `WiiUConnect_Wiimote1..3` and `WiiUConnect_GamePad` into Cemu's
`controllerProfiles` folder. If it can't find the folder, add `--dir "<Cemu folder>/controllerProfiles"`.

In Cemu, open **Options → Input settings**, select **Controller 2**, set it to **Wiimote**, and load
**WiiUConnect_Wiimote1**. Keep Controller 1 as the GamePad on your keyboard: the GamePad phone isn't built yet.

## 3. Buttons

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
