#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Checks the .deb and .rpm that build-desktop.sh made: the files they install and the libraries
# they ask for. The Release workflow runs it; it needs dpkg-deb and rpm.
#
#   packaging/linux/check-packages.sh target/release/bundle
set -euo pipefail

bundle="${1:?usage: check-packages.sh <bundle directory>}"
shopt -s nullglob
debs=("$bundle"/deb/*.deb)
rpms=("$bundle"/rpm/*.rpm)
if [ "${#debs[@]}" -ne 1 ] || [ "${#rpms[@]}" -ne 1 ]; then
  echo "expected one .deb and one .rpm under $bundle" >&2
  exit 1
fi
deb="${debs[0]}"
rpm="${rpms[0]}"

# Both install these.
files=(
  /usr/bin/pasteget
  /usr/bin/pasteget-tray
  /usr/share/applications/paste-get.desktop
  /usr/share/icons/hicolor/512x512/apps/pasteget-tray.png
)
# WebKitGTK and GTK are linked; the app indicator library (the tray icon) is loaded at run time,
# so nothing would notice it missing before the icon fails to show.
deb_depends=(libwebkit2gtk-4.1-0 libgtk-3-0 libayatana-appindicator3-1)
rpm_requires=(
  "libwebkit2gtk-4.1.so.0()(64bit)"
  "libgtk-3.so.0()(64bit)"
  "libayatana-appindicator3.so.1()(64bit)"
)

failed=0
missing() {
  echo "::error::$1 lacks $2" >&2
  failed=1
}

deb_files="$(dpkg-deb -c "$deb" | awk '{ print $6 }' | sed 's|^\./|/|; s|^\([^/]\)|/\1|')"
rpm_files="$(rpm -qlp "$rpm")"
for file in "${files[@]}"; do
  grep -qxF "$file" <<<"$deb_files" || missing "$deb" "$file"
  grep -qxF "$file" <<<"$rpm_files" || missing "$rpm" "$file"
done

deb_has="$(dpkg-deb -f "$deb" Depends | tr ',' '\n' | sed 's/^ *//; s/ .*//')"
for dep in "${deb_depends[@]}"; do
  grep -qxF "$dep" <<<"$deb_has" || missing "$deb" "a dependency on $dep"
done
rpm_has="$(rpm -qp --requires "$rpm")"
for dep in "${rpm_requires[@]}"; do
  grep -qxF "$dep" <<<"$rpm_has" || missing "$rpm" "a dependency on $dep"
done

if [ "$failed" -ne 0 ]; then
  exit 1
fi
echo "$deb:"
dpkg-deb -f "$deb" Package Version Depends
echo "$rpm:"
rpm -qp --requires "$rpm" | grep -v '^rpmlib('
echo "Both install:"
printf '  %s\n' "${files[@]}"
