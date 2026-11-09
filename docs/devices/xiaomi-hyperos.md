# Xiaomi / POCO / Redmi (HyperOS) setup

Reference device: **POCO M8 5G**, shipped with HyperOS 2 on Android 15.
HyperOS is one of the most aggressive Android skins at killing background apps, so each step matters.
Menu names may differ slightly between HyperOS versions and regions *(to verify on the device)*.

## 1. The automatic copy: built-in mode (recommended)

paste-get reads what you copy through Android's own **wireless debugging**, with no other app to
install. In paste-get, turn on **Send what you copy**: the set-up opens, and its four steps tick
themselves as you do them in Settings. On HyperOS:

1. **Before you start**: Wi-Fi on (wireless debugging only works on Wi-Fi), and paste-get's
   notifications allowed: the pairing code is typed into one. If that notification has no field
   for the code, search Settings for **Notification style** and choose **Android**, or use split
   screen at step 4.
2. **Developer options**: Settings → About phone → tap **OS version** 7 times, until a message
   says you are a developer. **Open About phone** in paste-get goes there.
3. **Wireless debugging**: Settings → Additional settings → Developer options → **Wireless
   debugging**: on, and allow it on this network.
4. **Pairing code**: tap **Open Wireless debugging** in paste-get, then **Pair device with
   pairing code**, and type the 6 digits in paste-get's notification (or in its boxes, in split
   screen). The set-up says "All set".

Wireless debugging can be turned off afterwards: paste-get switches it on for a moment when it
needs to start its helper (also after the phone restarts, on Wi-Fi), then off again. The set-up
works on the POCO M8 5G (November 2026); copies, and starting again after a restart, are still to
check there (`docs/TESTING.md`, section 12, W2–W6).

- Don't run the scan in the **Security** app: it can turn Developer options off, which stops
  the built-in mode until it is set up again
  ([Shizuku guide](https://shizuku.rikka.app/guide/setup/), same cause).
- Android shows "Shell pasted from your clipboard" on each copy, as with Shizuku. The Copies
  screen can hide it (*Hide "pasted from your clipboard"*, which turns Android's *Show clipboard
  access* off for every app).

## 2. Keep paste-get alive

paste-get lists these in its own settings (the gear → **Keep paste-get running**): battery and
notifications are checked by the app; Autostart and Battery saver can't be read by any app, so
you tick them there once done.

Settings → Apps → Manage apps → paste-get:
- **Autostart**: on
- **Battery saver**: No restrictions
- **Notifications**: allowed (the persistent notification keeps the service running)

Also: open recent apps, long-press paste-get → **lock** it.

## 3. Permissions paste-get will ask for

| Permission | Why |
|---|---|
| Notifications | Foreground service, "sent ✓" feedback, and the built-in mode's pairing code |
| Unrestricted battery use (Android asks "Let app always run in the background?") | Keep the connection to the PC when the screen is off |
| Camera | Scan the pairing QR code |
| Photos and videos: **Allow all** | Only for **Send new screenshots**: "selected photos" breaks detection |
| Notification access | Only for **Send 2FA codes**; granted in one tap through the built-in mode, with Android 15's hidden codes |
| Wireless debugging pairing (built-in mode) | Read the clipboard as the shell user; paste-get grants itself the rest through it |
| Shizuku: allow paste-get (Shizuku only) | Read the clipboard as the shell user (mode A) |
| Display over other apps, device logs (Shizuku only, granted through it) | Mode B, the fallback when Shizuku isn't running: a window shown for a moment reads the clipboard after the log says it changed |

## 4. Shizuku instead (optional)

If you already use Shizuku, paste-get can use it instead of the built-in mode; the built-in mode
replaces it once set up.

1. Settings → Additional settings → Developer options:
   - **USB debugging**: on
   - **USB debugging (Security settings)**: on. This is separate from USB debugging and needed
     for Shizuku to grant permissions. It may require being signed in to a Mi account (sometimes
     with a SIM inserted).
   - **Wireless debugging**: on (used to start Shizuku without a PC).
2. Install Shizuku and start it through **Wireless debugging**. If the pairing code notification
   doesn't show up, switch the notification style to "Android", as in section 1.
3. Allow Shizuku to run in the background (section 2, for Shizuku too).
4. **After every reboot, Shizuku must be started again** (no root). Meanwhile paste-get falls back
   to mode B, using permissions granted earlier.
5. In paste-get, turn on **Send what you copy**, tap **Allow in Shizuku**, then **Grant them**
   (paste-get restarts): that prepares mode B for the next reboot.

## 5. Things to check on HyperOS *(verify)*

- Whether the built-in mode starts its helper by itself after a restart on HyperOS 2, with
  Wireless debugging off (W4).
- With Shizuku: whether mode B's window may take focus from the background, or HyperOS also
  wants **Display pop-up windows while running in the background** (Settings → Apps → paste-get
  → Other permissions).
