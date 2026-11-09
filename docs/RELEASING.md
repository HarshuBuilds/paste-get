# Releasing paste-get

How the packages are built, and the steps of a release. CI does the building
(`.github/workflows/release.yml`); a person decides when to release.

## What gets built

| Package | Built by | Where it ends up |
|---|---|---|
| Windows installer, `paste-get_<version>_x64-setup.exe` | `packaging/build-desktop.sh nsis` on `windows-2025` | Installs for the current user (no admin prompt) in `%LOCALAPPDATA%\paste-get`, with a Start menu entry and an uninstaller. `pasteget.exe` is installed next to `pasteget-tray.exe`. Installing and uninstalling first stop both if they run (`desktop/tray/installer-hooks.nsh`); uninstalling also removes *Start at login*, for every profile |
| Linux AppImage, `paste-get_<version>_amd64.AppImage` | `packaging/build-desktop.sh appimage,deb,rpm` on Ubuntu 22.04 | One file; runs on distributions at least as new as Ubuntu 22.04. Contains `pasteget` too, but only the tray is started from outside |
| Debian/Ubuntu package, `paste-get_<version>_amd64.deb` | the same run | `/usr/bin/pasteget` and `pasteget-tray`, a desktop entry, an icon, the AppStream metainfo and a firewalld service (`packaging/linux/firewalld/paste-get.xml`: TCP 47290, UDP 5353; not turned on by the package). Depends on the system's WebKitGTK 4.1, GTK 3 and libayatana-appindicator3 (the tray icon). Ubuntu 22.04, Debian 12 or newer |
| Fedora/openSUSE package, `paste-get_<version>_x86_64.rpm` | the same run (Tauri names it `paste-get-<version>-1.x86_64.rpm`; the workflow renames it) | The same files. Requires the same libraries by their file names (`libwebkit2gtk-4.1.so.0()(64bit)` and so on), which works across RPM distributions |
| Android APK, `paste-get_<version>_foss.apk` | `./gradlew assembleFossRelease` on `ubuntu-24.04` | Signed with the release key, for both phone ABIs (`arm64-v8a`, `x86_64`). Without the key (forks, before it's set up) it comes out as `…_foss-unsigned.apk`, which Android won't install; a tag refuses to release without it |
| Arch package `paste-get-bin` | `packaging/aur/paste-get-bin/PKGBUILD`, tested with `makepkg` in an Arch container on the .deb above | The .deb's files, unpacked. Points at a published release's .deb, so it's updated after each release (see below) |
| Arch package `paste-get-git` | `packaging/aur/paste-get-git/PKGBUILD`, tested with `makepkg` in an Arch container | `/usr/bin/pasteget` and `pasteget-tray`, a desktop entry, an icon and the metainfo. Built from source on the user's machine |

Every push that changes the packaging builds them all, without releasing: they're in the run's
artifacts. A `v*` tag builds the packages and puts them, with a `SHA256SUMS` file, in a **draft**
GitHub release.

The workflows name their runner images (`ubuntu-24.04`, `windows-2025`, and `ubuntu-22.04` for
the Linux packages, whose glibc sets the oldest system they run on) rather than `…-latest`, so
a release isn't built on a different system than the last test run without anyone deciding
it. Move them on purpose, in a commit of their own, before GitHub retires one. The actions they
use run on Node.js 24 (`actions/checkout@v7`, `upload-artifact@v7`, `download-artifact@v8`,
`setup-java@v6`, `gradle/actions/setup-gradle@v6` with its open-source `basic` cache,
`actions-rust-lang/setup-rust-toolchain@v2`); Dependabot proposes new major versions weekly.

Every run also checks the Linux packages, and a tag's release waits for that:
`packaging/linux/check-packages.sh` lists what the .deb and .rpm install and the libraries they
ask for, then the .deb is installed with `apt` on Debian 12 and the .rpm with `dnf` on the
latest Fedora, and both programs must start (`--version`). A dependency with the wrong name, or a library missing from the list, fails there.
On Fedora, the installed desktop entry, metainfo and firewalld service also go through
`desktop-file-validate`, `appstreamcli validate` and `firewall-offline-cmd --info-service`.

The AppStream metainfo, `packaging/linux/io.github.nexoryx.pasteget.metainfo.xml`, is what KDE
Discover and GNOME Software show about paste-get: summary, description, links, and one
`<release>` per version. Every Linux package installs it in `/usr/share/metainfo/`. Check it
after editing with `appstreamcli validate --no-net` (package `appstream`), version 1.0.3 or
newer like CI's Fedora: older ones miss checks, such as the one refusing a translated `<ul>`
(translate each `<li>` instead). It has no screenshots yet: add a `<screenshots>` section with
image links once there are real ones.

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

## The AUR packages

Two PKGBUILDs, for Arch users (`yay -S …` once they are on the AUR):

- `paste-get-bin` (`packaging/aur/paste-get-bin/`) unpacks the release's .deb: nothing to
  compile, it installs in seconds. It is updated after each release, by `update.sh`.
- `paste-get-git` (`packaging/aur/paste-get-git/`) builds the latest commit of the GitHub
  repository's default branch from source, so it needs no update per release.

CI builds and installs both with `makepkg` in an Arch container, starts both programs
(`--version`), and checks that each `.SRCINFO` matches its PKGBUILD. `paste-get-bin` is made from
the .deb of the same run, so this also shows that programs built on Ubuntu find Arch's libraries.
After changing a PKGBUILD by hand, regenerate its `.SRCINFO` on Arch:
`makepkg --printsrcinfo > .SRCINFO`. Until they are published, Arch users can build them from the
repository: `cd packaging/aur/paste-get-git && makepkg -si`.

### Once: an AUR account

On the Arch PC:

1. Make an SSH key for the AUR: `ssh-keygen -t ed25519 -f ~/.ssh/aur -C aur`. It asks for a
   passphrase: one is safer (it's asked at each upload); Enter twice for none.
2. Tell SSH to use it for the AUR:
   `printf 'Host aur.archlinux.org\n  IdentityFile ~/.ssh/aur\n  User aur\n' >> ~/.ssh/config`
3. Open <https://aur.archlinux.org/register>. Fill in a user name and an e-mail address. In
   *SSH Public Key*, paste the line that `cat ~/.ssh/aur.pub` prints (it starts with
   `ssh-ed25519`). Answer the captcha and create the account; the e-mail it sends has a link to
   choose a password.
4. Check it works: `ssh aur.archlinux.org help`. The first time, SSH asks whether to trust the
   server: compare the fingerprint it shows with the ED25519 one listed on the AUR's home page,
   then type `yes`. It prints a list of commands: the account works.
5. On GitHub, make sure the repository's default branch is `main` with the released work
   (*Settings* → *General* → *Default branch*): `paste-get-git` builds that branch.

### paste-get-git: once (again only if its PKGBUILD changes)

6. In the paste-get checkout:
   ```sh
   git -c init.defaultBranch=master clone ssh://aur@aur.archlinux.org/paste-get-git.git ~/aur/paste-get-git
   cp packaging/aur/paste-get-git/{PKGBUILD,.SRCINFO} ~/aur/paste-get-git/
   cd ~/aur/paste-get-git
   git add PKGBUILD .SRCINFO
   git commit -m "Initial upload"
   git push
   ```
   Git warns that the clone is empty: that's expected, the push creates the package. (The AUR
   only takes the `master` branch, hence `init.defaultBranch=master`.)
7. Check <https://aur.archlinux.org/packages/paste-get-git>, and that `yay -S paste-get-git`
   installs it.

### paste-get-bin: after each release

Don't upload it before step 8 has run once: until then its checksum is `SKIP`.

8. Once the GitHub release is **published** (not a draft), in the paste-get checkout:
   `packaging/aur/paste-get-bin/update.sh 0.1.0` (the new version). It sets the version and
   the .deb's checksum from the release's `SHA256SUMS`, downloads the .deb once to check it, and
   rewrites `.SRCINFO`. Commit the two changed files to the paste-get repository as usual.
9. The first time only:
   `git -c init.defaultBranch=master clone ssh://aur@aur.archlinux.org/paste-get-bin.git ~/aur/paste-get-bin`.
   The next times: `cd ~/aur/paste-get-bin && git pull`.
10. Upload it:
    ```sh
    cp packaging/aur/paste-get-bin/{PKGBUILD,.SRCINFO} ~/aur/paste-get-bin/
    cd ~/aur/paste-get-bin
    git add PKGBUILD .SRCINFO
    git commit -m "Update to 0.1.0"
    git push
    ```
11. Check that `yay -S paste-get-bin` installs the new version.

To change something else in `paste-get-bin` between releases (a dependency, say), raise `pkgrel`
by one, run `makepkg --printsrcinfo > .SRCINFO` in its directory, and upload as in step 10.

## Why there is no Flatpak yet

A Flatpak (and so Flathub, the store behind KDE Discover and GNOME Software on most
distributions) can't do paste-get's main job on the tier-1 desktops: see a copy made in another
app while paste-get is in the background. On Wayland, only the data-control protocols allow that
(`ext_data_control_manager_v1`, or the older `zwlr_data_control_manager_v1`), and both
compositors hide them from sandboxed apps. Checked in their source, November 2026:

- **KDE Plasma (KWin):** `src/wayland_server.cpp` lists `ext_data_control_manager_v1` among the
  interfaces a sandboxed client never sees (it is the only data-control protocol KWin offers). A
  client counts as sandboxed if it came through Flatpak's security context, and also if its
  process runs in an `app-flatpak-…` systemd scope (`src/wayland/clientconnection.cpp`), so
  even a Flatpak given the raw Wayland socket is caught.
- **Hyprland:** sandboxed clients (those that came through the security context) only see the
  protocols on a fixed list (`CProtocolManager::isGlobalPrivileged`, in
  `src/managers/ProtocolManager.cpp`), and neither data-control protocol is on it. A Flatpak
  given the raw Wayland socket would escape this, but that permission defeats the sandbox, and
  Flathub's review would most likely refuse it *(not asked)*.
- **X11 through XWayland is no way around it:** both compositors hand a Wayland app's copy to X11
  apps only while an X11 window has the keyboard focus (KWin: `src/xwayland/clipboard.cpp`;
  Hyprland: `src/xwayland/XWM.cpp`).

So a Flatpak paste-get could only read the clipboard while its own window has focus, and it has
no window most of the time. Shipping one would look like it works and then miss every copy.

What would change that:
- an xdg-desktop-portal for clipboard managers (asking the user once, then allowed in the
  background), implemented by the KDE and Hyprland portals. Today the only clipboard portal
  works inside a remote-desktop session (a "control this computer" prompt and indicator), and
  Hyprland's portal has no remote-desktop part;
- or KWin and Hyprland letting the user grant data-control to one sandboxed app.

Until then: the AppImage, .deb, .rpm and AUR packages, which are not sandboxed.

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
2. The Android release key is set up (see above). Set the version:
   - `Cargo.toml`, `[workspace.package] version`. The daemon, the tray, the phone's Rust core
     and the Windows and Linux packages all take it from there (`desktop/tray/tauri*.json` set
     none on purpose);
   - `android/app/build.gradle.kts`: the same `versionName`, and `versionCode` = major × 10000
     + minor × 100 + patch (0.1.0 is 100, 0.2.0 is 200, 1.0.3 is 10003), so a newer version
     always has a higher code, as Android requires to update;
   - a `<release version="…" date="YYYY-MM-DD">` at the top of `<releases>` in
     `packaging/linux/io.github.nexoryx.pasteget.metainfo.xml`;
   - a `## <version> (YYYY-MM-DD)` section at the top of `CHANGELOG.md`, in plain words for
     the people who use paste-get. It is the base of the release notes.

   `packaging/check-version.sh` checks that they agree (CI runs it on every push); for 0.1.0
   they are there already: set the dates. Commit.
3. Tag and push: `git tag v0.1.0 && git push origin v0.1.0`. The *Release* workflow first runs
   `packaging/check-version.sh v0.1.0`: a tag that doesn't name the version of the tree fails
   before anything is built.
4. Wait for the *Release* workflow, then open the draft release on GitHub. Install each package
   on a real machine (Windows 11, Arch) and run `docs/TESTING.md`, sections 9–11.
5. The security review of `docs/REVIEW.md` is done for this version, or the release notes say it
   isn't.
6. Write the release notes from `CHANGELOG.md` (what works, what's known not to), then publish.
