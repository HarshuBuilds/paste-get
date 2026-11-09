# paste-get

Copy on your phone, paste on your PC. No tap, no cloud.

paste-get keeps the clipboard of your Android phone and your computer in sync over your own
Wi-Fi. You copy a line on the phone, press `Ctrl+V` on the PC, and it's there. A screenshot
taken on the phone is in the PC's clipboard less than a second later.

It's in beta. It's used daily on a POCO M8 5G and a Samsung with Windows 11, and it runs on
Linux (KDE Plasma, Hyprland). Bug reports are welcome.

## What it does

- **Phone → PC, automatically.** Whatever you copy on the phone lands on the PC. No share
  sheet, no app to open. Android 10+ blocks background clipboard access; paste-get gets around it
  without root, through Android's own wireless debugging (or [Shizuku](https://shizuku.rikka.app/)
  if you already use it).
- **Screenshots.** Take one on the phone, paste it on the PC, full size.
- **PC → phone.** Text and images copied on the PC go to the phone's clipboard.
- **2FA codes.** A code that arrives by SMS or in an app's notification goes to the PC, ready to
  paste. Only the code is sent, never the message. Off by default.
- **Links.** Copy a link on the phone and the PC offers to open it.
- **PC ↔ PC.** Two computers can be paired too (`pasteget join`).

## Install

Downloads are on the [Releases](https://github.com/Nexoryx-business/paste-get/releases) page.

**Windows 10/11:** run `paste-get_<version>_x64-setup.exe`. Windows SmartScreen may warn that
the app is unknown, since it isn't code-signed yet: *More info → Run anyway*. paste-get then
lives in the notification area (under `^` in the taskbar).

**Linux:**
- AppImage: `chmod +x paste-get_<version>_amd64.AppImage` and run it.
- Arch: the [`paste-get-git`](packaging/aur/paste-get-git/PKGBUILD) PKGBUILD builds it from source.
- The tray icon needs a tray: KDE Plasma has one; on Hyprland, use a bar with a tray (Waybar's
  `tray` module).

**Android 10+:** install `paste-get_<version>_foss.apk`. If Google Play Protect blocks it, that's
because the 2FA feature reads notifications: Play Store → your profile picture → Play Protect →
⚙ → turn off *Scan apps with Play Protect*, install, then turn it back on.

## Getting started

1. Start paste-get on the PC. The first time, it shows a QR code.
2. On the phone, tap **Pair a computer** and scan it. Both must be on the same Wi-Fi.
3. To send copies automatically, turn on **Send what you copy**, then **Set up without
   Shizuku** and follow the steps (it needs Developer options, once).
4. Optional: **Send new screenshots**, **Send 2FA codes**.

On the PC, the tray menu has **Settings…** for the computer's name, notifications and hotkeys.

## Privacy

There is no server and no account. Devices talk directly over the local network with TLS 1.3,
each one pinned to the other's key at pairing. Clipboard contents are never logged. Passwords
that a password manager marks as secret, and 2FA codes, are sent as sensitive: they stay out of
clipboard history and the tray's *Recent* list, and never wait for a device that is away.
Details and known limits are in [`SECURITY.md`](SECURITY.md).

## Building from source

```sh
cargo build --release        # pasteget (daemon + CLI) and pasteget-tray
cd android && ./gradlew assembleFossDebug
```

The tray needs WebKitGTK and libayatana-appindicator on Linux. [`CONTRIBUTING.md`](CONTRIBUTING.md)
has the details, and [`docs/TESTING.md`](docs/TESTING.md) shows how to try everything with a
second, in-memory daemon instead of a phone.

## Docs

| | |
|---|---|
| [`docs/PROTOCOL.md`](docs/PROTOCOL.md) | Wire protocol: transport, pairing, messages |
| [`docs/ROADMAP.md`](docs/ROADMAP.md) | What's done and what's next |
| [`docs/TESTING.md`](docs/TESTING.md) | Testing on real devices |
| [`docs/RELEASING.md`](docs/RELEASING.md) | How the packages are built |
| [`docs/AUDIT.md`](docs/AUDIT.md) | How each feature works, and what Android allows |
| [`docs/REVIEW.md`](docs/REVIEW.md) | Security review guide |
| [`docs/devices/xiaomi-hyperos.md`](docs/devices/xiaomi-hyperos.md) | Setup notes for Xiaomi / POCO / Redmi |

## License

[GPL-3.0-or-later](LICENSE).
