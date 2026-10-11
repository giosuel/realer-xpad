"""Command-line entry point.

Kept free of GTK imports so ``--help`` and ``--version`` work without a
display, and so a missing PyGObject gives install hints instead of a traceback.
"""

from __future__ import annotations

import sys

from . import APP_NAME, __version__

COMMANDS = {
    "show": "Show all open notes (default)",
    "new": "Create a new note",
    "toggle": "Hide all notes, or show them if hidden",
    "hide": "Hide all notes",
    "manager": "Open the notes manager",
    "quit": "Save everything and quit",
}

MISSING_DEPS = """\
Realer Xpad needs PyGObject with GTK >= 4.16 and libadwaita >= 1.5:

  Debian / Ubuntu  sudo apt install python3-gi gir1.2-gtk-4.0 gir1.2-adw-1
  Arch             sudo pacman -S python-gobject gtk4 libadwaita
  Fedora           sudo dnf install python3-gobject gtk4 libadwaita
"""


def usage() -> str:
    lines = [f"Usage: realer-xpad [COMMAND]\n\n{APP_NAME} – sticky notes with Markdown and drawing.\n",
             "Commands:"]
    lines += [f"  {name:<9} {desc}" for name, desc in COMMANDS.items()]
    return "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> int:
    argv = argv if argv is not None else sys.argv
    if len(argv) > 1 and argv[1] in ("-h", "--help", "help"):
        print(usage(), end="")
        return 0
    if len(argv) > 1 and argv[1] in ("-v", "--version"):
        print(f"{APP_NAME} {__version__}")
        return 0
    try:
        from .app import App
    except (ImportError, ValueError) as err:  # ValueError: gi.require_version failed
        print(f"error: {err}\n\n{MISSING_DEPS}", file=sys.stderr, end="")
        return 1
    return App().run(argv)


if __name__ == "__main__":
    sys.exit(main())
