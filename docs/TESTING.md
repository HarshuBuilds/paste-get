# Testing paste-get on real machines

Automated tests use in-memory clipboards. Only real machines can show whether the **system
clipboard** works. Sections 1–4 test the desktop daemon on its own; section 5 adds the phone,
section 6 automatic screenshots, section 7 the way back, from the PC to the phone, section 8
the automatic copy on the phone with Shizuku, and section 12 without it (the built-in mode).
Section 9 is the tray app (with a Waybar module), section 10 starting at login, section 11 the
hotkeys. Sections 13–15 are links, 2FA codes, and copies made while a device is away. Section 16
is GNOME, which is tier 2.

To test a release instead of a build, install its packages (see the README) and skip section 1:
`pasteget` is then on the `PATH` (with the AppImage, use the .deb, .rpm or AUR package for the
terminal rows).

The trick: on one computer, run the real daemon (system clipboard) **and** a second "fake phone"
daemon with a private in-memory clipboard. Whatever reaches the fake phone came out of your real
clipboard, and whatever the fake phone sends must show up when you press `Ctrl+V`.

Logs never contain clipboard contents (only types and sizes), so they're safe to share.

## 1. Build

### Windows 11
1. Install [rustup](https://rustup.rs/). When asked, let it install the Visual Studio C++
   Build Tools.
2. In PowerShell:
   ```powershell
   git clone https://github.com/Nexoryx-business/paste-get
   cd paste-get
   cargo build --release
   ```
3. The program is `target\release\pasteget.exe`. Below, `pasteget` means that file.

If the build stops with "An Application Control policy has blocked this file" (os error 4551),
Windows 11's **Smart App Control** is blocking the small programs Cargo compiles and runs during
the build, and it would block `pasteget.exe` too, since it isn't signed yet. Turn it off in
Windows Security → App & browser control → Smart App Control settings → Off, then build again.
On most Windows 11 versions it can't be turned back on without resetting Windows; if you'd rather
keep it, test on Arch instead.

### Arch Linux (KDE Plasma or Hyprland)
```sh
sudo pacman -S --needed rustup git base-devel webkit2gtk-4.1 libayatana-appindicator
rustup default stable
git clone https://github.com/Nexoryx-business/paste-get
cd paste-get
cargo build --release
alias pasteget="$PWD/target/release/pasteget"
```
Or install it as a package, which also puts the tray in the application menu:
```sh
cd packaging/aur/paste-get-git && makepkg -si
```
(it builds the repository's default branch; to test another branch, use the `cargo build`
above).
The daemon needs no other library: Wayland and X11 are spoken directly. The two extra packages
are for the tray app (section 9). Without them, build only the daemon and the command:
`cargo build --release -p pasteget-desktop`.

## 2. Start both daemons

Use three terminals.

```sh
# Terminal 1: the real daemon, on your real clipboard
pasteget daemon

# Terminal 2: the fake phone, with its own clipboard and port
pasteget --profile phone daemon --memory-clipboard --port 47291
```

Windows asks whether to allow `pasteget.exe` on the network: allow **private networks**.

On KDE Plasma and Hyprland, terminal 1 logs `watching Wayland clipboard change events` when it
starts. Without that line it reads the clipboard at every poll: normal on X11, and on GNOME,
which logs `no Wayland clipboard change events` with the reason.

```sh
# Terminal 3: pair them
pasteget pair
```
`pair` prints a QR code and a `pasteget join "…"` line. Copy that line, add `--profile phone`,
and run it in a fourth terminal (or stop `pair` with Ctrl+C after it succeeds):

```sh
pasteget --profile phone join "pasteget://pair?…"
```
Both sides print `✓ Paired with …`. `pasteget status` should show the phone as `● connected`.

## 3. Checklist

Watch terminal 2 (the fake phone): each clip it receives logs `clip received … mime_type=… len=…`.

| # | Do this | Expected |
|---|---|---|
| 1 | Copy some text in any app | Terminal 2: `clip received … text/plain … len=<length>` |
| 2 | `pasteget --profile phone send "Bonjour 👋"`, then `Ctrl+V` in a text editor | `Bonjour 👋` is pasted |
| 3 | Take a screenshot to the clipboard (see below) | Terminal 2: `clip received … image/png` |
| 4 | `pasteget --profile phone send --file some-image.png`, then `Ctrl+V` in an app that takes images (Discord, GIMP, Paint, a chat) | The image is pasted |
| 5 | After step 2, wait 5 s | Terminal 1 does **not** log `local copy` for it (no echo) |
| 6 | Leave an image on the clipboard for a minute | `pasteget` stays under a few % CPU (Task Manager / `htop`) |
| 7 | Stop terminal 2 (Ctrl+C), start it again | Terminal 1 logs `connected` again within ~30 s |
| 8 | `pasteget pause`, copy some text, then `pasteget --profile phone send "hi"` | Terminal 1 logs `local copy not sent: paused` and `clip dropped: paused`; `Ctrl+V` doesn't paste `hi`. `pasteget resume` brings syncing back |
| 9 | `pasteget recent` | The last clips, newest first, with where they came from |
| 12 | Stop terminal 1 and start it on another port: `pasteget daemon --port 47295`, as if its address had changed | Within about 30 s terminal 2 finds it by mDNS and logs `connected` (and `reached at a new address`); the fake phone's `devices.toml` lists the `…:47295` address first. Start terminal 1 again as before afterwards |
| 11 | Stop terminal 1. Copy the PC's `devices.toml` (`%APPDATA%\paste-get\default\` on Windows, `~/.config/paste-get/default/` on Linux) somewhere safe, replace it with a line of garbage, start terminal 1 again | It starts, logs `the paired devices could not be read: starting with none` and shows a notice; the garbage is now in `devices.toml.bad`. Terminal 2 keeps retrying and doesn't forget the PC: `pasteget --profile phone status` still lists it. Stop terminal 1, put the safe copy back as `devices.toml`, start terminal 1: terminal 2 connects again |
| 10 | Last: `pasteget forget phone` | `Unpaired phone.` Terminal 1 logs `unpaired`; terminal 2 logs `the device unpaired this one` and forgets the PC too (`pasteget --profile phone status` lists no device). Pair again (section 2) to go on |

Screenshot to clipboard:
- **Windows:** `Win+Shift+S`
- **KDE Plasma:** Spectacle → *Copy to clipboard*, or `Meta+Shift+Print`
- **Hyprland:** `grim -g "$(slurp)" - | wl-copy` (packages `grim`, `slurp`, `wl-clipboard`)

## 4. Report

For each environment (Windows 11, KDE Plasma Wayland, Hyprland), report which rows passed. For
failures, attach the output of terminals 1 and 2 and of `pasteget status`.

| Environment | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 |
|---|---|---|---|---|---|---|---|---|---|
| Windows 11 | | | | | | | | | |
| KDE Plasma 6 (Wayland) | | | | | | | | | |
| Hyprland | | | | | | | | | |

## 5. The phone (M3)

What the Android app does so far: pair by QR code, then send text or images **by hand** (share
sheet, Quick Settings tile, or the button in the app). Text sent from the PC lands on the phone
clipboard. Sections 6 to 8 add screenshots, the way back from the PC, and the automatic copy.

### Install the app

A release's `paste-get_<version>_foss.apk` (README, *Install*), or the debug APK the CI builds
on every push: on GitHub, **Actions** → latest **CI** run → **Artifacts** →
`paste-get-android-debug`. Unzip it, copy the `.apk` to the phone and open it (allow installing
from that source when asked). The two are signed with different keys: uninstall one before
installing the other.

With `adb` and USB debugging instead: `adb install -r app-foss-debug.apk`.

Then open the gear → **Keep paste-get running** and do each item, or HyperOS will stop the
connection in the background ([`devices/xiaomi-hyperos.md`](devices/xiaomi-hyperos.md), section 3,
says why).

### Checklist

The phone and the PC must be on the **same Wi-Fi**. On the PC, run `pasteget daemon`.

| # | Do this | Expected |
|---|---|---|
| P0 | Install paste-get fresh (or clear its data), PC: `pasteget pair`, then open it | A guide instead of the tabs: *Copy here. Paste there.*, the computer's side, **Scan the code** (P2's sheet; a check once paired), notifications (Android asks only once the page has said why), what to send (copies chosen), each chosen feature's set-up, *Keep paste-get running*, then **Copy this sentence**: "Arrived on <PC> in … ms", and `Ctrl+V` on the PC pastes it. With the PC's daemon stopped instead, after 5 s: "It didn't arrive" and what to check. **Skip** (any page) or **Finish** opens the tabs; the guide never shows again, nor on a phone that was paired before this version. Closed before that (swiped away from recent apps once paired), paste-get opens on the guide again |
| P1 | Open paste-get, allow notifications (P0 asked already on a fresh install) | A persistent notification appears: "No computer paired yet", then after P2 "Connected to <PC>" with a **Send clipboard** button |
| P2 | PC: `pasteget pair`. Phone: **Scan the code** on the home screen, scan it | Phone: a sheet "Pair with this computer?" with the PC's name and address. Tap **Pair**: a sheet with a progress bar, then "Paired with …" at the bottom of the screen for a few seconds, and the card at the top says "Connected to <PC>". PC: `✓ Paired with …` |
| P2b | Turn Wi-Fi off on the phone for a minute | The card says "Looking for <PC>…" with a moving bar; after 30 s it says what to check and offers **Try again**. Wi-Fi back on: "Connected to <PC>" again |
| P3 | In any app, select text → **Share** → **Send to PC**, then `Ctrl+V` on the PC | The text is pasted |
| P4 | Gallery → a screenshot → **Share** → **Send to PC**, then paste on the PC (Paint, GIMP, Discord) | The image is pasted |
| P5 | Add the **Send clipboard** tile to Quick Settings (on Android 13+: the gear → **Add the tile**). Copy text, pull down the shade, tap the tile | "Sent to 1 computer"; `Ctrl+V` on the PC pastes it |
| P6 | PC: `pasteget send "depuis le PC"`, then long-press a text field on the phone → Paste | `depuis le PC` is pasted |
| P7 | Lock the phone for 5 minutes, unlock, repeat P3 | Still works, without opening the app |
| P8 | Restart the phone, wait a minute, repeat P3 | Still works (the app restarts on boot) |
| P9 | Scan the PC's pairing QR code with the **normal camera app** (only if it offers to open the link; not every camera does) | paste-get opens and asks first, as in P2; it pairs only on **Pair**. Rotating the phone doesn't ask again |
| P10 | Quit paste-get on the PC, forget the computer on the phone, start the PC again (it still lists the phone: it wasn't told). PC: `pasteget pair`, scan the new code | Phone: "Paired with …", then "Connected to 1 computer". The PC lists the phone once |
| P11 | Turn the phone's Wi-Fi off (mobile data on), scan a pairing code | Within about 10 s, an error that lists each address of the code with its reason, the PC's Wi-Fi or Ethernet address first |
| P12 | Rename the phone (Settings → About phone → Device name), then force-stop paste-get and open it again | Within seconds `pasteget status` on the PC shows the new name, also after the PC restarts |
| P13 | Turn on dark mode, close paste-get (swipe it away in recents), open it again | The start screen and the app are dark from the first frame, with no white flash |
| P14 | Tap **History**, **Devices**, then the gear at the top right; press Back twice | History lists what was sent (P3–P5) with sizes and times. Devices lists the PC ("Connected · … ms") and *This phone* with its device ID in groups of four; tapping the PC shows its addresses. With the PC's daemon stopped and started again, pulling the Devices list down reconnects at once instead of after the backoff. The gear opens Settings; Back goes to Devices, then Home. Rotate the phone on any of them: the same screen stays |
| P15 | The gear → **Dark**, then **Light**, then **System**; turn **Wallpaper colors** off and on (Android 12+); **Language** → French (Android 13+); **Open-source licences** | The app changes at once; after closing and reopening it, the start screen matches the choice too (Android 12+). Without wallpaper colors, the app is indigo. In French, every screen is in French. The licences list the libraries and show the Apache License text |
| P16 | Quit the PC's daemon, pull down Quick Settings; start it again | The tile reads "Offline: sends when back", then the PC's name, lit. The notification says "Looking for <PC>…", then "Connected to <PC>" |
| P17 | Long-press paste-get's launcher icon: **Send**, then **Pair**, then **History**. Then close paste-get, open it with **Pair**, press Back until it closes, and reopen it from recent apps | Send sends the clipboard (as the tile does); Pair opens the scanner; History opens the History tab. Reopened from recent apps, paste-get opens without the scanner (Android 10 to 12 used to open it again) |
| P18 | Copy text, pull down the notification shade, tap **Send clipboard** in paste-get's notification | "Sent to 1 computer"; `Ctrl+V` on the PC pastes it |
| P19 | Turn the phone to landscape on Home, Devices and Settings (or open paste-get on a tablet) | A rail on the left instead of the bottom bar; each screen's column stays narrow and centred, switches next to their labels. Scrolled to the end, the last item clears the gesture bar and, on Devices, the Pair button |
| P20 | The gear → **Keep paste-get running**; tap **Allow** under *Unrestricted battery use* and accept | "Allow" turns into a check and the count goes up. On a Xiaomi phone: **Open Autostart** opens HyperOS's Autostart list (or App info); after turning paste-get on there, **It's done** ticks it; *Lock it in recent apps* is a tip. On other phones only battery and notifications are listed |
| P21 | Scan a PC's code and leave the sheet open. From the PC: `adb shell am start -a android.intent.action.VIEW -d "pasteget://pair?v=0&id=X&s=Y&a=203.0.113.7%3A47290&n=Other"` | The sheet keeps showing the PC you scanned, not "Other"; **Pair** pairs with that PC |
| P22 | In a password manager (Bitwarden, KeePassDX), copy a password, then tap **Send clipboard** in paste-get's notification. Then quit the PC's daemon, send it again, start the daemon | The PC pastes it, and it is not in Win+V history, Klipper or the tray's *Recent*. With the daemon stopped: "Not sent: no computer is connected", and nothing arrives when it's back |
| P23 | In **Devices**, open the PC's page. Go to Settings → Apps → paste-get → Permissions (or Notifications) and turn one off: Android stops paste-get. Come back to it from recent apps | The PC's page shows again, not the Devices list |
| P24 | PC: `pasteget pair`, then quit the daemon. Phone: scan the code, **Pair**. Then start the daemon, `pasteget pair` again, turn the phone's Wi-Fi off, scan, **Pair**; turn Wi-Fi back on, **Try again**, **Pair** | After a few seconds: "Pairing didn't work" and what to check (same Wi-Fi, paste-get open on the PC), in the phone's language, with **Try again**. **Details** shows the technical error. **Try again** asks "Pair with this computer?" again for the same code, and **Pair** then pairs |
| P25 | Last, with the phone connected: in the **Devices** tab, tap the PC, then **Forget this computer…**, then **Cancel**; tap it again, then **Forget** | Cancel keeps the PC. Forget removes it from the list, and the phone stops connecting to it. Within 2 s the PC's tray no longer lists the phone and shows "<phone> unpaired this computer"; `pasteget status` lists no device. Pair again to go on |

Phone logs never contain clipboard contents either. To get them: `adb logcat -s paste-get`.

## 6. Screenshots (M4)

Goal: take a screenshot on the phone, press `Ctrl+V` on the PC, and it's there, in under a
second. Update both sides first: `git pull` and `cargo build --release` on the PC, and the APK of
the latest CI run on the phone (it installs over the previous one; pairings are kept).

On the phone, turn on **Send new screenshots** and choose **Allow all** for photos. "Select
photos" hides new screenshots: the row then says "One step left", and its screen says why
and offers **Open settings**.

On Hyprland, notifications need a notification daemon (mako, dunst, swaync…). KDE Plasma and
Windows have one built in.

| # | Do this | Expected |
|---|---|---|
| S1 | Take a screenshot on the phone, then `Ctrl+V` in Discord (or Paint, GIMP) on the PC | The screenshot is pasted, upright. The PC shows "Image from POCO M8 5G ✓" |
| S2 | Look at the app's home screen, under **Recent** | "Screenshot, … kB", and under it "<PC> in … ms" |
| S2b | Settings → Apps → paste-get → Permissions → Photos → *Allow limited access*, back to paste-get | The Screenshots row says "One step left"; its screen checks *Only selected photos*, says why that hides new screenshots and offers **Open settings** |
| S3 | Share a photo from the gallery with **Send to PC**, paste on the PC | The photo is pasted the right way up, even if taken in portrait |
| S4 | Turn Wi-Fi off on the phone for a minute, then on again | "Connected to 1 computer" again within a few seconds |
| S5 | Restart the PC into the other OS (Windows ↔ Arch), start `pasteget daemon` there (pair it once if it isn't yet) | The phone connects to it within a few seconds, without a new QR code |
| S6 | Give the PC a new IP address (switch it from Wi-Fi to Ethernet, or the other way), keep the daemon running | The phone reconnects by itself, within about 90 s at worst |
| S7 | Lock the phone, wait 5 minutes, unlock it, take a screenshot | S1 still works |

### Measuring latency (target: under 1 s)

Two numbers, both on the phone:
- The home screen's Recent list (S2) counts from the moment Android announced the new screenshot
  file to the PC's acknowledgement. Repeat S1 five times and note the values.
- `adb logcat -s paste-get` shows each step: "screenshot of … bytes read in … ms", then
  "delivered … ms after sending, … ms after Android announced the screenshot".

On the PC, the daemon logs "clip received … ms" (the transfer) and "clip applied to the clipboard
… ms" (decoding the image and setting the clipboard). Add that last number to the phone's to get
the whole path. The time HyperOS takes to save the screenshot comes before all of these.

Report the five values per OS, and whether S1 felt instant.

### Settings

Notifications are set in `settings.toml`, in `%APPDATA%\paste-get\default\` on Windows and
`~/.config/paste-get/default/` on Linux: `notify = "images"` (the default), `"all"` (text too) or
`"off"`. `offer_to_open_links = false` turns off the notification for links (section 13).
The tray's *Settings…* window edits these, the name and the hotkeys, and applies them; after
editing the file by hand, restart the daemon.

## 7. From the PC to the phone (M6)

Whatever you copy on the PC lands on the phone's clipboard, ready to paste, with nothing to tap
on the phone. Update both sides first, as in section 6.

| # | Do this | Expected |
|---|---|---|
| C1 | PC: copy some text. Phone: long-press a text field → **Paste** | The text is pasted |
| C2 | PC: open a text file over 1 MB (a log, for example), select all, copy. Phone: paste it in a notes app | The whole text is pasted, and the paste-get notification is still there |
| C3 | Lock the phone for 5 minutes, unlock it, repeat C1 | Still works, without opening the app |
| C4 | PC: take a screenshot to the clipboard (as in section 3). Phone: in Telegram, WhatsApp or Google Messages, long-press the message field → **Paste** | The screenshot is attached, ready to send |
| C5 | PC: in a browser, right-click an image → **Copy image**. Phone: paste it as in C4 | The image is attached |
| C6 | If you use Gboard: after C4, open its clipboard (the clipboard icon above the keys) | The screenshot is there; tapping it inserts it in apps that take images (Discord, for example) |
| C7 | PC: copy a password from a password manager (KeePassXC on KDE Plasma or Hyprland; KeePass, KeePassXC, Bitwarden or 1Password on Windows). Phone: paste it | Pasted. The tray's *Recent* menu doesn't list it, and Android's clipboard preview hides it. `pasteget daemon` in a terminal logs `local copy` with `sensitive=true` |
| C8 | Windows: send text from the phone to the PC (section 8). In the tray's *Settings…*, rename this computer and **Save** (paste-get restarts). Then press `Ctrl+Alt+Shift+V` (section 11) | It goes out as an ordinary clip: the tray's *Recent*, emptied by the restart, lists it as from this computer, and the tray's log (T21) says `clipboard sent` with `sensitive=false` |
| C9 | Windows, with a clipboard manager running (Ditto, for example): repeat C7 five times | Each time `sensitive=true` |

`adb logcat -s paste-get` shows "clip received: …, … bytes" for each clip. Images, and texts over
256 KB (C2), go on the clipboard as a link to a file, since Android can't pass that much in one
go: the log then also says "clip saved as clip-….png for the clipboard".

Report which rows passed with the PC on Windows, KDE Plasma and Hyprland, and in which phone apps
C4 worked.

## 8. Copy on the phone, paste on the PC (M5)

Android lets only the app in front read the clipboard. paste-get reads it in two ways, shown on the
**Send what you copy** screen (tap its row on the home screen):

- **Mode A, Shizuku.** [Shizuku](https://shizuku.rikka.app/) runs a small part of paste-get as the
  shell user, which may read the clipboard in the background. Shizuku has to be started again
  after each restart of the phone (without root).
- **Mode B, the fallback**, while Shizuku isn't running. When the clipboard changes, Android writes
  a line in the system log; paste-get then shows an invisible window for a moment to read it.
  It needs two permissions that Shizuku grants once. Taking focus may close the keyboard.
  On Android 13 and later, Android also asks to allow log access each time paste-get starts.

Only text is sent automatically. Images and files: share them with **Send to PC**.

### Set up Shizuku

Follow [`devices/xiaomi-hyperos.md`](devices/xiaomi-hyperos.md), sections 1 and 2 (Developer
options, then Shizuku through Wireless debugging). Then in paste-get, turn on **Send what you
copy** and tap **Allow in Shizuku**.

No Shizuku? Mode B also works with two grants by hand, once: `adb shell pm grant
io.github.nexoryx.pasteget android.permission.READ_LOGS`, then allow **Display over other apps** for
paste-get in the phone's settings.

### Checklist

Watch the PC daemon's terminal, and on the phone `adb logcat -s paste-get` if you can.

| # | Do this | Expected |
|---|---|---|
| A1 | Turn on **Send what you copy**, allow paste-get in Shizuku | The row says "On, automatic"; its screen checks **Shizuku** under *How copies are read* |
| A2 | In any app (Chrome, Notes…), select text → **Copy**. On the PC, `Ctrl+V` | The text is pasted. The Copies screen says "Last copy (…) sent at …"; the home screen's Recent lists it |
| A3 | Copy the same text again | The PC logs nothing new (the phone logs "seen before, not sent") |
| A4 | Copy text **on the PC**, paste it on the phone, wait 5 s | The PC does not receive it back from the phone |
| A5 | During A2: does the phone show a notice like "Shell pasted from your clipboard"? | Report what it says (Android 12+ may show one; the Copies screen's *Open privacy settings* leads to "Show clipboard access", which turns it off) |
| A6 | Copy a password from a password manager (Bitwarden, KeePassDX…), paste it on the PC | Pasted, and absent from Windows' clipboard history (`Win+V`) or Klipper |
| A7 | Windows, if you have a second PC on the same Microsoft account with *Settings → System → Clipboard → Share across devices* on: copy text on the phone, then `Win+V` on the other PC | Not there (it is in this PC's own `Win+V` history) |
| B1 | With Shizuku running, tap **Prepare the backup method (paste-get restarts)** on the Copies screen | paste-get restarts. The Copies screen says "Ready for when Shizuku stops" |
| B2 | In Shizuku, tap **Stop**. Back in paste-get | Within a few seconds Android asks to allow access to the device logs: allow it. The row says "On, backup method" |
| B3 | Go to another app, copy text, paste on the PC | Pasted. Report whether anything flashed or the keyboard closed |
| B4 | Restart the phone, don't start Shizuku | A notification says "Copies are not sent". Tap it, allow log access, then repeat B3: it works |
| B5 | Start Shizuku again | The row goes back to "On, automatic" within a few seconds; B3 no longer takes focus |
| B6 | Lock the phone for 5 minutes, unlock it, repeat A2 (or B3 without Shizuku) | Still works |

The phone's log names the hidden Android methods it uses, in a line like "Shizuku: watching the
clipboard as uid 2000 with addPrimaryClipChangedListener(…), …". **Please include that line in
your report**: it shows which variant HyperOS has. If the Copies screen says "Shizuku can't watch the
clipboard on this phone: unknown signature …", report the whole message: the app only needs that
signature added.

## 9. The tray app (M7)

`pasteget-tray` (`pasteget-tray.exe` on Windows, next to `pasteget` in `target/release`) puts a
paste-get icon in the system tray and runs the daemon itself, so no terminal is needed. If a
daemon already runs for the profile (`pasteget daemon` in a terminal), it shows that one instead.
It takes `--profile` like `pasteget`.

Where the icon shows up:
- **Windows:** the notification area; it may hide under the `^` arrow at first. Drag it onto the
  taskbar to keep it visible.
- **KDE Plasma:** the system tray, possibly under the arrow.
- **Hyprland:** only in a bar with a tray, such as Waybar's `tray` module. Without one, the app
  runs but shows nothing.

Pairing needs no terminal: the first time `pasteget-tray` starts with nothing paired, it opens a
**Pair a phone** window with the QR code. Later, the menu's *Pair a phone…* opens it again. When
the phone has scanned it, the window says "Paired with …" and closes by itself. `pasteget pair`
still works too.

Stop any `pasteget daemon` first, then start `pasteget-tray` (from a terminal the first time, to
see its log). Keep the fake phone of section 2 running.

| # | Do this | Expected |
|---|---|---|
| T1 | Click (or right-click) the icon | A menu: "paste-get · <this PC>", "Syncing with 1 device", the paired devices with ● (connected) or ○, *Recent*, *Pause syncing*, *Settings…*, *About paste-get …*, *Quit* |
| T2 | Copy text on the PC, then open *Recent* | The text is first, "· this computer, just now" |
| T3 | `pasteget --profile phone send "from the phone"`, open *Recent* | It's first, with the fake phone's name |
| T4 | Copy something else, then click the T2 entry in *Recent*; `Ctrl+V` | The T2 text is pasted. The fake phone logs nothing (it had it already) |
| T5 | Tick *Pause syncing* | The icon turns grey and the menu says "Paused"; copies are no longer sent (row 8 of section 3) |
| T6 | `pasteget resume` in a terminal | Within 2 s, the icon is in colour again and *Pause syncing* is unticked |
| T7 | Stop the fake phone (Ctrl+C) | Within a few seconds its line turns ○ and the menu says "No device connected" |
| T8 | *Quit* | The icon disappears; `pasteget status` says the daemon is not running |
| T9 | Start `pasteget daemon` in a terminal, then `pasteget-tray` | The tray shows that daemon. *Quit* closes the tray only: the daemon keeps running |
| T10 | With a new profile (`pasteget-tray --profile fresh`), start the tray | The *Pair a phone* window opens by itself with a QR code and "A new code comes in 4:59", counting down. Under it, the Android app's download page as text; **Copy** copies it |
| T11 | Phone: **Pair a computer**, scan that code | The window shows ✓ "Paired with <phone>" and closes after a few seconds; the tray keeps running and lists the phone as ● |
| T12 | Menu → *Pair a phone…*, then close the window with its × | The tray keeps running (closing the window doesn't quit). The code shown still works until its time is up; no new one is made |
| T30 | Menu → *Pair a phone…*, and leave the window open for more than 5 minutes | When the count reaches 0:00, a new code replaces the old one by itself, without an error. The phone pairs with the new one; a photo of the old one no longer pairs |
| T37 | Menu → *Pair a phone…*, take a photo of the code, let it renew (T30), then scan the photo with the phone until the new code has had three wrong tries (the phone tries every address in the code: with two, twice is enough) | Until then, "A device tried an old or wrong code…". Then a fresh code replaces it at once, saying a device tried wrong codes too many times; the phone pairs with that one. `pasteget pair` in a terminal ends with "too many wrong attempts" instead |
| T31 | Turn Wi-Fi off and unplug Ethernet, then menu → *Pair a phone…*; connect again | "This computer isn't on a network…" and "Trying again in a few seconds…"; once connected, a code shows by itself |
| T32 | With the system (or `PASTEGET_LANG=fr`) in French, start the tray, open the menu and every window | Everything is in French, also "paste-get démarre…" while it starts and the pairing window's messages. `PASTEGET_LANG=en` turns all of it to English, windows included |
| T15 | Menu → *Settings…*, change the notifications to "Images and text", **Save** | "Saved."; text from the phone now shows a notification, without a restart |
| T16 | Menu → *Pair a phone…*, close the window. In *Settings…*, rename this computer, **Save**. Then the same with the pairing window left open | "Saved. paste-get restarted…", never "paste-get could not restart"; within seconds the phone reconnects, and its app shows the new name (bring the app back to the front if it was open). It still shows it after the phone restarts. The pairing window left open shows a new code, from the restarted paste-get |
| T17 | In *Settings…*, click a hotkey field, press Ctrl+Alt+K, **Save**, then press Ctrl+Alt+K | It does that action (KDE may ask to allow the shortcut again). Backspace in the field turns the hotkey off |
| T18 | In *Settings…*, give both hotkeys the same keys, **Save** | An error says so; nothing is saved |
| T19 | With the tray running, open paste-get again from the Start menu | A notification: "paste-get is already running: its icon is in the notification area…"; no second icon |
| T20 | Quit, then open paste-get from the Start menu (a phone is paired) | A notification says where its icon is. At login (*Start at login* on), no such notification |
| T21 | Open `%LOCALAPPDATA%\paste-get\logs\pasteget-tray.log` (Linux: `~/.local/share/paste-get/logs/`) | The tray's log, with "tray starting" for each start. Attach it to any report |
| T14 | Windows: get a notification (an image from the phone, or a hotkey's) | It says it comes from **paste-get**, with its icon, not "Windows PowerShell". After uninstalling, paste-get is gone from Settings → System → Notifications |
| T22 | `pasteget quit` while the tray runs its daemon | The icon disappears; `pasteget status` says the daemon is not running |
| T23 | Run an older paste-get (a build from before this row existed, or with a lower `version` in `Cargo.toml`), then start this one | The older icon disappears and this one's shows, without the "already running" notice; the log says "an older paste-get runs for this profile: asking it to quit". With the same version, T19 instead |
| T24 | Windows: with paste-get running (tray, and `pasteget daemon` in a terminal), run the installer of a newer version | No "paste-get is running" question: both are stopped before the files are copied. After *Finish*, one icon, and `pasteget status` shows the new version |
| T25 | Windows: tick *Start at login*, also in a second profile's tray (`pasteget-tray --profile work`), then uninstall paste-get (Settings → Apps) | Neither is in Task Manager → *Startup apps* any more (`reg query HKCU\Software\Microsoft\Windows\CurrentVersion\Run` lists no `paste-get` value). Updating with the installer (T24) keeps them there |
| T26 | Developers: build paste-get with another `PROTOCOL_VERSION` (`core/src/proto.rs`) for the phone or the PC, and let the phone connect | One notification on the PC: "<phone> runs another version of paste-get: update both", not repeated while the phone retries |
| T27 | With other profiles running (`pasteget-tray --profile work`, this version, and `pasteget --profile test daemon --memory-clipboard --port 47299` in a terminal), run an older paste-get for the default profile, then start this one | The older default one makes way as in T23. The `work` icon stays, and the `test` daemon keeps running |
| T28 | Developers: with two builds that both know `pasteget quit` (the older one with a lower `version` in `Cargo.toml`), run the older `pasteget daemon` in a terminal, then the older tray (it only shows that daemon), then start the newer tray | The terminal's daemon quits; the older tray starts a daemon of its own, which is asked to quit too, and the older icon disappears. One icon is left, the newer one's; the log says "the older paste-get quit", nothing was ended |
| T29 | Menu → *About paste-get 0.1.0* (the version of this build) | A small window with the version, the licence (GPL-3.0-or-later) and the source address. **Copy** puts the address on the clipboard; nothing opens a browser or goes online. In French: *À propos de paste-get* |
| T13 | Last: menu → the phone → *Forget this device* | The phone disappears from the menu within 2 s and can't reconnect. Right away (or, if it was not connected, when it next connects) the real phone shows "<PC> unpaired this phone" and drops the computer from its list. Pair again to go on |

| T35 | Windows: mark the Wi-Fi *Public* (Settings → Network & internet → Wi-Fi → the network → *Public network*), then start the tray | Once, a notification: "Windows treats the network “<name>” as Public: your phone can't reach paste-get…". *Settings…* has a *Network* part saying how to make it Private, and a `netsh advfirewall …` line with **Copy**. Mark it *Private* again and reopen *Settings…*: the part is gone |
| T36 | Windows, Wi-Fi *Private*: in Windows Security → Firewall → *Allow an app through firewall*, untick paste-get. Run the copied `netsh` line (from T35) in Terminal (Admin) | The phone connects again. The list shows a "paste-get" rule for *Private* only |
| T33 | Take paste-get's port first (`python3 -m http.server 47290`, or `pasteget --profile other daemon --memory-clipboard --port 47290` with another profile), then start the tray | A notification: "paste-get could not start: Port 47290 is taken: is another paste-get running?", once (not every 15 s while it retries); the menu says the same the whole time, never "start it with `pasteget daemon`". Free the port: within 15 s the tray runs |
| T34 | Linux: start the tray with a profile folder it can't create, from a terminal: `PASTEGET_HOME=/proc/nowhere pasteget-tray` | A notification that stays until closed: "paste-get could not start." with the error and the log's path (also printed in the terminal) |

If paste-get doesn't start at all, Windows shows a box saying why, Linux a notification that
stays until closed, and the log has the details.

### A Waybar module (Hyprland)

`pasteget status --json` prints one line of JSON, also when paste-get isn't running
(`"running": false`), so a status bar can poll it. Besides the facts (`name`, `paused`,
`devices`…), it has the four fields of a Waybar `custom` module: `text` (how many devices are
connected), `alt` and `class` (`stopped`, `paused`, `connected`, `away` or `unpaired`) and
`tooltip`. In `~/.config/waybar/config.jsonc`, add `"custom/paste-get"` to a `modules-…` list,
and:
```jsonc
"custom/paste-get": {
  "exec": "pasteget status --json",
  "return-type": "json",
  "interval": 5,
  "format": "{icon} {}",
  "format-icons": {
    "connected": "●", "away": "○", "paused": "⏸", "unpaired": "+", "stopped": "✕"
  },
  "on-click": "pasteget pause --toggle"
}
```
Style it in `style.css` with `#custom-paste-get.paused { opacity: 0.5; }` and the like. The
AppImage has no `pasteget` command: use the .deb, .rpm or AUR package for this.

| # | Do this | Expected |
|---|---|---|
| B1 | `pasteget status --json`, with paste-get running, then after `pasteget quit` | One line each: `"running":true` with the devices, then `{"alt":"stopped",…,"running":false,…}`, exit code 0 both times |
| B2 | Add the module above, restart Waybar (`killall -SIGUSR2 waybar`), pair or stop the phone, pause | The module shows ● 1, then ○ 0 when the phone is away, ⏸ when paused, ✕ when paste-get quits; the tooltip names the phone; a click pauses or resumes |

Report which rows passed on Windows 11, KDE Plasma and Hyprland, and anything that looks wrong
in the menu (cut-off text, missing ticks, a menu that closes by itself).

## 10. Starting at login (M7)

The tray's *Start at login* checkbox adds paste-get to the programs started when you log in:
- **Windows:** the `Run` key of your user, listed in Task Manager → *Startup apps* as
  "paste-get". Disabling it there wins over the checkbox, which then still shows ticked.
- **KDE Plasma:** `~/.config/autostart/io.github.nexoryx.pasteget.desktop`, listed in System
  Settings → *Autostart*.
- **Hyprland** doesn't read `~/.config/autostart`. Add an `exec-once` line to
  `~/.config/hypr/hyprland.conf` instead, and leave the checkbox off. The tray's *Settings…*
  shows it, with the real path (the AppImage's, when it runs from one), ready to copy:
  ```
  exec-once = /usr/bin/pasteget-tray --profile default --at-login
  ```

It starts the program the checkbox was ticked from, so tick it again after moving the program.

| # | Do this | Expected |
|---|---|---|
| L1 | Tick *Start at login* | Windows: "paste-get" appears in Task Manager → *Startup apps*. KDE: the file above exists and System Settings → *Autostart* lists paste-get |
| L2 | Log out and back in (or restart) | The tray icon comes back by itself; `pasteget status` answers |
| L3 | Untick it, log out and in | No paste-get icon; the entry is gone |
| L4 | Hyprland: copy the lines from *Settings…* (section 11, H6) into `hyprland.conf`, log out and in | The icon is in Waybar's tray, with no "paste-get is running" notice |

## 11. Hotkeys (M7)

Two global hotkeys work in every app, with `pasteget daemon` or the tray running:

| Hotkey | Does | Same as |
|---|---|---|
| `Ctrl+Alt+Shift+P` | Pause or resume syncing | `pasteget pause --toggle` |
| `Ctrl+Alt+Shift+V` | Send the clipboard now, even while paused | `pasteget send --clipboard` |

Each press shows a short notification ("Syncing paused", "Clipboard sent to 1 device"…). Pause
plus *send the clipboard* gives a private mode: nothing leaves the PC unless you press the key.

Change them in `settings.toml`, then restart paste-get. A letter, a digit or `F1`–`F24`, after
`Ctrl`, `Alt`, `Shift` and/or `Super`; an empty string turns a hotkey off:
```toml
[hotkeys]
toggle_pause = "Ctrl+Alt+Shift+P"
send_clipboard = ""
```
Why Shift: on Windows, `Ctrl+Alt` is also `AltGr`, which types `@`, `€` and others on many
keyboard layouts. A hotkey on `Ctrl+Alt+V` would swallow them.

How each desktop gets them:
- **Windows:** registered directly. If another app already uses one, the log says
  `hotkey not registered: another app uses it`: pick another in `settings.toml`.
- **KDE Plasma:** through the desktop portal's global shortcuts. The first time, Plasma may ask
  to allow them; afterwards they're in System Settings → *Shortcuts*, where they can be changed.
  The log says `hotkey registered … trigger=…` for each. *(verify: Plasma may need paste-get
  installed with a desktop file to list it by name.)* If the log says `hotkeys not available`
  instead, add them as custom shortcuts in System Settings → *Shortcuts* → *Add New* → *Command*,
  with the commands above.
- **Hyprland:** paste-get doesn't register anything. The tray's *Settings…* has a *Hyprland*
  part with the lines to add to `~/.config/hypr/hyprland.conf`, and **Copy**; the log prints
  them too. They use the real path of `pasteget`, and the hotkeys saved in *Settings…*:
  ```
  exec-once = /usr/bin/pasteget-tray --profile default --at-login
  bind = CTRL ALT SHIFT, P, exec, /usr/bin/pasteget pause --toggle
  bind = CTRL ALT SHIFT, V, exec, /usr/bin/pasteget send --clipboard
  ```
  The AppImage has no `pasteget` command: there the lines run the AppImage itself, which does
  the action and exits (`paste-get_0.1.0_amd64.AppImage --action toggle-pause`, or
  `--action send-clipboard`). Run `hyprctl reload` after editing the file.

| # | Do this | Expected |
|---|---|---|
| H1 | Start paste-get, look at its log | Windows / KDE: `hotkey registered` twice. Hyprland: the two `bind` lines |
| H2 | Press `Ctrl+Alt+Shift+P` (KDE: allow the shortcuts first if asked) | Notification "Syncing paused"; the tray icon turns grey |
| H3 | Copy some text, then press `Ctrl+Alt+Shift+V` | "Clipboard sent to 1 device"; the fake phone logs `clip received` |
| H4 | Press `Ctrl+Alt+Shift+P` again | "Syncing resumed" |
| H5 | On a layout with `AltGr` (Albanian, German, Polish…), type `@` in a text editor | It types normally |
| H6 | Hyprland: tray → *Settings…* | A *Hyprland* part with an `exec-once` line and two `bind` lines, with full paths (the AppImage's path when started from it). **Copy**, paste them into `hyprland.conf`, `hyprctl reload`: H2–H4 work. Change a hotkey, **Save**: the lines follow. Not shown on KDE Plasma or Windows |
| H7 | Hyprland, AppImage: in a terminal, `./paste-get_*.AppImage --action toggle-pause` twice | The tray's icon turns grey, then back; the command exits at once each time. `--action nonsense` says which actions exist |

## 12. Copy on the phone without Shizuku (built-in mode, prototype)

paste-get can do what Shizuku does by itself: it pairs with Android's **wireless debugging**
once, then starts its own helper as the shell user whenever needed, even after a restart of the
phone (on Wi-Fi). No other app, nothing to restart by hand. It's new and experimental: please
report what happens.

**Once, to set up** (the phone on Wi-Fi): in paste-get, turn on **Send what you copy**. The
set-up opens ("Set up the automatic copy", also from the Copies screen's **Start set-up**). Its
four steps tick themselves as you do them in Settings, and only the current one says what to do:
1. **Before you start**: Wi-Fi and paste-get's notifications (the code is typed into one), each
   with a button to fix it. On Xiaomi phones, a tip about HyperOS's notification style.
2. **Developer options**: Settings → About phone → tap *OS version* (Xiaomi) or *Build number*
   seven times. **Open About phone** goes there.
3. **Wireless debugging**: turn it on in Developer options and allow it on this network.
4. **Pairing code**: **Open Wireless debugging** shows the notification "Pair paste-get with
   wireless debugging" and opens Developer options. Tap Wireless debugging → **Pair device with
   pairing code**; without leaving that screen, type the 6 digits in paste-get's notification
   (**Enter code**). The set-up shows each step (finding the code screen, pairing, getting
   permission), then "All set". You can turn Wireless debugging off: paste-get switches it on for
   a moment when it needs it, then off again.

**Split screen instead of the notification:** put paste-get in split screen from recent apps,
then **Open Wireless debugging**: Settings opens in the other half, and six boxes in paste-get
take the code. This works with paste-get's notifications off too.

If Shizuku was set up, the built-in mode replaces it; you can uninstall Shizuku.

| # | Do this | Expected |
|---|---|---|
| W1 | Set up as above | "Paired devices" in Wireless debugging lists **paste-get**; the Copies screen checks **Built-in** |
| W2 | Copy text in any app, paste on the PC | Pasted. Android shows "Shell pasted from your clipboard", as with Shizuku |
| W3 | Turn Wireless debugging off by hand, copy again | Still works (the helper runs on its own) |
| W4 | Restart the phone, unlock it, wait a minute on Wi-Fi, copy text | Pasted on the PC, without opening paste-get. Wireless debugging is off again afterwards |
| W5 | Restart the phone with Wi-Fi off, open paste-get | The home screen's card says "Looking for <PC>…" (without Wi-Fi nothing reaches the PC, so that comes first); the Copies row says "One step left", and the Copies screen says the built-in mode is waiting for Wi-Fi, with **Open Wi-Fi settings**. Turn Wi-Fi on: the PC connects and copies start by themselves |
| W6 | In Wireless debugging → Paired devices, forget paste-get, then restart the phone | The row says "Not working: tap to fix"; the Copies screen says "wireless debugging no longer knows paste-get: pair again" |
| W7 | **Pair again** on the Copies screen | The Copies screen offers **Start set-up** again |
| W8 | With the built-in mode running, turn on **Hide "pasted from your clipboard"** on the Copies screen (it shows once a copy went out), then repeat W2 | Pasted, with no notice. Settings → Privacy → *Show clipboard access* is now off. Turning the switch off brings the notice back |
| W9 | Before setting up: Settings → Apps → paste-get → Notifications off. In paste-get, turn on **Send what you copy** | The set-up stays at step 1: *Notifications* is unticked, with **Allow notifications** (paste-get's notification settings). Turn them on, come back: step 1 ticks itself |
| W10 | Start the set-up with Developer options off, then do each step in Settings and come back | Each step ticks by itself when you're back, "Step 2 of 4", "Step 3 of 4"…; the bar fills |
| W11 | At step 4, type a wrong code in the notification | "Wrong or expired code…" with **Retry**; a new code from Settings works |
| W12 | Split screen as above: turn Wireless debugging on in the Settings half, then type the code in paste-get's boxes | Step 3 ticks without leaving Settings; the code is sent at the sixth digit and the set-up says "All set" |

If a step fails, the Copies screen shows why. **Please report that text** (and what **Details** shows), and, if you can, the log:
`adb logcat -s paste-get` (lines start with "built-in mode" or "shell helper").

## 13. Links (v0.2)

When the PC receives a text that is nothing but a web link, it shows "Link from <phone>:
<site>" instead of staying quiet. Clicking it (or its **Open** button) opens the link in the
default browser. Nothing opens without a click.

| # | Do this | Expected |
|---|---|---|
| L1 | On the phone, copy a link (Chrome: long-press the address bar → Copy) | Within a second, the PC shows "Link from <phone>: <site>". The link is on the PC's clipboard too |
| L2 | Click the notification | The link opens in the default browser |
| L3 | Copy a link again, click **Open** (Windows: in the toast; KDE: under the notification) | Opens too |
| L4 | Copy a sentence with a link inside it | No link notification (it's just text) |
| L5 | Set `offer_to_open_links = false`, restart, repeat L1 | No notification; the link is still on the clipboard |

Report whether the click works on Windows 11, KDE Plasma and your Hyprland notification daemon
(mako, dunst, swaync: some need a setting or a keybinding to invoke an action).

## 14. 2FA codes (v0.2)

On the phone, **Send 2FA codes** finds one-time codes in notifications (an SMS, a bank or login
app) and puts just the code on the computers' clipboard, as a secret: `Ctrl+V` pastes it. The PC
shows "Code from <app> (<phone>) ✓", never the code.

It needs notification access, and on Android 15 (the POCO) the right to see codes, which Android
otherwise hides from apps that read notifications. With the built-in mode (or Shizuku) running,
**Allow (one tap)** on the **Send 2FA codes** screen grants both. Without, it opens Settings →
Notification access, and
Android 15 keeps hiding the codes.

| # | Do this | Expected |
|---|---|---|
| C1 | With the built-in mode running, turn on **Send 2FA codes**, open its screen, tap **Allow (one tap)** | "Allowed"; both items under *Access* are checked (the second only on Android 15) and the row says "On" |
| C2 | Get a code by SMS (log in to a site that texts one, or have someone send "Your code is 482913") | Within a second or two, the PC shows "Code from Messages (<phone>) ✓"; `Ctrl+V` pastes 482913 |
| C3 | Open `Win+V` (or Klipper) | The code is not in the clipboard history; the tray's *Recent* doesn't list it |
| C4 | A code from an app's notification (bank, Google prompt, authenticator push) | Same as C2, with that app's name |
| C5 | An SMS with an order number or an amount and no "code" word | Nothing is sent |
| C6 | More than 5 minutes after C2, reply to that SMS, or get another message without a code in the same conversation | Nothing is sent: the old code doesn't go out again |
| C7 | Turn off **Send 2FA codes**, get another code | Nothing is sent |
| C8 | Before C1, without the built-in mode or Shizuku: turn on **Send 2FA codes** and open its screen | *Read notifications* is unchecked, with **Open notification access** and what to do if Android says the setting is restricted (**Open App info**). On Android 15, once allowed, *See the codes Android 15 hides* stays unchecked and the screen offers to set up the automatic copy |

If a code isn't found, report the shape of the message (with the digits changed) so the detector
can learn it.

## 15. Copies made while a device is off (v0.2)

A copy made while the other side is off or out of range waits, and arrives as soon as it
connects again. That includes a connection that died without closing (a PC asleep, a phone that
just left the Wi-Fi), which still looks alive for up to 90 s: a copy waits until the other side
confirms it got it. Only the last one waits (text or image), in memory only. Passwords and 2FA
codes never wait: a code would be stale by then. Nor does the copy made before one, which would
land over it.

| # | Do this | Expected |
|---|---|---|
| Q1 | Quit paste-get on the PC (tray → *Quit*). On the phone, copy a line | The phone says it went to 0 computers |
| Q2 | Start paste-get on the PC again | Within a few seconds the line is in the PC's clipboard; the log says "sending the clip it missed" |
| Q3 | PC off, copy two lines on the phone one after the other, look at Home's *Recent*, start the PC | *Recent* says "It will arrive when <PC> is back" under the second line only, "Not sent: no computer was connected" under the first. Only the second one arrives |
| Q4 | Phone's Wi-Fi off, and right away copy something on the PC (the tray may still show the phone connected), turn Wi-Fi back on | The copy is in the phone's clipboard once it reconnects, once |
| Q5 | PC off, copy a line on the phone, then a password from a password manager (or get a 2FA code), start the PC | *Recent* (and the 2FA screen's *Last code*) says "Not sent: no computer was connected" under the password or code. Nothing arrives, not even the line |
| Q6 | PC off, copy on the phone, force-stop paste-get on the phone, start both | Nothing arrives: the copy was only in memory |
| Q7 | Put the PC to sleep. Within a minute, copy a line on the phone (it may still say it went to 1 computer), then wake the PC | The line is in the PC's clipboard once the phone reconnects |
| Q8 | With both connected, copy a line on the PC; once it's on the phone, turn the phone's Wi-Fi off and on | The line is not sent a second time when the phone reconnects |
| Q9 | Turn the phone's Wi-Fi off. Right away copy a line on the PC, then a few seconds later another line on the phone, and turn Wi-Fi back on | Once the phone reconnects, both paste the phone's line: the PC's log says "clip received", the phone keeps its own |
| Q10 | Same as Q9, but copy on the phone first, then on the PC | Both paste the PC's line; the PC's log says "clip dropped: a newer copy from here is on its way to the device" |
| Q11 | Phone's Wi-Fi off, copy a password on the PC as in C7, turn Wi-Fi back on | Nothing arrives |
| Q12 | PC off. On the phone, copy a line, then **Send clipboard now** in the app (or the tile, or Share → Send to PC) | The button works while the PC is offline, and the phone says "It will arrive when <PC> is back". Start the PC: the line arrives |

## 16. GNOME (tier 2)

GNOME (the default desktop of Ubuntu and Fedora) is best effort: it lacks the two things paste-get
relies on elsewhere on Linux.

- **No tray area.** The icon needs the "AppIndicator and KStatusNotifierItem Support" extension.
  Ubuntu ships it on by default ("Ubuntu AppIndicators"); on Fedora and Arch, install
  `gnome-shell-extension-appindicator`, log out and back in, and turn it on in the *Extensions*
  app. Without it, paste-get runs with no icon, and about 15 s after starting says so in a
  notification ("paste-get is running, but its icon can't be shown"); the log says
  `no system tray (StatusNotifierItem host) on the session bus`.
- **No clipboard-control protocol.** Mutter, GNOME's compositor, has no data-control protocol,
  so on Wayland paste-get uses the X11 clipboard through XWayland (the log says
  `no Wayland clipboard change events`, section 2). What that sees of copies made in Wayland
  apps, and whether received clips paste in them, is what this section finds out *(verify)*.
  With XWayland turned off, paste-get has no clipboard at all. Every poll reads the clipboard,
  and password managers' marks aren't seen (Known limitations).
- **Hotkeys:** if the log says `hotkeys not available`, add them in *Settings* → *Keyboard* →
  *Custom Shortcuts*, with the commands of section 11.

| # | Do this | Expected |
|---|---|---|
| G1 | Without the extension, start `pasteget-tray` from a terminal | No icon. Within about 15 s, the notification above, and the log line |
| G2 | Turn the extension on, quit (`pkill pasteget-tray`) and start it again | The icon is in the top bar; no such notification |
| G3 | Rows 1–5 of section 3, copying and pasting in a Wayland app (GNOME Text Editor), then in an X11 app (`xterm`) | Report which rows pass in each |
| G4 | Section 9, T1–T8 | The menu opens from the top-bar icon |
| G5 | *Start at login*, log out and in | The icon comes back (`~/.config/autostart` works on GNOME) |

## Known limitations

- If the PC changes address while the phone (or a PC that joined it) is connected, the other
  side notices only when the old connection times out (up to 90 s), then finds it again by
  mDNS. Devices that can't receive mDNS (some guest or office networks block it) still need a
  new pairing.
- Only the last copy made while a device is away waits for it, in memory: it's lost if the
  sending side restarts first. Passwords and 2FA codes never wait, nor does the copy made
  before one.
- When the PC can't read its paired devices (`devices.toml`), devices it no longer knows are
  refused but not told they were unpaired, so they keep the PC, until paste-get restarts.
  After a restart without the file, they are told, and forget it.
- When both sides copied while apart, the newer copy wins on both. Copies made within a few
  milliseconds of each other (less than the network delay) can still end up crossed.
- The phone shows nothing of its own when a clip from the PC arrives (Android 13 and later may
  show their clipboard preview); its home screen's Recent list shows what came (kind, size and
  time, never the contents), in memory only.
- Only text copied on the phone is sent automatically, not images or files: share those.
- Mode B takes focus for a moment on each copy, which may close the keyboard. On Android 13+, it
  needs paste-get opened once (and log access allowed) after each restart of the phone.
- Changes on the PC are detected by polling every 0.5 s, so a copy can take that long to leave.
  Each poll is cheap on Windows (a sequence number) and on KDE Plasma and Hyprland (Wayland change
  events): the clipboard is only read after a change. On X11 and GNOME every poll reads it, images
  included (row 6 measures the cost).
- A password copied on the PC goes out as sensitive only if the password manager marks it, and
  the PC sees the mark only on Windows and on KDE Plasma and Hyprland (through Wayland's
  data-control protocol, which the change events of section 2 also use). On X11 and GNOME it goes
  out as an ordinary clip. A copy the PC can't check (another app holds the clipboard) goes out
  as sensitive.
- IPv4 only.
- The identity key is a file readable only by your user, not yet in the OS keystore.
- The tray's *Recent* list is in memory only, holds the last 10 clips, and forgets them when the
  daemon stops. Sensitive clips (passwords) never appear in it. Pausing doesn't survive a restart.
- Hotkeys are read from `settings.toml` at start: restart paste-get after changing them. On
  GNOME and other desktops without the global shortcuts portal, bind the commands yourself.
- Text and images only (PNG and JPEG on the wire). Copying a file in a file manager isn't synced.
