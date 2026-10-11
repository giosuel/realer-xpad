"""Optional Hyprland integration via its IPC socket.

Wayland does not let clients position their own windows, so on Hyprland we ask
the compositor directly to float, move, resize and pin note windows, and read
back where the user put them.  On any other compositor this module is inert.
"""

from __future__ import annotations

import json
import os
import socket
from pathlib import Path


def _socket_path() -> Path | None:
    sig = os.environ.get("HYPRLAND_INSTANCE_SIGNATURE")
    if not sig:
        return None
    candidates = []
    if runtime := os.environ.get("XDG_RUNTIME_DIR"):
        candidates.append(Path(runtime) / "hypr" / sig / ".socket.sock")
    candidates.append(Path("/tmp/hypr") / sig / ".socket.sock")  # Hyprland < 0.40
    return next((p for p in candidates if p.exists()), None)


class Hyprland:
    def __init__(self, socket_path: Path | str):
        self.socket_path = str(socket_path)

    @classmethod
    def detect(cls) -> "Hyprland | None":
        if os.environ.get("REALER_XPAD_NO_HYPR"):
            return None
        path = _socket_path()
        return cls(path) if path else None

    def request(self, message: str) -> str:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
            sock.settimeout(1.0)
            sock.connect(self.socket_path)
            sock.sendall(message.encode())
            chunks = []
            while chunk := sock.recv(65536):
                chunks.append(chunk)
        return b"".join(chunks).decode(errors="replace")

    def clients(self) -> list[dict]:
        try:
            return json.loads(self.request("j/clients"))
        except (OSError, ValueError):
            return []

    def find(self, initial_title: str, pid: int | None = None) -> dict | None:
        for client in self.clients():
            if client.get("initialTitle") == initial_title and (pid is None or client.get("pid") == pid):
                return client
        return None

    def batch(self, *commands: str) -> bool:
        try:
            reply = self.request("[[BATCH]]" + ";".join(f"dispatch {c}" for c in commands))
        except OSError:
            return False
        return "ok" in reply

    def place(self, address: str, x: int | None, y: int | None, width: int, height: int) -> bool:
        target = f"address:{address}"
        commands = [f"setfloating {target}", f"resizewindowpixel exact {width} {height},{target}"]
        if x is not None and y is not None:
            commands.append(f"movewindowpixel exact {x} {y},{target}")
        return self.batch(*commands)

    def set_pinned(self, client: dict, pinned: bool) -> bool:
        if bool(client.get("pinned")) == pinned:
            return True
        target = f"address:{client['address']}"
        commands = [f"pin {target}"]
        if pinned and not client.get("floating"):
            commands.insert(0, f"setfloating {target}")
        return self.batch(*commands)

    @staticmethod
    def geometry(client: dict) -> tuple[int, int, int, int]:
        (x, y), (w, h) = client.get("at", (0, 0)), client.get("size", (0, 0))
        return x, y, w, h
