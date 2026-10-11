"""Note colour palette (light + dark variants) and stylesheet loading."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from .drawing import INK_COLORS


@dataclass(frozen=True)
class Swatch:
    bg: str      # note background
    fg: str      # text colour
    accent: str  # bullets, links, checkboxes
    label: str


PALETTE: dict[str, tuple[Swatch, Swatch]] = {
    #            light                                         dark
    "yellow":   (Swatch("#fff4b0", "#3d3510", "#b07d00", "Lemon"),
                 Swatch("#3b3418", "#f5ecc2", "#f0c43c", "Lemon")),
    "peach":    (Swatch("#ffdcc2", "#45250f", "#c4561a", "Peach"),
                 Swatch("#422a1c", "#f8dccb", "#ff9a5c", "Peach")),
    "pink":     (Swatch("#ffd6e4", "#4a1730", "#c2316b", "Rose"),
                 Swatch("#40222f", "#f9d6e3", "#ff7aab", "Rose")),
    "lavender": (Swatch("#e6dcff", "#2c1d57", "#6d48d4", "Lavender"),
                 Swatch("#2f2846", "#e3dbff", "#ab92ff", "Lavender")),
    "sky":      (Swatch("#d3eaff", "#0f2f4f", "#1d6fc4", "Sky"),
                 Swatch("#1e3247", "#d5e9ff", "#6cb4ff", "Sky")),
    "mint":     (Swatch("#cdf3de", "#0f3a26", "#1c8a55", "Mint"),
                 Swatch("#1c3a2d", "#d0f3e0", "#5bd69a", "Mint")),
    "graphite": (Swatch("#e8e9ed", "#1f2228", "#4a5260", "Graphite"),
                 Swatch("#26282d", "#e6e7eb", "#a6afbf", "Graphite")),
}


def swatch(color: str, dark: bool) -> Swatch:
    pair = PALETTE.get(color, PALETTE["yellow"])
    return pair[1] if dark else pair[0]


def rgba(hex_color: str, alpha: float) -> str:
    h = hex_color.lstrip("#")
    r, g, b = (int(h[i:i + 2], 16) for i in (0, 2, 4))
    return f"rgba({r},{g},{b},{alpha})"


def palette_css() -> str:
    rules = []
    for name, pair in PALETTE.items():
        for sw, sel in ((pair[0], f"window.note.{name}"), (pair[1], f"window.note.dark.{name}")):
            rules.append(
                f"{sel} {{ --note-bg: {sw.bg}; --note-fg: {sw.fg}; --note-accent: {sw.accent};"
                f" --note-tint: {rgba(sw.fg, 0.07)}; --note-tint-strong: {rgba(sw.fg, 0.14)};"
                f" --note-dim: {rgba(sw.fg, 0.45)}; }}"
            )
        rules.append(f".swatch.{name} {{ background: {pair[0].bg}; }}")
        rules.append(f".swatch.dark.{name} {{ background: {pair[1].bg}; }}")
    rules.append(".ink-ink { background: var(--note-fg); }")
    rules += [f".ink-{c.lstrip('#')} {{ background: {c}; }}" for c in INK_COLORS if c != "ink"]
    return "\n".join(rules)


def load_css() -> None:
    from gi.repository import Gdk, Gtk

    display = Gdk.Display.get_default()
    for css in (palette_css(), (Path(__file__).parent / "style.css").read_text()):
        provider = Gtk.CssProvider()
        provider.load_from_string(css)
        Gtk.StyleContext.add_provider_for_display(
            display, provider, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION
        )
    icons = Gtk.IconTheme.get_for_display(display)
    icons.add_search_path(str(Path(__file__).parent / "icons"))
