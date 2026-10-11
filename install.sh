#!/usr/bin/env bash
# Install Realer Xpad for the current user (no root needed).
#
#   ./install.sh               build from source with cargo (or use the bundled
#                              binary when run from a release archive)
#   ./install.sh --uninstall   remove launcher and desktop entry
set -euo pipefail

src="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bin_dir="$HOME/.local/bin"
apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
desktop="$apps/ch.giosuel.RealerXpad.desktop"
icon="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps/ch.giosuel.RealerXpad.svg"

if [[ "${1:-}" == "--uninstall" ]]; then
  rm -f "$bin_dir/realer-xpad" "$desktop" "$icon"
  echo "Removed launcher and desktop entry. Your notes in ~/.local/share/realer-xpad are untouched."
  exit 0
fi

deps_hint() {
  cat >&2 <<'HINT'

Realer Xpad needs GTK >= 4.16 and libadwaita >= 1.5. To build it you also need
Rust (https://rustup.rs) and the development packages:

  Debian / Ubuntu  sudo apt install libgtk-4-dev libadwaita-1-dev build-essential
  Arch             sudo pacman -S gtk4 libadwaita base-devel
  Fedora           sudo dnf install gtk4-devel libadwaita-devel gcc
HINT
}

if [[ -x "$src/realer-xpad" ]]; then
  binary="$src/realer-xpad"            # release archive
elif command -v cargo >/dev/null; then
  if ! pkg-config --atleast-version=4.16 gtk4 2>/dev/null ||
     ! pkg-config --atleast-version=1.5 libadwaita-1 2>/dev/null; then
    echo "error: GTK 4.16 / libadwaita 1.5 development files not found." >&2
    deps_hint
    exit 1
  fi
  echo "Building Realer Xpad (release)…"
  cargo build --release --locked --manifest-path "$src/Cargo.toml"
  binary="${CARGO_TARGET_DIR:-$src/target}/release/realer-xpad"
else
  echo "error: cargo not found, and no prebuilt binary next to this script." >&2
  deps_hint
  exit 1
fi

mkdir -p "$bin_dir" "$apps" "$(dirname "$icon")"
install -m 755 "$binary" "$bin_dir/realer-xpad"
install -m 644 "$src/extras/ch.giosuel.RealerXpad.desktop" "$desktop"
install -m 644 "$src/data/icons/hicolor/scalable/apps/ch.giosuel.RealerXpad.svg" "$icon"

echo "Installed: $bin_dir/realer-xpad ($("$bin_dir/realer-xpad" --version))"
case ":$PATH:" in *":$bin_dir:"*) ;; *) echo "Note: add ~/.local/bin to your PATH.";; esac
echo "Hyprland users: see extras/hyprland.conf for window rules and keybinds."
