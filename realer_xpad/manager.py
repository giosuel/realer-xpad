"""Notes manager: search, reopen, show/hide and delete notes."""

from __future__ import annotations

import time

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gtk

from . import markdown


def _ago(ts: float) -> str:
    delta = max(0, time.time() - ts)
    for limit, unit in ((60, None), (3600, "min"), (86400, "h"), (86400 * 7, "d")):
        if delta < limit:
            if unit is None:
                return "just now"
            div = {"min": 60, "h": 3600, "d": 86400}[unit]
            return f"{int(delta // div)} {unit} ago"
    return time.strftime("%d %b %Y", time.localtime(ts))


def _preview(text: str) -> str:
    rendered = markdown.render(text).text
    lines = [ln.strip() for ln in rendered.splitlines() if ln.strip()]
    return " · ".join(lines[1:4])[:120]


class ManagerWindow(Adw.ApplicationWindow):
    def __init__(self, app):
        super().__init__(application=app, title="Realer Xpad", default_width=380, default_height=540)
        self.app = app
        self.add_css_class("manager")

        header = Adw.HeaderBar()
        new_btn = Gtk.Button(icon_name="list-add-symbolic", tooltip_text="New note", action_name="app.new-note")
        header.pack_start(new_btn)
        menu = Gtk.MenuButton(icon_name="open-menu-symbolic", menu_model=self._menu())
        header.pack_end(menu)

        self.search = Gtk.SearchEntry(placeholder_text="Search notes", hexpand=True,
                                      margin_start=12, margin_end=12, margin_top=6, margin_bottom=6)
        self.search.connect("search-changed", lambda *_: self.refresh())

        self.list = Gtk.ListBox(selection_mode=Gtk.SelectionMode.NONE, margin_start=12, margin_end=12,
                                margin_bottom=12, valign=Gtk.Align.START)
        self.list.add_css_class("boxed-list-separate")
        self.list.connect("row-activated", self._on_row_activated)

        self.empty = Adw.StatusPage(icon_name="rx-text-symbolic", title="No Notes",
                                    description="Create a note with the + button")
        self.stack = Gtk.Stack()
        self.stack.add_named(Gtk.ScrolledWindow(child=self.list, vexpand=True,
                                                hscrollbar_policy=Gtk.PolicyType.NEVER), "list")
        self.stack.add_named(self.empty, "empty")

        bottom = Gtk.Box(spacing=6, margin_start=12, margin_end=12, margin_top=6, margin_bottom=12,
                         homogeneous=True)
        bottom.append(Gtk.Button(label="Show All", action_name="app.show-all"))
        bottom.append(Gtk.Button(label="Hide All", action_name="app.hide-all"))

        content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        content.append(self.search)
        content.append(self.stack)
        view = Adw.ToolbarView(content=content)
        view.add_top_bar(header)
        view.add_bottom_bar(bottom)
        self.set_content(view)
        self.refresh()

    def _menu(self):
        from gi.repository import Gio

        menu = Gio.Menu()
        menu.append("Open Notes Folder", "app.open-folder")
        menu.append("About Realer Xpad", "app.about")
        menu.append("Quit", "app.quit")
        return menu

    def refresh(self) -> None:
        query = self.search.get_text().strip().lower()
        self.list.remove_all()
        notes = sorted(self.app.notes(), key=lambda n: n.updated, reverse=True)
        shown = [n for n in notes if not query or query in n.text.lower()]
        for note in shown:
            self.list.append(self._row(note))
        self.stack.set_visible_child_name("list" if shown else "empty")
        if notes and not shown:
            self.empty.set_title("No Matches")
            self.empty.set_description(None)
        else:
            self.empty.set_title("No Notes")
            self.empty.set_description("Create a note with the + button")

    def _row(self, note) -> Gtk.ListBoxRow:
        dot = Gtk.Box(valign=Gtk.Align.START, margin_top=4)
        dot.add_css_class("swatch")
        dot.add_css_class(note.color)
        if Adw.StyleManager.get_default().get_dark():
            dot.add_css_class("dark")

        title = Gtk.Label(label=note.title, xalign=0, ellipsize=3)
        title.add_css_class("card-title")
        texts = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2, hexpand=True)
        texts.append(title)
        if preview := _preview(note.text):
            label = Gtk.Label(label=preview, xalign=0, ellipsize=3)
            label.add_css_class("card-preview")
            texts.append(label)
        meta = "Hidden · " if not note.visible else ""
        meta += f"Edited {_ago(note.updated)}"
        if note.strokes:
            meta += " · ✎ drawing"
        meta_label = Gtk.Label(label=meta, xalign=0)
        meta_label.add_css_class("card-meta")
        texts.append(meta_label)

        eye = Gtk.Button(icon_name="view-reveal-symbolic" if note.visible else "view-conceal-symbolic",
                         tooltip_text="Hide" if note.visible else "Show", valign=Gtk.Align.CENTER)
        eye.add_css_class("flat")
        eye.connect("clicked", lambda *_: self.app.toggle_note(note.id))
        trash = Gtk.Button(icon_name="user-trash-symbolic", tooltip_text="Delete", valign=Gtk.Align.CENTER)
        trash.add_css_class("flat")
        trash.connect("clicked", lambda *_: self.app.window_for(note.id).confirm_delete())

        box = Gtk.Box(spacing=12)
        box.add_css_class("note-card")
        for w in (dot, texts, eye, trash):
            box.append(w)
        row = Gtk.ListBoxRow(child=box, activatable=True)
        row.note_id = note.id
        return row

    def _on_row_activated(self, _list, row) -> None:
        self.app.show_note(row.note_id)
