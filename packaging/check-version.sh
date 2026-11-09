#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Checks that every place that states paste-get's version agrees with Cargo.toml:
#
#   packaging/check-version.sh            # the files agree with each other
#   packaging/check-version.sh v0.1.0     # and with this release tag
#
# - Cargo.toml, `[workspace.package] version`: the source of truth. The tray, the daemon and the
#   phone's Rust core take it from there, and so do the Windows and Linux packages, since
#   desktop/tray/tauri*.json don't set a version of their own.
# - android/app/build.gradle.kts: `versionName` is the same, and `versionCode` is
#   major × 10000 + minor × 100 + patch.
# - packaging/linux/io.github.nexoryx.pasteget.metainfo.xml has a `<release>` for it.
# - CHANGELOG.md has a `## <version>` section.
#
# See docs/RELEASING.md.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

failed=0
fail() {
  echo "::error::$1" >&2
  failed=1
}

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
if ! [[ "$version" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
  echo "::error::Cargo.toml's version \"$version\" is not major.minor.patch" >&2
  exit 1
fi
major=${BASH_REMATCH[1]}
minor=${BASH_REMATCH[2]}
patch=${BASH_REMATCH[3]}
if ((minor > 99 || patch > 99)); then
  fail "minor and patch must stay below 100 for the Android versionCode"
fi
code=$((major * 10000 + minor * 100 + patch))

if [ $# -gt 0 ] && [ "$1" != "v$version" ]; then
  fail "the tag $1 doesn't match Cargo.toml's version $version: tag v$version, or change the version first"
fi

gradle=android/app/build.gradle.kts
grep -qE "^\s*versionName = \"$version\"$" "$gradle" ||
  fail "$gradle: versionName should be \"$version\", as in Cargo.toml"
grep -qE "^\s*versionCode = $code$" "$gradle" ||
  fail "$gradle: versionCode should be $code for $version (major × 10000 + minor × 100 + patch)"

for config in desktop/tray/tauri*.json; do
  if grep -q '"version"' "$config"; then
    fail "$config sets a version: leave it out, so Tauri takes Cargo.toml's"
  fi
done

grep -qF "<release version=\"$version\"" packaging/linux/io.github.nexoryx.pasteget.metainfo.xml ||
  fail "add a <release> for $version to the AppStream metainfo"

grep -qE "^## $version( |$)" CHANGELOG.md ||
  fail "add a \"## $version\" section to CHANGELOG.md"

if [ "$failed" -ne 0 ]; then
  exit 1
fi
echo "Version $version (Android versionCode $code) everywhere."
