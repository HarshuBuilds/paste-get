# Security review guide

Before release 0.1, someone other than its authors reviews the crypto and protocol code
(ROADMAP M8). This guide says where to look and what must hold, so a reviewer can do it in a
day or two. It doesn't replace reading the code.

**Status: not reviewed yet.** When a review is done, add it to the log at the end.

## Scope

About 3,000 lines of Rust in `core/src/`, plus the parts of the desktop and phone code that
make trust decisions. In order of risk:

| File | What it does | Lines |
|---|---|---|
| `core/src/transport.rs` | TLS 1.3 setup, custom certificate verifiers (pinning), ALPN, `UnverifiedConnection` | ~620 |
| `core/src/pairing.rs` | Invite (QR code) parsing, pairing window, secret check, `pair_with` / `accept_pairing` | ~540 |
| `core/src/identity.rs` | Ed25519 key, device ID = SHA-256 of the SPKI, certificate generation | ~200 |
| `core/src/session.rs` | Session per device, reconnects, who may stay connected | ~670 |
| `core/src/clip.rs` | Clip offer/chunks/ack: sizes, offsets, hash check, reassembly | ~510 |
| `core/src/framing.rs` | Length-prefixed frames, 1 MiB limit | ~190 |
| `core/src/echo.rs` | Echo-loop guard (not a security boundary, but it hashes untrusted data) | ~120 |
| `core/src/detect.rs` | Finds a link to open, or a one-time code, in untrusted text; sorts clips into shelves and spots secrets for the history | ~1,200, plus tests |
| `core/src/history/record.rs` | The one place that decides what the clipboard history keeps | ~380 |
| `core/src/history/retention.rs` | Deleting from the history, and keeping it small | ~360 |
| `desktop/src/daemon.rs` | `handle_incoming`: known device → session, unknown → pairing only while a window is open | part |
| `desktop/src/control.rs` | Local control channel: Unix socket in a 0700 directory, Windows named pipe | ~550 |
| `desktop/src/config.rs` | Key file permissions (0600), profile names (path traversal) | part |
| `ffi/src/lib.rs` | The same decisions on the phone side, through the core | part |
| `android/…/OneTimeCodes.kt` | Notification listener for 2FA codes: what it reads, what it sends | ~170 |
| `ffi/src/wireless_adb/` | ADB client for the phone's own wireless debugging: pairing (TLS 1.3 + BoringSSL SPAKE2 + AES-GCM), `exec:` over TLS | ~700 |
| `android/…/ShellHelper.kt`, `ShellBinderProvider.kt`, `HiddenProviders.kt`, `BuiltInShell.kt` | The shell helper the app starts through it, and how it hands its binder to the app | ~600 |

The protocol itself is specified in `docs/PROTOCOL.md`; the review should also say whether the
code matches it.

## What must hold

Each line is a claim to check. "→" points at where it's enforced.

### Trust and authentication
1. A client connecting to a device ID completes the handshake **only** if the server's key
   hashes to that ID. → `PinnedServerVerifier::verify_server_cert`
2. Both verifiers delegate the proof of key possession to rustls
   (`verify_tls13_signature`); a certificate alone, without the private key, is useless.
3. TLS 1.2 can't be negotiated. → `with_protocol_versions(&[&TLS13])`, `tls12_disabled`
4. An accepted connection can't send or receive application data before the daemon decided
   the peer is trusted. → `UnverifiedConnection::authorize`
5. The device ID is computed the same way everywhere (SPKI DER, not the whole certificate,
   which is regenerated on every start). → `DeviceId::of_certificate`
6. A peer that is no longer paired (forgotten on the phone or on the PC) can't keep or reopen a
   session; one being set up while it is forgotten is closed too. It is sent a `Refusal`
   (`NOT_PAIRED`) and nothing it sent is acted on; a client forgets the refusing device only
   after the TLS handshake proved that device's pinned key, so a refusal can't be forged.
   → `session.rs` (`Peers::disconnect`, `keep_connected`), `transport.rs`
   (`UnverifiedConnection::refuse`), `desktop/src/daemon.rs` (`Daemon::forget`, `connected`,
   `handle_incoming`), `ffi/src/lib.rs` (`forget`, `unpaired_by`)

### Pairing
7. An unknown device can do nothing but send one `PairRequest`, and only while a window is
   open. A paired device that sends a `PairRequest` instead of its `Hello` (pairing again) is
   checked against the same window. → `desktop/src/daemon.rs` `handle_incoming`,
   `PAIRING_REQUEST_TIMEOUT`, `pairing::read_opening`
8. The secret: 16 bytes from the system RNG (`ring::rand::SystemRandom`), compared in
   constant time, valid once, for 5 minutes, at most 3 wrong tries. → `PairingWindow::check`.
   It runs under the window's lock, which is never held across network I/O: attempts made at
   once can't get around these limits, and a client that connects and stays silent holds up
   no other attempt. → `Shared::answer_pairing`, `PairingRequest::answer`
9. The secret is zeroized after use and never logged or `Debug`-printed. → `PairingSecret`
10. The phone pins the device ID from the QR code before sending the secret, so a machine that
    doesn't own that key never sees the secret. → `pair_with`. It tries every address of the
    code at once and drops the rest after the first success, which can't drop another success:
    the window accepts the secret once. → `pair_with_any_address`
11. Invite parsing rejects what `PROTOCOL.md` §5.1 says to reject (length, version, duplicates,
    missing keys) and can't panic. → `PairingInvite::from_str`, fuzz target `pairing_invite`
12. Device names from peers are sanitized before display or storage, also when a paired
    device's `Hello` renames it. → `sanitize_device_name`, `KnownDevices::rename`,
    `ffi/src/lib.rs` `renamed`

### Untrusted input from a paired device
13. No allocation is sized by a peer beyond the limits: frame ≤ 1 MiB, chunk ≤ 256 KiB, clip ≤
    64 MiB. Offsets and lengths can't overflow or overlap. → `framing.rs`, `clip.rs`, fuzz
    targets `frame_decoder`, `clip_receiver`
14. A reassembled clip is checked against the offered SHA-256 before use.
15. Images are checked (dimensions ≤ 16,384 px, decoded size ≤ 512 MiB) before decoding.
    → `desktop/src/clipboard.rs`
16. A clip whose origin is this device is dropped (echo through another device). → `echo.rs`

### Local attack surface
17. Another user on the same machine can't talk to the daemon: socket in a 0700 profile
    directory (Unix); named pipe with the user name, `first_pipe_instance`, remote clients
    rejected (Windows). → `control.rs`
18. The identity key file is readable by the user only. Unix: created with mode 0600 (via a
    temporary file, then renamed). Windows: no ACL of its own; it relies on `%APPDATA%` being
    private to the user. → `config.rs` `write_private_file`
19. Profile names can't escape the configuration directory. → `Profile::open_in`
20. Clipboard contents never reach the logs. Grep for `tracing::` near `data`, `text`,
    `preview`.

### Phone
21. Clips written to the Android clipboard carry `EXTRA_IS_SENSITIVE` when sensitive and
    `EXTRA_IS_REMOTE_DEVICE`. Images and long texts go through a `FileProvider` URI that other
    apps can read only with a granted permission.
22. Only this app can call the Shizuku service, which runs as `shell`. Shizuku hands the
    service's binder to the app that bound it; the service doesn't check the caller's UID itself.
    Is that enough? → `ShellClipboardService`, `IShellClipboard.aidl`

### Built-in mode (wireless debugging, no Shizuku)
23. The ADB client only connects to loopback or this phone's own addresses. → `check_local`
24. Pairing: the password is the code plus 64 bytes exported from the TLS session (label
    `adb-label\0`), SPAKE2 is BoringSSL's with AOSP's names, the AES-128-GCM key comes from HKDF
    as in `aes_128_gcm.cpp`, nonces never repeat. Compare with AOSP `pairing_connection.cpp` and
    `pairing_auth.cpp`.
25. adbd's server certificate is not verified, by necessity (adbd makes a random key at every
    start; Google's client accepts any). Is the reasoning in the module's SECURITY note sound?
26. The handover: the helper calls the app's `ShellBinderProvider`, which requires
    `INTERACT_ACROSS_USERS_FULL`, accepts only UID 2000 or 0, and only a helper of this very
    build; the helper checks that the provider belongs to the app's package. (A local socket was
    tried first: SELinux doesn't let the shell connect to an app's socket.) → `ShellBinderProvider`,
    `ShellHelper`, `HiddenProviders`
27. The app's RSA key (`files/adb.key`) gives full adb access to the phone while wireless
    debugging is on. It is private to the app, never backed up (`allowBackup=false`), and the app
    switches wireless debugging off again after starting the helper. The `WRITE_SECURE_SETTINGS`
    it grants itself is used for `adb_wifi_enabled` and, only when the user turns the switch on,
    `clipboard_show_access_notifications`. → `BuiltInShell`, `AccessNotices`

### Smart features (v0.2)
28. A link from a paired device opens only when the user clicks its notification, and only
    `http`/`https` links that are the whole clip. The site shown is the one the browser will
    open (no userinfo, ASCII host). The URL reaches the browser without a shell or command line
    in between (`xdg-open` with one argument, `ShellExecuteW`). → `core/src/detect.rs` (fuzz
    target `detect`), `desktop/src/notify.rs`
29. 2FA codes: off by default; only the digits of a code leave the phone, as a sensitive
    `CLIP_KIND_ONE_TIME_CODE` clip that receivers treat as secret even without the flag, with a
    cleaned-up app name. The detector is bounded (4,096 characters, no superlinear blow-up) and
    can't panic. The shell grants are fixed commands for this app's own listener only.
    → `core/src/detect.rs`, `core/src/clip.rs`, `OneTimeCodes.kt`, `ShellCommands.codeAccessGrants`
30. The clipboard history writes nothing about a clip that is sensitive, a one-time code (or a
    copy of one seen in the last 15 minutes), or matched by `detect::looks_secret`, except a
    count per reason and day; `HistoryStore::record` is the only way in. Every byte kept is
    checked (texts over 1 MiB keep their first 4 KiB only). Errors keep SQLite's result code,
    never its message, which can quote a search. → `core/src/history/record.rs`,
    `core/src/detect.rs` (fuzz target `detect`), tests `secrets_are_kept_out_and_only_counted`
    and `logs_never_hold_contents`. A search is parsed in Rust and reaches FTS5 only as quoted
    phrases, with control characters (NUL included) read as spaces, so no query can fail or be
    read as FTS5 syntax. → `core/src/history/query.rs` (fuzz target `history_query`)
31. Deleting from the history leaves nothing of the clip: rows, body, thumbnail and search
    entries go in one transaction (`secure_delete`, FTS5 secure-delete), the original file
    after the commit, then the free pages and the WAL are emptied. The same goes for an expired
    item (a number that looks like a code): hidden when it expires, deleted at the next
    retention. Retention never deletes a pinned item. → `core/src/history/retention.rs`, tests `a_deleted_clip_leaves_nothing_behind`
    and `an_expired_code_is_hidden_then_leaves_nothing_behind`

## Known gaps (not bugs to report, but worth an opinion)

- The identity key is a file, not in the OS keystore (v0.2).
- No forward-secrecy concerns beyond TLS 1.3's own; no replay protection beyond TLS.
- mDNS announces the device name to the local network.

## How to run things

```sh
cargo test --workspace                       # includes two-daemon and phone-API end-to-end tests
cd fuzz && cargo +nightly fuzz run <target>  # frame_decoder, pairing_invite, clip_receiver, hotkey, adb_wire, detect, history_query
cargo deny check                             # dependency advisories and licenses
```

## Review log

| Date | Reviewer | Scope | Findings / outcome |
|---|---|---|---|
| | | | |
