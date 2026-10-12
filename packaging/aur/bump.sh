#!/usr/bin/env bash
# Point both PKGBUILDs at a released version and refresh their checksums.
#
#   packaging/aur/bump.sh 0.1.2 && packaging/aur/test.sh
#
# test.sh then builds both packages and regenerates the .SRCINFO files.
set -euo pipefail
version="${1:?usage: bump.sh VERSION}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
url=https://github.com/giosuel/realer-xpad

sum() { curl -fsSL "$1" | sha256sum | cut -d' ' -f1; }
src=$(sum "$url/archive/v$version.tar.gz")
bin=$(sum "$url/releases/download/v$version/realer-xpad-$version-x86_64-linux.tar.gz")

for pkg in realer-xpad realer-xpad-bin; do
  sed -i -e "s/^pkgver=.*/pkgver=$version/" -e "s/^pkgrel=.*/pkgrel=1/" "$here/$pkg/PKGBUILD"
done
sed -i "s/^sha256sums=('.*')/sha256sums=('$src')/" "$here/realer-xpad/PKGBUILD"
sed -i "s/^sha256sums_x86_64=('.*')/sha256sums_x86_64=('$bin')/" "$here/realer-xpad-bin/PKGBUILD"
grep -H "^pkgver\|sha256" "$here"/*/PKGBUILD
