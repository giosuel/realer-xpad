#!/usr/bin/env bash
# Build both AUR packages in a clean Arch container, lint them with namcap,
# install them and refresh their .SRCINFO files.   packaging/aur/test.sh
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
engine="$(command -v podman || command -v docker)"
# shellcheck disable=SC2016 # the script runs inside the container
"$engine" run --rm -v "$here:/aur:Z" docker.io/archlinux:base-devel bash -euo pipefail -c '
  pacman -Syu --noconfirm --needed namcap >/dev/null
  useradd -m builder
  echo "builder ALL=(ALL) NOPASSWD: ALL" > /etc/sudoers.d/builder
  for pkg in realer-xpad realer-xpad-bin; do
    echo "==> $pkg"
    cp -r "/aur/$pkg" "/home/builder/$pkg"
    chown -R builder: "/home/builder/$pkg"
    cd "/home/builder/$pkg"
    namcap PKGBUILD
    su builder -c "makepkg --syncdeps --noconfirm --cleanbuild" >makepkg.log 2>&1 || { tail -40 makepkg.log; exit 1; }
    grep -E "test result|Finished" makepkg.log || true
    su builder -c "makepkg --printsrcinfo" > "/aur/$pkg/.SRCINFO"
    namcap ./*.pkg.tar.zst
    pacman -U --noconfirm ./*.pkg.tar.zst >/dev/null
    realer-xpad --version
    pacman -Ql "$pkg" | grep -v "/$"
    pacman -R --noconfirm "$pkg" >/dev/null
  done
'
