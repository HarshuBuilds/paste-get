# Releasing paste-get

How the packages are built, and the steps of a release. CI does the building
(`.github/workflows/release.yml`); a person decides when to release.

## What gets built

| Package | Built by | Where it ends up |
|---|---|---|
| Windows installer, `paste-get_<version>_x64-setup.exe` | `packaging/build-desktop.sh nsis` on `windows-latest` | Installs for the current user (no admin prompt) in `%LOCALAPPDATA%\paste-get`, with a Start menu entry and an uninstaller. `pasteget.exe` is installed next to `pasteget-tray.exe` |
| Linux AppImage, `paste-get_<version>_amd64.AppImage` | `packaging/build-desktop.sh appimage,deb,rpm` on Ubuntu 22.04 | One file; runs on distributions at least as new as Ubuntu 22.04. Contains `pasteget` too, but only the tray is started from outside |
| Debian/Ubuntu package, `paste-get_<version>_amd64.deb` | the same run | `/usr/bin/pasteget` and `pasteget-tray`, a desktop entry, an icon and the AppStream metainfo. Depends on the system's WebKitGTK 4.1, GTK 3 and libayatana-appindicator3 (the tray icon). Ubuntu 22.04, Debian 12 or newer |
| Fedora/openSUSE package, `paste-get_<version>_x86_64.rpm` | the same run (Tauri names it `paste-get-<version>-1.x86_64.rpm`; the workflow renames it) | The same files. Requires the same libraries by their file names (`libwebkit2gtk-4.1.so.0()(64bit)` and so on), which works across RPM distributions |
| Android APK, `paste-get_<version>_foss.apk` | `./gradlew assembleFossRelease` on `ubuntu-latest` | Signed with the release key, for both phone ABIs (`arm64-v8a`, `x86_64`). Without the key (forks, before it's set up) it comes out as `…_foss-unsigned.apk`, which Android won't install; a tag refuses to release without it |
| Arch package `paste-get-git` | `packaging/aur/paste-get-git/PKGBUILD`, tested with `makepkg` in an Arch container | `/usr/bin/pasteget` and `pasteget-tray`, a desktop entry, an icon and the metainfo. Built from source on the user's machine |

Every push that changes the packaging builds them all, without releasing: they're in the run's
artifacts. A `v*` tag builds them and puts them, with a `SHA256SUMS` file, in a **draft**
GitHub release.

Before that, CI checks the Linux packages: `packaging/linux/check-packages.sh` lists what the
.deb and .rpm install and the libraries they ask for, then the .deb is installed with `apt` on
Debian 12 and the .rpm with `dnf` on the latest Fedora, and both programs must start
(`--version`). A dependency with the wrong name, or a library missing from the list, fails there.
On Fedora, the installed desktop entry and metainfo also go through `desktop-file-validate` and
`appstreamcli validate`.

The AppStream metainfo, `packaging/linux/io.github.nexoryx.pasteget.metainfo.xml`, is what KDE
Discover and GNOME Software show about paste-get: summary, description, links, and one
`<release>` per version. Every Linux package installs it in `/usr/share/metainfo/`. Check it
after editing with `appstreamcli validate --no-net` (package `appstream`). It has no
screenshots yet: add a `<screenshots>` section with image links once there are real ones.

The Windows installer installs WebView2 if the PC doesn't have it (Windows 11 always does): the
*Pair a phone* and *Settings* windows need it. It then downloads Microsoft's bootstrapper, silently;
nothing is downloaded when WebView2 is there.

### Building them yourself

```sh
cargo install tauri-cli --version =2.12.4 --locked
packaging/build-desktop.sh appimage,deb,rpm    # Linux; nsis on Windows, from Git Bash
packaging/linux/check-packages.sh target/release/bundle    # needs dpkg-deb and rpm
```

The script builds `pasteget`, copies it to `desktop/tray/binaries/` under the target triple (as
Tauri expects for a "sidecar" program), then bundles the tray with
`desktop/tray/tauri.bundle.conf.json` on top of `tauri.conf.json`. That file is kept apart
because declaring the sidecar makes `cargo build` look for it, and everyday builds don't have it.

## The AUR package

`paste-get-git` builds the latest commit of the default branch, so it needs no update per
release. The CI job builds it from the commit being tested and checks that `.SRCINFO` matches the
PKGBUILD. After changing the PKGBUILD, regenerate it on Arch:
`makepkg --printsrcinfo > .SRCINFO`.

Publishing it on the AUR is done once, by hand, by whoever maintains it there (it needs an AUR
account with an SSH key):
```sh
git clone ssh://aur@aur.archlinux.org/paste-get-git.git
cp packaging/aur/paste-get-git/{PKGBUILD,.SRCINFO} paste-get-git/
cd paste-get-git && git add PKGBUILD .SRCINFO && git commit -m "Initial upload" && git push
```
Until then, Arch users can build it from the repository:
`cd packaging/aur/paste-get-git && makepkg -si`.

A `paste-get` package built from release tarballs can follow once releases are regular.

## The Android release key

Phones only install an update signed with the same key as the installed app, so this key is
made **once** and kept for good. Losing it means users must uninstall to update; leaking it
lets someone else ship "updates". The committed `debug.keystore` is only for debug builds.

1. On a trusted computer, with a JDK:
   ```sh
   keytool -genkeypair -keystore paste-get-release.jks -alias paste-get \
     -keyalg RSA -keysize 4096 -validity 10000 -dname "CN=paste-get"
   ```
   It asks for a password; use a long random one, the same for the store and the key.
2. Back up `paste-get-release.jks` and the password somewhere safe and offline (a password
   manager plus an encrypted backup).
3. In the GitHub repository: *Settings* → *Secrets and variables* → *Actions*, add:

   | Secret | Value |
   |---|---|
   | `ANDROID_KEYSTORE_BASE64` | `base64 -w0 paste-get-release.jks` |
   | `ANDROID_KEYSTORE_PASSWORD` | the password |
   | `ANDROID_KEY_ALIAS` | `paste-get` |
   | `ANDROID_KEY_PASSWORD` | the password |
4. Delete the `.jks` file from that computer if it isn't the backup.

The Gradle build reads the key from `PASTEGET_KEYSTORE` and the three variables above, so a
local signed build works the same way. Publish the certificate's SHA-256 fingerprint
(`apksigner verify --print-certs paste-get_….apk`) in the release notes, so users of Obtainium
or IzzyOnDroid can check it.

## Desktop packages are not signed yet

- **Windows:** the installer and programs are unsigned. SmartScreen says "Windows protected your
  PC" (*More info* → *Run anyway*), and Smart App Control refuses to run them at all. The plan is
  [SignPath](https://signpath.org/)'s free signing for open-source projects, which needs an
  application from the maintainer once the repository is public.
- **Linux:** the AppImage, .deb and .rpm aren't signed; check them against `SHA256SUMS`.

## The README demo GIF

It shows the real thing, so it's recorded on the real devices, not mocked up. About 10 seconds:
take a screenshot on the phone, then `Ctrl+V` in a chat app on the PC, with the "Image from …"
notification in view.

1. Record the PC screen: Windows: `Win+Alt+R` (Game Bar) or ShareX; KDE: Spectacle →
   *Record screen*; Hyprland: `wf-recorder -g "$(slurp)"`. Hold the phone in view of a second
   camera, or show its screen with `scrcpy` next to the chat window.
2. Convert at 960 px wide, 12 fps, kept under 5 MB:
   ```sh
   ffmpeg -i demo.mp4 -vf "fps=12,scale=960:-1:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse" docs/demo.gif
   ```
3. Check no private messages, notifications or names are visible, then add it to the top of
   `README.md`: `![Screenshot on the phone, Ctrl+V on the PC](docs/demo.gif)`.

## Making a release

1. Everything for the version is merged on `main`, and CI is green.
2. The Android release key is set up (see above). Bump the version in `Cargo.toml` (`[workspace.package] version`) and in
   `android/app/build.gradle.kts` (`versionName`, and `versionCode` + 1). Add a
   `<release version="…" date="YYYY-MM-DD">` at the top of `<releases>` in
   `packaging/linux/io.github.nexoryx.pasteget.metainfo.xml` (a tag without one fails; for 0.1.0,
   one is there already: set its date). Commit.
3. Tag and push: `git tag v0.1.0 && git push origin v0.1.0`.
4. Wait for the *Release* workflow, then open the draft release on GitHub. Install each package
   on a real machine (Windows 11, Arch) and run `docs/TESTING.md`, sections 9–11.
5. The security review of `docs/REVIEW.md` is done for this version, or the release notes say it
   isn't.
6. Write the release notes (what works, what's known not to), then publish.
