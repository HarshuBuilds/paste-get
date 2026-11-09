# Changes

What changed in each version of paste-get, for people who use it. The version is the same on
the phone and on the computer: update both.

## 0.1.0 (2026-11-09)

The first release. paste-get keeps the clipboard of an Android phone and of your computers in
sync over your own Wi-Fi, with no account and no server.

**From the phone to the PC**
- What you copy on the phone is on the PC a moment later, without opening the app. Android 10
  and later normally forbid that; paste-get does it without root, through Android's own wireless
  debugging (set up once, step by step in the app), or through Shizuku if you already use it.
- New screenshots go to the PC by themselves, full size.
- 2FA codes from text messages and app notifications can go to the PC clipboard, ready to paste.
  Only the code is sent, never the message. Off until you turn it on.
- A link copied on the phone shows a notification on the PC that opens it when clicked.
- Share text or an image from another app with **Send to PC**, or use the Quick Settings
  tile, the notification's **Send clipboard** button or the launcher shortcuts.

**From the PC to the phone**
- Text and images copied on the PC go to the phone's clipboard, long texts included.
- Two computers can be paired with each other too (`pasteget join`).

**On the computer**
- Windows 11 and Linux (KDE Plasma, Hyprland; GNOME as best effort). An icon in the tray with
  your devices, the last clips, *Pause syncing* and *Settings…* (the computer's name,
  notifications, hotkeys, starting at login).
- The *Pair a phone* window opens by itself the first time: no terminal needed.
- Hotkeys to pause syncing and to send the clipboard now (`Ctrl+Alt+Shift+P` and
  `Ctrl+Alt+Shift+V`).
- Installers: a Windows installer, an AppImage, .deb and .rpm packages, and AUR PKGBUILDs for
  Arch. Installing a new version replaces the running one.
- The `pasteget` command does everything from a terminal: `status`, `pair`, `send`, `pause`,
  `recent`, `forget`, `quit`, and `hotkey` for desktops that bind keys to commands (Hyprland).

**On the phone**
- A guide on the first start ends with a real test copy: "Arrived on <your PC> in … ms".
- Home shows what paste-get is doing and the next step if something is missing; *History* lists
  what was sent and received (kind, size and time, never the contents); *Devices* has a page for
  each computer.
- *Keep paste-get running* checks battery and notification settings, with the extra steps
  Xiaomi phones need.
- English and French.

**Privacy and security**
- Devices talk directly to each other with TLS 1.3, each pinned to the other's key when they
  paired. The phone asks before pairing, even from a link.
- Passwords that a password manager marks as secret, and 2FA codes, are sent as sensitive: they
  stay out of clipboard histories and the tray's *Recent* list, and never wait for a device that
  is away.
- Nothing is logged of what you copy. No telemetry.
- Unpair a device from either side: the other side is told and forgets it too.

**Known limits**
- Only text copied on the phone is sent automatically; images go by sharing them. Files can't be
  sent yet.
- The Windows installer isn't code-signed yet: SmartScreen warns once.
- Google Play Protect may block the APK because of the 2FA feature (the README says what to do).
- Nothing is kept on disk: there is no clipboard history yet, and a copy waiting for a device
  that is away is lost if paste-get restarts.
- See [`docs/TESTING.md`](docs/TESTING.md#known-limitations) for the rest.
