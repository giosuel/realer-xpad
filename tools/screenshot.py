#!/usr/bin/env python3
"""Render demo notes to PNGs (and a collage) for docs and visual checks.

Run inside a Wayland session (e.g. headless sway, see tools/headless.sh):

    python tools/screenshot.py OUT_DIR [--dark]
"""

from __future__ import annotations

import math
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
gi.require_version("Graphene", "1.0")
from gi.repository import Adw, GLib, Graphene, Gtk

from realer_xpad.app import App
from realer_xpad.store import Note, NoteStore

SHOPPING = """# Weekend
- [x] Coffee beans
- [x] Oat milk
- [ ] Sourdough
- [ ] **Basil** for pesto
"""

IDEAS = """## Project ideas
Things to try with *Realer Xpad*:

1. Markdown that **renders live**
2. Draw with a ~~mouse~~ pen tablet
3. Pin notes on [Hyprland](https://hyprland.org)

> Ideas are cheap, shipping is not.

Run `realer-xpad new` from a keybind.
"""

SKETCH = """### Layout sketch
"""


def wave(x0, y0, length, amp, steps=40, pressure=False):
    pts = []
    for i in range(steps + 1):
        t = i / steps
        p = 0.3 + 0.7 * math.sin(math.pi * t) if pressure else 1.0
        pts.append([x0 + length * t, y0 + amp * math.sin(t * math.pi * 3), p])
    return pts


def box(x, y, w, h):
    corners = [(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y + 1)]
    return densify(corners)


def densify(corners, step=4.0):
    """Interpolate like real pointer input (which reports every few pixels)."""
    pts = []
    for (ax, ay), (bx, by) in zip(corners, corners[1:]):
        n = max(1, int(math.hypot(bx - ax, by - ay) / step))
        pts += [[ax + (bx - ax) * i / n, ay + (by - ay) * i / n, 1.0] for i in range(n)]
    pts.append([*corners[-1], 1.0])
    return pts


def demo_notes() -> list[Note]:
    sketch_strokes = [
        {"tool": "pen", "color": "ink", "width": 2.0, "points": box(24, 70, 120, 80)},
        {"tool": "pen", "color": "ink", "width": 2.0, "points": box(160, 70, 100, 36)},
        {"tool": "pen", "color": "ink", "width": 2.0, "points": box(160, 116, 100, 34)},
        {"tool": "highlighter", "color": "#f76b15", "width": 4.0, "points": wave(30, 185, 220, 4)},
        {"tool": "pen", "color": "#3e63dd", "width": 4.0, "points": wave(40, 230, 200, 14, pressure=True)},
        {"tool": "pen", "color": "#e5484d", "width": 4.0,
         "points": densify([(70, 110), (80, 122), (104, 92)])},
    ]
    return [
        Note(text=SHOPPING, color="yellow", width=280, height=250, created=1),
        Note(text=IDEAS, color="lavender", width=320, height=390, created=2),
        Note(text=SKETCH, color="mint", width=290, height=330, mode="draw", strokes=sketch_strokes, created=3),
        Note(text="", color="sky", width=260, height=200, created=4),
    ]


def render_widget(widget: Gtk.Widget, path: Path) -> None:
    w, h = widget.get_width(), widget.get_height()
    paintable = Gtk.WidgetPaintable.new(widget)
    snap = Gtk.Snapshot()
    paintable.snapshot(snap, w, h)
    node = snap.to_node()
    texture = widget.get_native().get_renderer().render_texture(node, Graphene.Rect().init(0, 0, w, h))
    texture.save_to_png(str(path))


def main() -> int:
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "screenshots")
    dark = "--dark" in sys.argv
    out.mkdir(parents=True, exist_ok=True)
    store = NoteStore(tempfile.mkdtemp(prefix="rx-shot-"))
    for note in demo_notes():
        store.save(note, touch=False)

    app = App(store, unique=False)
    shots: list[Path] = []

    def setup(*_):
        Adw.StyleManager.get_default().set_color_scheme(
            Adw.ColorScheme.FORCE_DARK if dark else Adw.ColorScheme.FORCE_LIGHT)

    def capture() -> bool:
        suffix = "-dark" if dark else ""
        wins = [app.windows[n.id] for n in sorted(app.notes(), key=lambda n: n.created)]
        # Show the hover state (controls visible) on the drawing note.
        wins[2].add_css_class("hover")
        for i, win in enumerate(wins):
            path = out / f"note{i + 1}{suffix}.png"
            render_widget(win, path)
            shots.append(path)
        app.open_manager()
        GLib.timeout_add(600, capture_manager, suffix)
        return False

    def capture_manager(suffix) -> bool:
        path = out / f"manager{suffix}.png"
        render_widget(app.manager, path)
        shots.append(path)
        app.quit()
        return False

    app.connect("startup", setup)
    GLib.timeout_add(1500, capture)
    app.run(["realer-xpad"])

    collage(shots, out / f"collage{'-dark' if dark else ''}.png", dark)
    print("\n".join(str(p) for p in shots))
    return 0


def collage(shots: list[Path], dest: Path, dark: bool) -> None:
    bg = ("#1b1d26", "#2a2140") if dark else ("#e8ecf7", "#f6e9f2")
    notes = [str(p) for p in shots if p.name.startswith("note")]
    manager = [str(p) for p in shots if p.name.startswith("manager")]
    shadow = ["(", "+clone", "-background", "black", "-shadow", "35x12+0+8", ")", "+swap",
              "-background", "none", "-layers", "merge", "+repage"]
    args = ["magick", "-background", "none"]
    for img in notes[:3]:
        args += ["(", img, *shadow, ")"]
    args += ["-gravity", "north", "+smush", "-30", "(", notes[3], *shadow, ")",
             "(", manager[0], *shadow, ")", "+smush", "-30", "-append",
             "-gravity", "center", "(", "+clone", "-alpha", "off", "-fill", bg[0], "-colorize", "100",
             "-sparse-color", "barycentric", f"0,0 {bg[0]} %[fx:w],%[fx:h] {bg[1]}", ")", "+swap",
             "-compose", "over", "-composite", "-bordercolor", bg[0], "-border", "36", str(dest)]
    subprocess.run(args, check=False)


if __name__ == "__main__":
    sys.exit(main())
