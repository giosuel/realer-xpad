#!/usr/bin/env bash
# Run a command inside a throwaway headless sway (pure Wayland, no Xwayland).
#   tools/headless.sh pytest -q
#   tools/headless.sh python tools/screenshot.py screenshots
set -euo pipefail

export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
tmp=$(mktemp -d)
trap 'kill "$sway_pid" 2>/dev/null || true; rm -rf "$tmp"' EXIT

cat > "$tmp/config" <<'EOF'
output HEADLESS-1 resolution 1600x1000 bg #303030 solid_color
for_window [app_id="ch.giosuel.RealerXpad"] floating enable
default_border none
EOF

before=$(ls "$XDG_RUNTIME_DIR" | grep -E '^wayland-[0-9]+$' || true)
WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman WAYLAND_DISPLAY= \
  sway -c "$tmp/config" >"$tmp/sway.log" 2>&1 &
sway_pid=$!

for _ in $(seq 50); do
  display=$(comm -13 <(echo "$before") <(ls "$XDG_RUNTIME_DIR" | grep -E '^wayland-[0-9]+$') | head -1)
  [ -n "$display" ] && break
  sleep 0.1
done
[ -n "${display:-}" ] || { echo "sway failed to start" >&2; cat "$tmp/sway.log" >&2; exit 1; }

# No desktop portal in a bare session; don't let GTK/libadwaita wait on it.
export WAYLAND_DISPLAY="$display" GDK_BACKEND=wayland ADW_DISABLE_PORTAL=1 GDK_DEBUG=no-portals
unset DISPLAY
"$@"
