# Roadmap

Each step is sized for one working session: it builds, has tests, and ends in a commit.
Work top to bottom. The audit (`AUDIT.md`) explains the why.

## v0.1 — "it just works"

Goal: take a screenshot on the POCO, press `Ctrl+V` on Windows or Arch, and it's there.

### M0 — Foundations
- [x] Choose a license: GPL-3.0-or-later
- [x] Cargo workspace (`core`, `desktop`), `rustfmt`, `clippy`, `cargo-deny` config
- [x] GitHub Actions: fmt + clippy + tests on Linux and Windows, `cargo-deny`, Dependabot
- [x] `CONTRIBUTING.md`, issue templates

### M1 — Protocol and core (`core/`, `proto/`)
- [x] `proto/` messages: `Hello`, `ClipOffer`, `ClipChunk`, `Ack`, `Ping`, `Pong`
- [x] Length-prefixed framing + decoder fuzz target
- [x] Device identity (keypair) + mutual TLS with pinned keys
- [x] Pairing payload (what the QR encodes) + one-time secret
- [x] Echo-loop guard (origin + hash)
- [x] `docs/PROTOCOL.md` v0
- [x] Clip transfer (offer, chunks, ack) and `Hello` exchange
- **Done when:** two test peers in `cargo test` pair and exchange text and a PNG.
  ✅ `core/tests/end_to_end.rs`

### M2 — Desktop daemon, headless (`desktop/`)
- [x] `pasteget daemon`: watch and set the clipboard (text + image) through `arboard`
- [ ] Verified on Windows 11, KDE Plasma 6 Wayland, Hyprland
- [x] mDNS advertising
- [x] `pasteget pair` prints a QR code in the terminal (and `pasteget join` for PC-to-PC)
- [x] `pasteget send <text|file>`, `pasteget status`
- **Done when:** two daemons (different profiles on one machine) sync clipboards.

### M3 — Android skeleton (`android/`)
- [x] Session engine moved into `core` so phone and desktop share it
- [x] `ffi/`: phone API through UniFFI, tested against a real desktop daemon
- [x] Compose app, `foss` flavor, Rust core through UniFFI + `cargo-ndk`
- [x] `connectedDevice` foreground service with status notification
- [x] Pairing by scanning the QR code (in-app scanner or system camera)
- [x] Manual sending: share sheet + Quick Settings tile
- [x] Verified on the POCO M8 5G (and a Samsung phone), with Windows 11, November 2026
- **Done when:** sharing text from the POCO puts it on the PC clipboard.
  ✅ in `ffi/tests/phone_and_pc.rs` against a real daemon; pending on the real phone.

### M4 — Screenshots
- [x] Phone finds its computers again by mDNS when their address changes
- [x] MediaStore observer, screenshot folder detection, `IS_PENDING` handling
- [x] Send the image (JPEG as it is, other formats as PNG); the PC sets it as an image and shows a "Received ✓" notification
- [x] Measure latency (target < 1 s on LAN): shown in the app and logged on both sides; a 2 MB
  image takes 13 ms on loopback (`ffi/tests/phone_and_pc.rs`)
- [ ] Verified on the POCO M8 5G with Windows and Arch, latency measured on Wi-Fi. Windows ✓ (POCO and
  Samsung, November 2026); Arch and the Wi-Fi latency figure still to do
  (`TESTING.md`, section 6)
- **Done when:** screenshot on the POCO → `Ctrl+V` in Discord works on Windows and Arch.
  ✅ up to the clipboard in automated tests; pending on the real devices.

### M5 — Automatic clipboard on Android
- [x] Mode A: Shizuku user service listening as shell, with reflection fallbacks
- [x] One-time grants through Shizuku (`READ_LOGS`, overlay)
- [x] Mode B: `READ_LOGS` + overlay fallback
- [x] Health screen (current mode, Shizuku state, last sync): the home screen's status card and
  the *Send what you copy* screen (how copies are read, next step, details)
- [x] Verified on the POCO M8 5G (`TESTING.md`, section 8), with Windows 11, November 2026.
  HyperOS shows Android's "Shell pasted from your clipboard" notice on each copy. Which
  `IClipboard` signature it has is still to be read from the log
- [ ] Built-in mode, without Shizuku: the app pairs itself with wireless debugging and starts
  its own shell helper, also after a reboot (prototype, `TESTING.md` section 12). Set up and
  running on the POCO M8 5G, November 2026 (W1); still to check: copies (W2), wireless debugging
  off (W3) and restarts (W4–W6)
- [x] Opt-in switch to hide Android's "Shell pasted from your clipboard" notice (Android's own
  *Show clipboard access* setting, for every app), when the built-in mode gave the app
  `WRITE_SECURE_SETTINGS`; otherwise a button to the privacy settings (`TESTING.md`, W8)
- **Done when:** copy on the phone → paste on the PC, no tap, even after the POCO reboots (mode B).
  ✅ up to the phone API in `ffi/tests/phone_and_pc.rs`, and the app's logic in Robolectric tests;
  pending on the real phone. On Android 13+, mode B needs one tap after a reboot: Android lets an
  app read the system log only after asking the user, while the app is in front. Android 10–12
  start it at boot.

### M6 — PC → phone
- [x] Text into the Android clipboard. Long texts go as a link to a file, like images: clips pass
  through Binder, whose buffer is 1 MB
- [x] Images through `FileProvider`
- [ ] Verified on the POCO M8 5G with Windows and Arch (`TESTING.md`, section 7). Windows ✓ (POCO and
  Samsung, November 2026); Arch still to do
- **Done when:** a PC screenshot pastes into a phone app.
  ✅ up to the phone API in `ffi/tests/phone_and_pc.rs`; pending on the real devices.

### M7 — Tray app
- [x] Tauri 2 tray: status, paired devices, pause, last items (`pasteget-tray`, runs the daemon
  in-process unless one already runs; `pasteget pause`, `resume` and `recent` on the command line)
- [x] Autostart on Windows and Linux: the tray's *Start at login* (`Run` key, XDG autostart;
  `exec-once` on Hyprland, which ignores XDG autostart)
- [x] Hotkeys: `RegisterHotKey` (Windows), GlobalShortcuts portal (KDE), `bind … exec pasteget …` (Hyprland).
  Pause/resume and "send the clipboard now" (`pasteget send --clipboard`), set in `settings.toml`
- [x] Linux: when no tray can show the icon (GNOME without the AppIndicator extension, a bar
  without a tray), the tray says so once per start in a notification and in its log
  (`desktop/tray/src/linux_tray.rs`). GNOME stays tier 2 (`TESTING.md`, section 16)
- [ ] Verified on Windows 11, KDE Plasma 6 Wayland and Hyprland (`TESTING.md`, sections 9–11)

### M8 — Release 0.1
- [x] CI builds: Windows installer, AppImage, AUR `PKGBUILD`, signed APK (all four green in
  the Release workflow; the APK is signed once the release key is in the secrets)
  - [x] Windows installer (NSIS, per user) and AppImage, with `pasteget` inside: `release.yml`,
    a draft release for each `v*` tag (`RELEASING.md`)
  - [x] AUR `paste-get-git` PKGBUILD, built, installed and started with `makepkg` in an Arch
    container by CI (publishing it on the AUR needs a maintainer account)
  - [x] AUR `paste-get-bin` PKGBUILD from the release's .deb, built and installed by CI the same
    way; `update.sh` points it at each new release. Publishing both: `RELEASING.md`, steps 1–11
  - [x] `.deb` and `.rpm`, built with the AppImage. The Release workflow checks their files and
    dependencies, then installs them on Debian 12 and the latest Fedora (checked locally; its
    first CI run is still to see)
  - [x] AppStream metainfo (`packaging/linux/io.github.nexoryx.pasteget.metainfo.xml`) in every
    Linux package, for KDE Discover and GNOME Software; validated by CI
  - [x] A firewalld service (TCP 47290, UDP 5353) in the .deb, .rpm and `paste-get-git`, and a
    README section for when the phone doesn't find the PC (Wi-Fi, Windows and Linux firewalls)
  - [x] Release APK, signed in CI with a key kept in repository secrets (creating the key and
    the secrets is a one-time step for the maintainer, `RELEASING.md`)
- [x] `SECURITY.md` draft: threat model, known gaps, private reporting
- [ ] README demo GIF: needs the real devices; how to record it is in `RELEASING.md`
- [ ] Human review of crypto and protocol code: `docs/REVIEW.md` lists the files and the
  31 properties to check
- [x] Pairing from the tray: a *Pair a phone* window with the QR code, opened by itself on the
  first start, so no terminal is needed
- [x] PC side can revoke a paired phone: `pasteget forget <name>`, or the tray menu → the phone
  → *Forget this device*; its session ends at once. The phone is told (`Unpaired` in the
  session, or `Refusal`, `NOT_PAIRED`, when it next dials), forgets the PC too and shows "<PC>
  unpaired this phone". Forgetting a connected PC on the phone tells the PC the same way
  (`TESTING.md`, P25)
- [x] The phone asks before pairing: a scanned code or a `pasteget://` link (which any web page
  or message can open) shows the name and addresses it gives and pairs only on *Pair*. Pairing
  errors are explained in the phone's language, with the technical message under *Details*
- [x] The phone's screens, redrawn: Home (a status card with one next step, what paste-get
  sends, the last clips with their latency), History (what was sent and received since the app
  started: kind, size, time, never contents, until the history store exists), Devices (each
  computer's page, this phone's ID, pull to refresh), a screen per feature, Settings
  (appearance, language, the tile, licences); the notification and the tile name the computers;
  launcher shortcuts; 640 dp columns on wide screens (`TESTING.md`, P14–P19)
- [x] The built-in mode's set-up, step by step: Wi-Fi, notifications, Developer options and
  Wireless debugging checked live (also in split screen), Xiaomi's own words, the pairing's
  progress mirrored from the notification, a code field in split screen, and each failure
  explained with **Retry** (`TESTING.md`, W9–W12)
- [x] *Keep paste-get running*, in Settings: battery optimisation (Android's own "always run in
  the background?" question) and notifications checked live; on Xiaomi phones, Autostart and
  Battery saver (behind `VendorSettings`, with App info as the fallback), ticked by hand since
  no API reads them, and the recent-apps lock as a tip (`TESTING.md`, P20)
- [x] A first-run guide, on phones with no pairing: what paste-get is, the computer's side,
  pairing, notifications (asked on their own page), what to send and each choice's set-up,
  keeping it running, then a test sentence copied for real and "Arrived on <PC> in … ms", or
  what to check after 5 s (`TESTING.md`, P0)
- [x] *Settings…* window in the tray: this computer's name, notifications, links, hotkeys and
  starting at login, checked before saving. Notifications and links apply at once
  (`pasteget` control request `reload_settings`); a new name or hotkeys restart the daemon the
  tray runs (`TESTING.md`, T15–T18)
- [x] Windows notifications come from "paste-get" with its icon, not "Windows PowerShell": its
  AppUserModelID is registered for the user, and removed by the uninstaller
- [x] Received clips never go to the Windows cloud clipboard (Microsoft account sync); they stay
  in the local `Win+V` history unless sensitive
- [x] A PC that joined another finds it again by mDNS after its address changed, like the phone
  does, and remembers the address that worked (`TESTING.md`, section 3, row 12)
- [x] A device that speaks another protocol version gets a notification on the PC, once:
  "<device> runs another version of paste-get: update both" (`TESTING.md`, T26)
- [x] Updates replace the running version: the installer stops paste-get first, and a newer
  tray asks an older running one to quit (control request `quit`, `pasteget quit`), or ends
  its processes if it's too old to know it: only those of its profile, the tray and `pasteget
  daemon` (`TESTING.md`, T22–T25, T27, T28). The uninstaller also removes *Start at login*,
  for every profile

## v0.2 — "smart"
History + search, 2FA codes, PC-side OCR, sensitive filters, auto-crop, smart actions.
- [x] A link copied on another device: the PC shows "Link from <phone>: <site>", which opens it
  when clicked (`TESTING.md`, section 13; `offer_to_open_links` in `settings.toml`)
- [x] 2FA codes: a notification listener on the phone finds one-time codes (`core/src/detect.rs`)
  and sends only the code, as a secret, to the PC clipboard (`ClipKind.ONE_TIME_CODE` in the
  protocol). Notification access and Android 15's hidden codes are granted in one tap through
  the built-in mode or Shizuku (`TESTING.md`, section 14)
- [x] A copy made while a paired device is away waits for it and is sent when it connects
  again: the last one only, in memory, never a sensitive clip (`TESTING.md`, section 15). Also
  when the connection died without closing: a copy waits until the device acknowledges it.
  Copies made on both sides meanwhile cross on reconnection, and both keep the newer one (each
  offer says how old its clip is, `ClipOffer.age_ms`)
- [x] Wayland clipboard change events: on KDE Plasma 6 and Hyprland the PC reads the clipboard
  only after the compositor reports a change (ext-data-control or wlr-data-control), not at
  every 0.5 s poll. X11 and GNOME still read at every poll (`TESTING.md`, section 2)
- [x] Passwords copied on the PC go out sensitive when the password manager marks them:
  `x-kde-passwordManagerHint` on Wayland (KeePassXC, KDE), the formats KeePass, KeePassXC,
  Bitwarden and 1Password set on Windows. Out of the phone's clipboard preview, the tray's
  *Recent* list, and never kept for a device that is away (`TESTING.md`, C7 and Q11)
- [ ] Clipboard history, on the phone first, then on each PC. Off by default; passwords, codes,
  keys and card numbers never reach the disk. The store is in the core, so both sides share it:
  - [x] Shelves and secrets: `detect::category` (link, email, phone number, colour, code,
    JSON) and `detect::looks_secret` (private keys, `otpauth://`, API tokens, card numbers,
    generated passwords), with `detect::bare_code` for numbers kept a short while only
  - [x] Store skeleton (`core/src/history`, feature `history`): SQLite with FTS5 trigram, built
    from source (cross-compiles with `cargo-ndk`; about 1.9 MB per ABI)
  - [x] Recording text: dedupe by SHA-256, secrets refused and counted (only numbers reach the
    disk), a 4 KiB head and a deflated body, texts over 1 MiB kept "too large", numbers that
    look like codes kept 15 minutes
  - [x] Pages and search: any part of a word, accents ignored both ways (folding checked
    against SQLite's own), words under 3 characters, `site:`/`from:`/`type:`/`is:pinned`/
    `after:`/`before:`, filters for the chips, keyset pages, UTF-16 highlights. 20,000 items:
    texts of about 250 bytes, 32 ms p95 on a PC for a word found in nearly every item, 15 ms or
    less otherwise, and 0.6 ms to record a clip (`search_speed_at_20000_items`); still to
    measure on the POCO, where the target is 30 ms
  - [x] Images: originals as files (written whole or not at all, swept at open), thumbnails
    made by the app and checked (JPEG, PNG or WebP, 64 KiB at most), phone screenshots as
    MediaStore references that follow gallery deletions
  - [x] Retention, delete and stats: expiry and age, originals after 7 days or past their
    budget, the item count, then images before texts when the database is full; pins never go.
    A deleted or expired clip is gone from the database, its WAL and the index (`next_expiry`
    tells the app when to run retention); space goes back to the disk. Measured on 1,000
    texts: the trigram index is 3.2 times the text, 1.4 KB per item
  - [ ] Phone: recording through `ffi`, then the Android screens
  - [ ] PC: history in the daemon, `pasteget history`, the tray's history window
Desktop identity key in the OS keystore when one is available (M2 stores it in a user-only file,
like SSH keys, because Hyprland setups often run no Secret Service).

## v0.3 — "deep"
Insert from phone (camera), links both ways, files, multiple PCs, GNOME extension.

## v1.0 — "platform"
Local API + hooks, browser extension, relay / USB fallback, F-Droid, signed releases, website and final name.
