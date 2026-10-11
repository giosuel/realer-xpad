#!/usr/bin/env bash
# Install Realer Xpad for the current user (no root needed).
#   ./install.sh            install launcher + desktop entry
#   ./install.sh --uninstall
set -euo pipefail

src="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bin="$HOME/.local/bin/realer-xpad"
apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
desktop="$apps/ch.giosuel.RealerXpad.desktop"

if [[ "${1:-}" == "--uninstall" ]]; then
  rm -f "$bin" "$desktop"
  echo "Removed launcher and desktop entry. Your notes in ~/.local/share/realer-xpad are untouched."
  exit 0
fi

python3 - <<'PY' || { echo; echo "Missing dependencies. On Debian/Ubuntu:"; echo "  sudo apt install python3-gi gir1.2-gtk-4.0 gir1.2-adw-1"; echo "On Arch:"; echo "  sudo pacman -S python-gobject gtk4 libadwaita"; exit 1; }
import gi
gi.require_version("Gtk", "4.0"); gi.require_version("Adw", "1")
from gi.repository import Adw, Gtk, Gsk
assert (Gtk.get_major_version(), Gtk.get_minor_version()) >= (4, 14), "GTK >= 4.14 required"
assert hasattr(Gsk, "PathBuilder")
PY

mkdir -p "$(dirname "$bin")" "$apps"
cat > "$bin" <<SH
#!/bin/sh
PYTHONPATH="$src\${PYTHONPATH:+:\$PYTHONPATH}" exec python3 -m realer_xpad "\$@"
SH
chmod +x "$bin"
cp "$src/extras/ch.giosuel.RealerXpad.desktop" "$desktop"

echo "Installed: $bin"
case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *) echo "Note: add ~/.local/bin to your PATH.";; esac
echo "Hyprland users: see $src/extras/hyprland.conf for window rules and keybinds."
