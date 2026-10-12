#!/usr/bin/env bash
# Build the Flatpak from the local checkout in a container, run Flathub's linter
# on the manifest and the result, and check that the app starts.
#
#   packaging/flatpak/test.sh
#
# Needs podman (runs privileged, for flatpak-builder's sandbox). Runtimes are
# cached in the "rx-flatpak" volume. The bundle ends up in dist/.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
podman volume exists rx-flatpak || podman volume create rx-flatpak >/dev/null
mkdir -p "$root/dist"

# shellcheck disable=SC2016 # the script runs inside the container
podman run --rm --privileged -v rx-flatpak:/var/lib/flatpak -v "$root:/src:Z" \
  docker.io/library/fedora:latest bash -euo pipefail -c '
  dnf -q -y install flatpak ostree dbus-daemon sway python3 >/dev/null 2>&1
  flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
  # flatpak-builder cannot install runtimes from inside its own sandbox.
  flatpak install -y --noninteractive flathub org.flatpak.Builder \
    org.gnome.Platform//51 org.gnome.Sdk//51 org.freedesktop.Sdk.Extension.rust-stable//26.08 >/dev/null
  export XDG_RUNTIME_DIR=/run/user/0 && mkdir -p -m 700 "$XDG_RUNTIME_DIR"
  cd /src/packaging/flatpak
  out=/src/.flatpak-builder # state, build and repo must share a filesystem
  # org.flatpak.Builder (what Flathub uses) asks the host flatpak about runtimes
  # over the session bus.
  builder() { dbus-run-session -- flatpak run --command="$1" org.flatpak.Builder "${@:2}" 2>&1 | { grep -v "^dbus-daemon" || true; }; }

  echo "==> lint manifest"
  builder flatpak-builder-lint manifest ch.giosuel.RealerXpad.yml
  echo "==> build"
  # Mirror screenshots into the repo the way the Flathub builders do.
  builder flatpak-builder --force-clean --disable-rofiles-fuse --repo="$out/repo" --state-dir="$out/state" \
    --compose-url-policy=full --mirror-screenshots-url=https://dl.flathub.org/media/ "$out/build" ch.giosuel.RealerXpad.yml \
    >/tmp/build.log 2>&1 || { tail -40 /tmp/build.log; exit 1; }
  grep -E "Finished|Exporting" /tmp/build.log || true
  ostree commit --repo="$out/repo" --canonical-permissions --branch=screenshots/x86_64 \
    "$out/build/files/share/app-info/media" >/dev/null
  flatpak build-update-repo "$out/repo" >/dev/null
  echo "==> lint repo"
  builder flatpak-builder-lint repo "$out/repo"
  echo "==> run"
  flatpak remote-delete --force local 2>/dev/null || true
  flatpak remote-add --no-gpg-verify local "$out/repo"
  flatpak install -y --noninteractive --reinstall local ch.giosuel.RealerXpad >/dev/null
  flatpak run ch.giosuel.RealerXpad --version
  setcap -r /usr/bin/sway 2>/dev/null || true # file caps make sway refuse to run as root
  dbus-run-session -- /src/tools/headless.sh /src/packaging/flatpak/smoke.sh 2>&1 | { grep -v "^dbus-daemon" || true; }
  flatpak build-bundle --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
    "$out/repo" /src/dist/ch.giosuel.RealerXpad.flatpak ch.giosuel.RealerXpad
  ls -l /src/dist/ch.giosuel.RealerXpad.flatpak
'
