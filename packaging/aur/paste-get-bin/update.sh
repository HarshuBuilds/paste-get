#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Points paste-get-bin at a published release: sets pkgver, and the checksum of the release's
# .deb from its SHA256SUMS. Then downloads the .deb once to check it, and rewrites .SRCINFO.
# Run it on Arch (it needs makepkg) once the GitHub release is published, not while it's a draft:
#
#   packaging/aur/paste-get-bin/update.sh 0.1.0
#
# See docs/RELEASING.md for the upload to the AUR.
set -euo pipefail

version="${1:?usage: update.sh <version, e.g. 0.1.0>}"
version="${version#v}"
# pkgver can't contain '-'.
if ! [[ "$version" =~ ^[0-9]+(\.[0-9]+)*$ ]]; then
  echo "not a version: $version (digits and dots only)" >&2
  exit 1
fi
cd "$(dirname "$0")"

release="https://github.com/Nexoryx-business/paste-get/releases/download/v$version"
deb="paste-get_${version}_amd64.deb"
if ! sums="$(curl -fsSL "$release/SHA256SUMS")"; then
  echo "could not download $release/SHA256SUMS: is the release published?" >&2
  exit 1
fi
sum="$(awk -v file="$deb" '$2 == file { print $1 }' <<<"$sums")"
if ! [[ "$sum" =~ ^[0-9a-f]{64}$ ]]; then
  echo "no checksum for $deb in $release/SHA256SUMS" >&2
  exit 1
fi

sed -i \
  -e "s/^pkgver=.*/pkgver=$version/" \
  -e "s/^pkgrel=.*/pkgrel=1/" \
  -e "s/^sha256sums=.*/sha256sums=('$sum')/" \
  PKGBUILD

# Downloads the .deb and checks it against the checksum, in a directory of its own.
downloads="$(mktemp -d)"
trap 'rm -rf "$downloads"' EXIT
SRCDEST="$downloads" BUILDDIR="$downloads" makepkg --verifysource
makepkg --printsrcinfo > .SRCINFO

echo "paste-get-bin is now $version. Commit PKGBUILD and .SRCINFO, then push them to the AUR."
