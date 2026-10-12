#!/usr/bin/env bash
# Run a command inside a throwaway headless sway (pure Wayland, no Xwayland).
#   tools/headless.sh pytest -q
#   tools/headless.sh python tools/screenshot.py screenshots
set -euo pipefail

# A private session bus too, so tray tests don't reach the real tray.
if [ -z "${REALER_XPAD_HEADLESS:-}" ] && command -v dbus-run-session >/dev/null; then
  export REALER_XPAD_HEADLESS=1
  exec dbus-run-session -- "$0" "$@"
fi
export REALER_XPAD_HEADLESS=1

export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
tmp=$(mktemp -d)
trap 'kill "$sway_pid" 2>/dev/null || true; rm -rf "$tmp"' EXIT

# sway reports its socket name through an exec'd child, so there's no guessing.
cat > "$tmp/config" <<EOF
output HEADLESS-1 resolution 1600x1000 bg #303030 solid_color
for_window [app_id="ch.giosuel.RealerXpad"] floating enable
default_border none
exec printenv WAYLAND_DISPLAY > "$tmp/display"
EOF

WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman WAYLAND_DISPLAY='' \
  sway -c "$tmp/config" >"$tmp/sway.log" 2>&1 &
sway_pid=$!

for _ in $(seq 50); do
  [ -s "$tmp/display" ] && break
  sleep 0.1
done
if [ ! -s "$tmp/display" ]; then
  echo "sway failed to start" >&2
  cat "$tmp/sway.log" >&2
  exit 1
fi

# No desktop portal in a bare session; don't let GTK/libadwaita wait on it.
WAYLAND_DISPLAY=$(cat "$tmp/display")
export WAYLAND_DISPLAY GDK_BACKEND=wayland ADW_DISABLE_PORTAL=1 GDK_DEBUG=no-portals GTK_A11Y=none
# Not the real Hyprland, even when run from inside it.
unset DISPLAY HYPRLAND_INSTANCE_SIGNATURE
"$@"
