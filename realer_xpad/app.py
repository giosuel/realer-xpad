"""Application entry point: single instance, CLI commands, note lifecycle."""

from __future__ import annotations

import sys

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gio, GLib, Gtk

from . import APP_ID, APP_NAME, __version__
from .hypr import Hyprland
from .store import Note, NoteStore
from .theme import load_css

COMMANDS = {
    "show": "Show all open notes (default)",
    "new": "Create a new note",
    "toggle": "Hide all notes, or show them if hidden",
    "hide": "Hide all notes",
    "manager": "Open the notes manager",
    "quit": "Save everything and quit",
}
GEOMETRY_POLL_S = 3
PLACE_RETRIES = 20


class App(Adw.Application):
    def __init__(self, store: NoteStore | None = None, unique: bool = True, app_id: str | None = APP_ID):
        flags = Gio.ApplicationFlags.HANDLES_COMMAND_LINE
        if not unique:  # tests and screenshots run beside a real instance
            flags |= Gio.ApplicationFlags.NON_UNIQUE
        super().__init__(application_id=app_id, flags=flags)
        self._store = store
        self.store: NoteStore | None = None
        self.hypr: Hyprland | None = None
        self.windows: dict[str, "NoteWindow"] = {}
        self._notes: dict[str, Note] = {}
        self.manager = None
        self._started = False

    # -- lifecycle ------------------------------------------------------

    def do_startup(self) -> None:
        Adw.Application.do_startup(self)
        load_css()
        self.store = self._store or NoteStore()
        self.hypr = Hyprland.detect()
        self.load_notes(self.store)
        for name, callback, accel in (
            ("new-note", lambda *_: self.new_note(), "<Ctrl>n"),
            ("show-all", lambda *_: self.show_all(), None),
            ("hide-all", lambda *_: self.hide_all(), None),
            ("manager", lambda *_: self.open_manager(), None),
            ("open-folder", lambda *_: self._open_folder(), None),
            ("about", lambda *_: self._about(), None),
            ("quit", lambda *_: self.quit_app(), "<Ctrl>q"),
        ):
            action = Gio.SimpleAction.new(name, None)
            action.connect("activate", callback)
            self.add_action(action)
            if accel:
                self.set_accels_for_action(f"app.{name}", [accel])
        if self.hypr:
            GLib.timeout_add_seconds(GEOMETRY_POLL_S, self._poll_geometry)

    def do_command_line(self, command_line: Gio.ApplicationCommandLine) -> int:
        args = command_line.get_arguments()[1:]
        cmd = args[0] if args else "show"
        if cmd in ("-h", "--help", "help"):
            command_line.print_literal(_usage())
            return 0
        if cmd in ("-v", "--version"):
            command_line.print_literal(f"{APP_NAME} {__version__}\n")
            return 0
        if cmd not in COMMANDS:
            command_line.printerr_literal(f"Unknown command: {cmd}\n\n{_usage()}")
            return 1
        first = not self._started
        self._started = True
        self.run_command(cmd, first)
        return 0

    def run_command(self, cmd: str, first: bool = False) -> None:
        if cmd == "new":
            self.new_note()
        elif cmd == "toggle":
            self.hide_all() if self.any_visible() else self.show_all()
        elif cmd == "hide":
            self.hide_all()
        elif cmd == "manager":
            self.open_manager()
        elif cmd == "quit":
            self.quit_app()
        else:
            self.show_all()
            if not self.any_visible():
                if self._notes:
                    self.open_manager()
                else:
                    self.new_note()
        if first and not self.get_windows():
            self.quit()

    def quit_app(self) -> None:
        for win in self.windows.values():
            if win.get_visible():
                self.sync_geometry(win)
            win.save_now()
        self.quit()

    # -- notes ----------------------------------------------------------

    def load_notes(self, store: NoteStore) -> None:
        """(Re)load all notes from ``store``, closing any open windows."""
        for win in list(self.windows.values()):
            win.destroy()
        self.windows.clear()
        self.store = store
        self._notes = {n.id: n for n in store.load_all()}

    def notes(self) -> list[Note]:
        return list(self._notes.values())

    def window_for(self, note_id: str):
        from .note_window import NoteWindow

        win = self.windows.get(note_id)
        if win is None:
            win = NoteWindow(self, self._notes[note_id])
            win.connect("map", self._on_note_mapped)
            self.windows[note_id] = win
        return win

    def new_note(self) -> None:
        note = Note()
        if self.windows:  # inherit the colour of the most recently used note
            last = max(self._notes.values(), key=lambda n: n.updated)
            note.color = last.color
        self._notes[note.id] = note
        self.store.save(note)
        win = self.window_for(note.id)
        win.show_note()
        win.editor.show_edit()
        self._changed()

    def show_note(self, note_id: str) -> None:
        self.window_for(note_id).show_note()
        self._changed()

    def toggle_note(self, note_id: str) -> None:
        note = self._notes[note_id]
        if note.visible:
            self.window_for(note_id).hide_note()
        else:
            self.window_for(note_id).show_note()
        self._changed()

    def show_all(self) -> None:
        for note in self.notes():
            if note.visible:
                self.window_for(note.id).present()

    def hide_all(self) -> None:
        # Temporarily hide windows; notes stay "open" for the next show/toggle.
        for win in self.windows.values():
            if win.get_visible():
                self.sync_geometry(win)
                win.save_now()
                win.set_visible(False)

    def any_visible(self) -> bool:
        return any(w.get_visible() for w in self.windows.values())

    def delete_note(self, win) -> None:
        note = win.note
        self.store.delete(note)
        self._notes.pop(note.id, None)
        self.windows.pop(note.id, None)
        win.destroy()
        self._changed()

    def save_note(self, note: Note, touch: bool = True) -> None:
        if note.id in self._notes:
            self.store.save(note, touch=touch)
            self._changed()

    def _changed(self) -> None:
        if self.manager is not None:
            self.manager.refresh()

    # -- manager & misc -------------------------------------------------

    def open_manager(self) -> None:
        from .manager import ManagerWindow

        if self.manager is None:
            self.manager = ManagerWindow(self)
            self.manager.connect("destroy", lambda *_: setattr(self, "manager", None))
        self.manager.refresh()
        self.manager.present()

    def _open_folder(self) -> None:
        Gtk.FileLauncher.new(Gio.File.new_for_path(str(self.store.dir))).launch(self.manager, None, None)

    def _about(self) -> None:
        about = Adw.AboutDialog(application_name=APP_NAME, version=__version__, developer_name="giosuel",
                                application_icon="rx-text-symbolic", license_type=Gtk.License.AGPL_3_0,
                                comments="Sticky notes with Markdown and drawing")
        about.present(self.manager or self.get_active_window())

    # -- Hyprland -------------------------------------------------------

    def _on_note_mapped(self, win) -> None:
        if self.hypr is None:
            return
        attempts = [0]

        def place() -> bool:
            client = self.hypr.find(win.initial_title)
            attempts[0] += 1
            if client is None:
                return attempts[0] < PLACE_RETRIES
            note = win.note
            self.hypr.place(client["address"], note.x, note.y, note.width, note.height)
            if note.pinned:
                self.hypr.set_pinned({**client, "floating": True}, True)
            win.set_title(note.title)
            return False

        GLib.timeout_add(50, place)

    def apply_pin(self, win) -> None:
        if self.hypr and (client := self.hypr.find(win.initial_title)):
            self.hypr.set_pinned(client, win.note.pinned)

    def sync_geometry(self, win) -> bool:
        """Copy the window's compositor geometry into its note. Returns True if changed."""
        if self.hypr is None:
            return False
        client = self.hypr.find(win.initial_title)
        if client is None:
            return False
        x, y, w, h = self.hypr.geometry(client)
        note = win.note
        if (note.x, note.y, note.width, note.height) == (x, y, w, h):
            return False
        note.x, note.y, note.width, note.height = x, y, w, h
        return True

    def _poll_geometry(self) -> bool:
        for win in self.windows.values():
            if win.get_visible() and self.sync_geometry(win):
                self.store.save(win.note, touch=False)
        return GLib.SOURCE_CONTINUE


def _usage() -> str:
    lines = [f"Usage: realer-xpad [COMMAND]\n\n{APP_NAME} – sticky notes with Markdown and drawing.\n",
             "Commands:"]
    lines += [f"  {name:<9} {desc}" for name, desc in COMMANDS.items()]
    return "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> int:
    return App().run(argv if argv is not None else sys.argv)


if __name__ == "__main__":
    sys.exit(main())
