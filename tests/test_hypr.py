import json
import socket
import threading

import pytest

from realer_xpad.hypr import Hyprland


class FakeHyprland:
    """Minimal stand-in for Hyprland's request socket."""

    def __init__(self, path, clients):
        self.clients = clients
        self.requests = []
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.bind(str(path))
        self.sock.listen()
        self.thread = threading.Thread(target=self._serve, daemon=True)
        self.thread.start()

    def _serve(self):
        while True:
            try:
                conn, _ = self.sock.accept()
            except OSError:
                return
            with conn:
                msg = conn.recv(65536).decode()
                self.requests.append(msg)
                reply = json.dumps(self.clients) if msg == "j/clients" else "ok"
                conn.sendall(reply.encode())

    def close(self):
        self.sock.close()


CLIENT = {"address": "0xabc", "pid": 42, "initialTitle": "realer-xpad-1", "at": [100, 200],
          "size": [300, 250], "floating": False, "pinned": False}


@pytest.fixture
def hypr(tmp_path):
    path = tmp_path / ".socket.sock"
    fake = FakeHyprland(path, [CLIENT, {**CLIENT, "address": "0xdef", "initialTitle": "other"}])
    yield Hyprland(path), fake
    fake.close()


def test_find_by_initial_title(hypr):
    h, _ = hypr
    assert h.find("realer-xpad-1")["address"] == "0xabc"
    assert h.find("realer-xpad-1", pid=7) is None
    assert h.find("missing") is None
    assert h.geometry(h.find("realer-xpad-1")) == (100, 200, 300, 250)


def test_place_sends_batch(hypr):
    h, fake = hypr
    assert h.place("0xabc", 10, 20, 300, 250)
    assert fake.requests[-1] == (
        "[[BATCH]]dispatch setfloating address:0xabc;"
        "dispatch resizewindowpixel exact 300 250,address:0xabc;"
        "dispatch movewindowpixel exact 10 20,address:0xabc"
    )


def test_place_without_position_only_resizes(hypr):
    h, fake = hypr
    h.place("0xabc", None, None, 300, 250)
    assert "movewindowpixel" not in fake.requests[-1]


def test_pin_floats_first_and_is_idempotent(hypr):
    h, fake = hypr
    assert h.set_pinned(CLIENT, True)
    assert fake.requests[-1] == "[[BATCH]]dispatch setfloating address:0xabc;dispatch pin address:0xabc"
    n = len(fake.requests)
    assert h.set_pinned(CLIENT, False)  # already unpinned: no request
    assert len(fake.requests) == n


def test_detect(tmp_path, monkeypatch):
    monkeypatch.delenv("HYPRLAND_INSTANCE_SIGNATURE", raising=False)
    assert Hyprland.detect() is None
    sock_dir = tmp_path / "hypr" / "sig"
    sock_dir.mkdir(parents=True)
    (sock_dir / ".socket.sock").touch()
    monkeypatch.setenv("HYPRLAND_INSTANCE_SIGNATURE", "sig")
    monkeypatch.setenv("XDG_RUNTIME_DIR", str(tmp_path))
    monkeypatch.delenv("REALER_XPAD_NO_HYPR", raising=False)
    assert Hyprland.detect().socket_path == str(sock_dir / ".socket.sock")


def test_dead_socket_is_harmless(tmp_path):
    h = Hyprland(tmp_path / "nope.sock")
    assert h.clients() == []
    assert h.batch("pin") is False
