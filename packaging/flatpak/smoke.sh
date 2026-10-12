#!/usr/bin/env bash
# Inside tools/headless.sh: start the installed Flatpak against a fake Hyprland
# IPC socket and check that a note window opens and Hyprland gets asked about it.
set -euo pipefail
SWAYSOCK=$(ls "$XDG_RUNTIME_DIR"/sway-ipc.*.sock)
export SWAYSOCK
sig=realer-xpad-smoke
sock="$XDG_RUNTIME_DIR/hypr/$sig/.socket.sock"
log="$(mktemp)"
mkdir -p "$(dirname "$sock")"
python3 -I -c '
import json, socket, sys
srv = socket.socket(socket.AF_UNIX)
srv.bind(sys.argv[1]); srv.listen()
with open(sys.argv[2], "a", buffering=1) as log:
    while True:
        conn, _ = srv.accept()
        msg = conn.recv(65536).decode()
        log.write(msg + "\n")
        conn.sendall(b"[]" if msg.startswith("j/") else b"ok")
        conn.close()
' "$sock" "$log" &
fake=$!
app=""
trap 'kill $fake $app 2>/dev/null || true; rm -f "$log"' EXIT

HYPRLAND_INSTANCE_SIGNATURE=$sig flatpak run ch.giosuel.RealerXpad new &
app=$!
for _ in $(seq 100); do
  swaymsg -t get_tree | grep -q '"app_id": *"ch.giosuel.RealerXpad"' && grep -q "j/clients" "$log" && break
  sleep 0.2
done
echo "Hyprland requests from the sandbox: $(sort -u "$log" | tr '\n' ' ')"
swaymsg -t get_tree | grep -q '"app_id": *"ch.giosuel.RealerXpad"' || { echo "error: no note window" >&2; exit 1; }
grep -q "j/clients" "$log" || { echo "error: the sandbox could not reach the Hyprland socket" >&2; exit 1; }
echo "Note window open, Hyprland IPC reachable"
flatpak run ch.giosuel.RealerXpad quit
