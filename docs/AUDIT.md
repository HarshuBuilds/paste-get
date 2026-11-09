# Technical audit

How every planned feature can be built, what's hard, and what isn't possible.
Written before any code, October 2026. Claims marked *(verify)* still need testing on a real device.

Legend: ✅ straightforward · 🟡 doable, with traps · 🔴 limited by Android or the OS

## TL;DR

- **Almost everything is feasible.** About 70% is ordinary development, 20% needs Shizuku,
  10% is limited by Android or the desktop OS.
- **The three real obstacles:**
  1. reading the Android clipboard in the background;
  2. GNOME on Wayland (no clipboard-manager protocol);
  3. Android vendors (Xiaomi included) killing background apps.
- **Not possible as originally imagined:**
  - "Copy a file in an Android file manager, paste it in Explorer": file managers almost never
    use the system clipboard. Files go through the share sheet instead.
  - "Continue on PC the page open on the phone" can't be automatic: Android doesn't expose
    another app's current URL. Share sheet again (2 taps).

## Target platforms

| Tier | Platform | Notes |
|---|---|---|
| 1 | Windows 11 | Clean clipboard APIs |
| 1 | Arch Linux + KDE Plasma 6 (Wayland) | KWin supports data-control: background clipboard access works |
| 1 | Arch Linux + Hyprland | wlroots data-control: works. Hotkeys can be plain `bind = …, exec, pasteget …` |
| 1 | Android 15 / HyperOS 2 (POCO M8 5G) | Aggressive background killing, extra Shizuku toggle. See `devices/xiaomi-hyperos.md` |
| 2 | macOS | No clipboard change event: poll every 250–500 ms |
| 2 | GNOME (Wayland) | Needs a GNOME Shell extension (like GSConnect) |
| 2 | Other Android vendors | Test matrix later |

**Dual boot:** Windows and Arch are two separate devices for paste-get, each with its own key and
pairing. Only one is online at a time; the phone just sends to whichever answers.

## 1. Architecture

```
┌─────────── Android ────────────┐          ┌──────────── PC ─────────────┐
│ Kotlin + Jetpack Compose UI    │          │ Tauri tray UI (later)       │
│ Foreground service             │          │ Rust daemon + CLI           │
│  ├ Watchers: clipboard,        │   LAN    │  ├ Clipboard per OS         │
│  │  screenshots, notifications │◄────────►│  ├ Hotkeys, native OCR      │
│  ├ Shizuku service (as shell)  │ mTLS     │  ├ Local API                │
│  └ Rust core (UniFFI)          │          │  └ Rust core (same crate)   │
└────────────────────────────────┘          └─────────────────────────────┘
      core = protocol + crypto + pairing + history + tests
```

| Piece | Choice | Why | Rejected |
|---|---|---|---|
| Shared core | **Rust**, used from Android through UniFFI | One implementation of protocol and crypto, testable and fuzzable. Delta Chat ships a Rust core on F-Droid | Two implementations (Kotlin + Rust): twice the protocol bugs |
| Android | Kotlin + Compose + a Shizuku user service | System and hidden APIs are Java/Kotlin only | Flutter: everything important would still be native |
| Desktop | **Headless Rust daemon + CLI first**, Tauri 2 tray UI on top | The daemon is testable without a UI; Tauri gives tray, global shortcuts, autostart, updater in ~10 MB | Electron: 150 MB for a tray app |
| Wire format | Protobuf (`prost`) | Clear, versioned spec; binary-friendly for images | JSON: bad for binary data |

## 2. Reading the Android clipboard in the background

Since Android 10, only the focused app and the default keyboard can read the clipboard.

| Mode | How | Pros | Cons |
|---|---|---|---|
| **A. Shizuku service as `shell`** | A Shizuku user service runs with the shell identity and listens to the clipboard through the hidden `IClipboard` interface. Same technique as scrcpy, which syncs the device clipboard automatically | Event-driven, clean | Hidden API: signatures change across Android versions and vendors ([scrcpy #4703](https://github.com/Genymobile/scrcpy/issues/4703)), so several reflection fallbacks are needed. Shizuku dies on reboot (no root). A "Shell pasted from your clipboard" toast may appear on Android 12+ *(verify)* |
| **B. `READ_LOGS` + invisible overlay** | Watch logcat for the "denied clipboard access" message, then briefly focus a transparent overlay to read the clip ([ClipCascade](https://github.com/Sathvik-Rao/ClipCascade)) | Grants **survive reboots** | Hacky: steals focus, may close the keyboard. Android 13+ asks the user for log access each time the app starts, and only while the app is in front: one tap after a reboot |
| **C. Our own keyboard (IME)** | The default keyboard may read the clipboard | Official, never breaks | Huge project; user must switch keyboards |
| **D. Manual** | Quick Settings tile, share sheet, text selection menu | Works everywhere | Not automatic |

**Decision:** automatic fallback **A → B → D**, shown on a health screen ("Mode: Shizuku ✓").

**Update (November 2026): mode A without Shizuku.** Shizuku itself only gets the shell identity
through Android's wireless debugging. The app now does the same on its own ("built-in mode"):
it pairs its RSA key once with wireless debugging (the 6-digit code from Settings), grants itself
`WRITE_SECURE_SETTINGS`, and from then on switches wireless debugging on for a moment whenever it
needs to start its shell helper (also after a reboot, on Wi-Fi), then off again. No other app to
install, nothing to restart by hand, and no dependency on Shizuku, whose official build fails on
Android 17. The ADB client is in `ffi/src/wireless_adb` (rustls, ring, BoringSSL's SPAKE2), the
helper in `ShellHelper.kt`. Order is now built-in → A (Shizuku) → B → D. *(verify on HyperOS)*

**Key idea:** also use Shizuku as a **one-time permission granter**: `READ_LOGS`,
`SYSTEM_ALERT_WINDOW`, `RECEIVE_SENSITIVE_NOTIFICATIONS`, usage stats. These grants persist after
a reboot, so when Shizuku is down the app drops to mode B instead of stopping, and offers a
"restart Shizuku" shortcut.

## 3. Keeping the Android service alive

- **Foreground service type `connectedDevice`, not `dataSync`.** On Android 15, `dataSync`
  is capped at **6 hours per 24 hours**
  ([docs](https://developer.android.com/develop/background-work/services/fgs/timeout)).
- **Start on boot:** Android 15 forbids some types (including `dataSync`) from `BOOT_COMPLETED`;
  `connectedDevice` should be allowed *(verify)*.
- **Xiaomi/HyperOS:** Autostart permission, "No restrictions" battery mode, lock in recents.
  The app should detect and guide the user (see `devices/xiaomi-hyperos.md`).
- **Network:**
  - hold a `MulticastLock` for mDNS;
  - reconnect on network changes;
  - newer Android versions are adding a local network permission *(verify)*.

## 4. Screenshots → PC

- **Standard approach:** `ContentObserver` on MediaStore images, filter the `Screenshots` folder
  (path varies by vendor; Xiaomi uses `DCIM/Screenshots` *(verify)*), wait for `IS_PENDING = 0`.
- **Android 14+ trap:** with "selected photos only" access, detection sees nothing. Request full
  access and explain why.
- **Shizuku alternative:** watch the folder directly as shell, no photo permission needed.
- Android 14's screenshot callback only works for one's own activity, so it's useless here.

## 5. Desktop clipboard per OS

| | Watch changes | Images | Mark as sensitive | Global hotkeys | Type text | Verdict |
|---|---|---|---|---|---|---|
| **Windows** | `AddClipboardFormatListener` | PNG + DIB | Exclusion formats so Windows history and Cloud Clipboard skip it | ✅ `RegisterHotKey` | ✅ `SendInput` | ✅ |
| **KDE Plasma 6 (Wayland)** | ✅ data-control | ✅ | `x-kde-passwordManagerHint` | GlobalShortcuts portal | 🟡 RemoteDesktop portal | ✅ |
| **Hyprland** | ✅ wlr-data-control | ✅ | idem | Config `bind` → CLI | 🟡 portal / `wtype` | ✅ |
| **X11** | ✅ (daemon owns the selection) | ✅ | idem | ✅ | ✅ | ✅ |
| **macOS** | Poll `changeCount` | ✅ | `org.nspasteboard.ConcealedType` | ✅ | 🟡 Accessibility permission | ✅ |
| **GNOME (Wayland)** | ❌ Mutter has no data-control ([source](https://docs.rs/lamco-data-control)) | ❌ | | 🟡 | 🟡 | 🔴 → extension |

Library: `arboard` (with the `wayland-data-control` feature), wrapped behind our own trait so
OS-specific extras (sensitive markers, file lists, HTML) can be added per platform.

## 6. Network, pairing, security

- **Discovery:** mDNS `_pasteget._tcp`.
  - Windows 10+ has a native DNS-SD API, so no Bonjour needed.
  - Manual IP as fallback, which also makes Tailscale work for free.
- **Transport:** TCP + TLS 1.3, mutual authentication, keys pinned at pairing (rustls).
  - Windows shows a firewall prompt on first run; Arch may need a port opened if a firewall is enabled.
- **Pairing:** the PC shows a QR code (device id, key fingerprint, IPs, port, one-time secret); the
  phone scans it. Fallback: 6-digit code with SPAKE2. Devices can be revoked from either side.
- **Transfers:** the sender offers metadata first (hash, MIME types, size), then streams resumable chunks.
- **No echo loops:** every clip carries origin device + content hash. Clips we just applied are
  ignored by the local watcher. On Android 13+, clips we write carry `EXTRA_IS_REMOTE_DEVICE`.
- **History:** SQLite + FTS5 in the core, kept on each device (section 10). Version 1 relies on
  the OS's disk encryption; encrypting it with a key in the OS keystore (Android Keystore, Windows
  DPAPI, Secret Service on Linux, Keychain on macOS) comes later, with the identity key.
- **Away from home:**
  - user-hosted encrypted relay;
  - evaluate *iroh* for NAT traversal;
  - **USB via `adb reverse`**, since Shizuku users already have USB debugging enabled.

### Threat model (to expand in `SECURITY.md`)

| Threat | Mitigation |
|---|---|
| Attacker on the same Wi-Fi | Pinned keys, no unauthenticated endpoints |
| Stolen phone | Revoke from the PC; the history is protected by Android's file-based encryption, not by paste-get (section 10) |
| Other local app on the PC | Local API on a user-only socket/named pipe + token |
| Passwords leaking through sync | Sensitive flag, app denylist, pattern detection, auto-expiry |
| Supply chain | `cargo-deny`, pinned dependencies, signed releases, reproducible builds |

## 7. Feature by feature

| Feature | How | Difficulty | Trap |
|---|---|---|---|
| Screenshot → PC clipboard | Section 4 | ✅ | Full photo access required |
| Text both ways | Section 2. Writing the Android clipboard from the background is allowed | 🟡 | Background reading |
| Images PC → phone | Share via `FileProvider` URI, not raw bytes | 🟡 | 1 MB Binder limit |
| Skip sensitive copies | `EXTRA_IS_SENSITIVE`, foreground app (usage stats via Shizuku), patterns (`detect::looks_secret`: private keys, tokens, Luhn card numbers). IBANs are not secret (they are printed on invoices) | 🟡 | Many apps don't set the flag |
| 2FA codes | Notification listener + code extraction | 🟡 | **Android 15 hides OTPs** from untrusted listeners. Fixed through the shell (built-in mode or Shizuku): `appops set <pkg> RECEIVE_SENSITIVE_NOTIFICATIONS allow`, done in one tap (v0.2) ([source](https://androidauthority.com/android-15-two-factor-authentication-codes-3492585)) |
| OCR | **On the PC**, with the OS engine (Windows.Media.Ocr, Tesseract on Linux, Vision on macOS) | ✅ | Not ML Kit on the phone: proprietary, breaks F-Droid |
| Blur sensitive data | v1 on the PC (OCR returns word positions). Later on the phone (ONNX models) | 🔴 | Hardest image feature |
| Crop status/nav bars | Inset sizes + orientation | 🟡 | Full-screen apps have no bars: make it optional |
| Strip GPS, convert HEIC | `ExifInterface`, `ImageDecoder` | ✅ | |
| Clean paste (links, plain text, JSON) | Desktop only | ✅ | |
| Smart actions | Links, colors, addresses ✅. "Call from phone" 🟡 | 🟡 | Background activity starts blocked → `am start` via Shizuku |
| History, search, pins | SQLite FTS5 in the core, one history per device (section 10). Syncing histories later | ✅ | |
| Expiring clips, private mode | Timer + Quick Settings tile | ✅ | |
| Rich text (HTML) | Per-OS format | 🟡 | Windows "HTML Format" header |
| Files phone → PC | Share sheet; pasteable as real files on the PC | ✅ | ❌ from Android file managers |
| Send from PC | Explorer "Send to", Dolphin service menu, macOS Services | ✅ | ❌ drag onto a Windows tray icon is unsupported |
| Links PC → phone | Browser extension (MV3) + native messaging | 🟡 | Registration per browser and OS |
| Links phone → PC | Share sheet | ✅ | ❌ not automatic |
| "Insert from phone" (camera) | PC hotkey → phone opens camera via Shizuku `am start` | 🟡 | Locked phone → notification. FOSS document scanner (OpenCV) later |
| QR → PC | zxing-cpp | ✅ | |
| Type on phone with PC keyboard | Key injection via Shizuku; Unicode via clipboard + `KEYCODE_PASTE` | 🟡 | `input text` has no Unicode. Xiaomi needs "USB debugging (Security settings)" |
| Dictation phone → PC cursor | Speech recognition on the phone, typing on the PC | 🟡 / 🔴 Wayland | Mic needs an action on the phone; can't be triggered from the PC |
| Send to the active PC | Each PC reports idle time | 🟡 | Different API per Wayland desktop |
| Wi-Fi rules | Network name | 🟡 | **Reading the SSID needs location** on Android 10+ |
| Ring phone, battery level | | ✅ | |
| CLI, local API, launchers | Token-protected local socket / named pipe | ✅ | |
| Plugins / hooks | v1: "run a command" only | 🟡 | No remote code execution |

## 8. Distribution

- **Android:** GitHub Releases + IzzyOnDroid + Obtainium first, then F-Droid (`foss` / `full` flavors).
  Play Store is risky (`READ_LOGS`, Shizuku, foreground service justification): later.
- **Windows:** unsigned builds trigger SmartScreen. SignPath offers free signing for open source.
- **Linux:** AppImage, AUR package, .deb/.rpm. Avoid Flatpak at first (sandbox vs clipboard access).
- **macOS:** notarization needs a paid Apple developer account. Homebrew cask in the meantime.

## 9. Main risks

| Risk | Impact | Mitigation |
|---|---|---|
| Hidden clipboard API changes (Android version or vendor) | High | Reflection fallbacks, A → B → D modes, device matrix |
| HyperOS kills the app | High (it's the main test phone) | `connectedDevice` service, setup guide, health screen |
| Scope creep | High | Strict roadmap |
| Trust (the app sees everything you copy) | High | Open source, local-only, sensitive filters, visible activity log |
| GNOME Wayland | Medium (not tier 1) | Shell extension later |
| Signing costs | Low | SignPath; Apple later |

## 10. Clipboard history

Decided in November 2026 after comparing three designs (storage-first, organization-first,
experience-first); built on the storage-first one. Phone first, then each PC.

| Question | Decision | Why |
|---|---|---|
| Where | `core::history` (Cargo feature `history`): rusqlite with SQLite built from its C source (3.54, FTS5), on the phone and the PC | Same schema, rules and tests everywhere. Android's own SQLite is as old as 3.22 (Android 10), and Room has FTS3/4 only |
| What an item is | The SHA-256 of the bytes (= `ClipOffer.sha256`), one row per unique content; copying it again moves it to the top | Already on the wire and checked on receipt |
| Search | FTS5 `trigram` tokenizer with `remove_diacritics 1`, on the first 4 KiB of each text. Words of 1–2 characters are matched in Rust-folded text instead | Any part of a word, accents ignored: "verif" finds "vérification", "hub" finds github. Costs about 3× the indexed text (3.2× measured on 1,000 texts, `size_per_item_and_trigram_ratio`); above 3.5×, switch to `unicode61` with prefix queries |
| Index layout | Contentless (`content=''`) with FTS5 `secure-delete`; a delete hands FTS5 the indexed text back (the `'delete'` command), so `text_head` never changes | The text isn't stored twice, and a deleted row's trigrams leave the index's pages at once. `contentless_delete=1` only marks a row deleted and leaves its trigrams in the file, from which the text can be rebuilt |
| Secrets | Refused before anything is written, and only counted, by reason: the sensitive flag, one-time codes, `detect::looks_secret` (keys, tokens, cards, generated passwords). A number of 4–8 digits alone is kept 15 minutes | Passwords and codes never reach the disk; a postcode copied by mistake costs nothing |
| Images | Received images: the original file for 7 days or until a 150 MB budget (500 MB on a PC) is full, then a thumbnail made by the app (≤ 64 KiB, in the database). Phone screenshots: a MediaStore reference, no copy | About 40 MB for a heavy user over 3 months; copying every screenshot would be about 3.4 GB a year |
| Retention | Off by default. Unpinned items kept 3 months, at most 20,000; a 200 MB guard on the database. Order: expired and old items, originals older than 7 days, the oldest originals past the budget (halved when the disk has under 1 GiB), the oldest items past the count, then images before texts while the database is too big. Pins are never evicted, and an original stays until its thumbnail is made | A history is a honeypot: keep less |
| At rest | **The OS's encryption only, in version 1**: Android's file-based encryption, BitLocker or LUKS on the PC. SQLCipher with a keystore key later, with the identity key | Encrypting the history alone while `identity.key` is a plain file buys little, and with a key that needs the phone unlocked, clips arriving while it is locked couldn't be written. **On a PC without BitLocker or LUKS, the history is readable from the disk** |
| Delete | `secure_delete`, FTS5 secure-delete, the WAL truncated after a delete or an expiry (checked: if another connection keeps it, retried at the next retention), files removed after the commit. Expired items are hidden at once; `next_expiry` tells the app when to run retention, which otherwise runs after each clip | A deleted or expired clip is gone from the database, its WAL and its index (tested). Items dropped for their age or the limits leave the WAL at the next checkpoint. Flash wear-levelling may still keep old blocks. One connection per database, so nothing keeps the WAL |
| AI | No model in the app | The smallest useful ones are 400–500 MB, would have to be downloaded (no network but your devices), and the built-in ones are proprietary. Plain rules (`detect::category`) sort links, code, colours, emails and phone numbers |
| OCR | Not in version 1. On the PC as in section 7; on the phone only after a measured spike | About 12 MB of models in the APK, and battery |

**Cost:** SQLite adds about 1.9 MB to the native library per ABI (arm64, stripped, measured with
the bundled build's default options), so about 3.7 MB for the APK's two ABIs. Its C source is
compiled by the `cc` crate, so `cargo-ndk` cross-compiles it like `ring`.

## Sources

- [scrcpy clipboard docs](https://github.com/Genymobile/scrcpy/blob/master/doc/control.md), [scrcpy #4703](https://github.com/Genymobile/scrcpy/issues/4703)
- [ClipCascade](https://github.com/Sathvik-Rao/ClipCascade), [Sefirah](https://github.com/shrimqy/Sefirah-Android)
- [Android FGS timeouts](https://developer.android.com/develop/background-work/services/fgs/timeout), [Secure clipboard handling](https://developer.android.com/privacy-and-security/risks/secure-clipboard-handling)
- [Android 15 hides 2FA codes](https://androidauthority.com/android-15-two-factor-authentication-codes-3492585)
- [lamco-data-control (compositor support)](https://docs.rs/lamco-data-control)
- [Shizuku setup guide](https://shizuku.rikka.app/guide/setup/)
- [KDE Connect clipboard discussion](https://discuss.kde.org/t/is-it-possible-to-send-text-or-links-from-to-android-windows-without-enabling-permanent-clipboard-sync/33799)
