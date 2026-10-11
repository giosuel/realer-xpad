"""Markdown editor: a live-styled edit view plus a clean rendered read view."""

from __future__ import annotations

import gi

gi.require_version("Gtk", "4.0")
from gi.repository import Gdk, GLib, GObject, Gtk, Pango

from . import markdown
from .theme import Swatch, rgba

MARGINS = dict(left_margin=16, right_margin=16, top_margin=2, bottom_margin=16)


def _create_tags(buffer: Gtk.TextBuffer) -> None:
    # Creation order is priority order: later tags win on conflicts.
    defs = {
        "h1": dict(scale=1.55, weight=800, pixels_above_lines=4, pixels_below_lines=2),
        "h2": dict(scale=1.3, weight=750, pixels_above_lines=3, pixels_below_lines=1),
        "h3": dict(scale=1.12, weight=700, pixels_above_lines=2),
        "bold": dict(weight=700),
        "italic": dict(style=Pango.Style.ITALIC),
        "strike": dict(strikethrough=True),
        "quote": dict(style=Pango.Style.ITALIC, left_margin=MARGINS["left_margin"] + 12),
        "done": dict(strikethrough=True),
        "code": dict(family="monospace", scale=0.92),
        "code_block": dict(family="monospace", scale=0.92),
        "link": dict(underline=Pango.Underline.SINGLE),
        "marker": dict(),
        "fence": dict(family="monospace", scale=0.85),
        "hr": dict(),
        "bullet": dict(weight=700),
        "checkbox": dict(weight=700),
    }
    for name, props in defs.items():
        buffer.create_tag(name, **props)


def _apply_palette(buffer: Gtk.TextBuffer, sw: Swatch) -> None:
    table = buffer.get_tag_table()
    dim, faint, tint = rgba(sw.fg, 0.5), rgba(sw.fg, 0.32), rgba(sw.fg, 0.08)
    for name, prop, value in (
        ("marker", "foreground", faint),
        ("fence", "foreground", faint),
        ("hr", "foreground", faint),
        ("done", "foreground", dim),
        ("quote", "foreground", rgba(sw.fg, 0.75)),
        ("code", "background", tint),
        ("code_block", "paragraph-background", tint),
        ("link", "foreground", sw.accent),
        ("bullet", "foreground", sw.accent),
        ("checkbox", "foreground", sw.accent),
    ):
        table.lookup(name).set_property(prop, value)


def _apply_spans(buffer: Gtk.TextBuffer, spans, markers=()) -> None:
    start, end = buffer.get_bounds()
    buffer.remove_all_tags(start, end)

    def tag(name, a, b):
        buffer.apply_tag_by_name(name, buffer.get_iter_at_offset(a), buffer.get_iter_at_offset(b))

    for sp in spans:
        tag(sp.tag, sp.start, sp.end)
    for mk in markers:
        tag(mk.tag if mk.tag in ("checkbox", "bullet", "fence", "hr") else "marker", mk.start, mk.end)


def _text_view(**props) -> Gtk.TextView:
    view = Gtk.TextView(wrap_mode=Gtk.WrapMode.WORD_CHAR, **MARGINS, **props)
    view.add_css_class("note-text")
    _create_tags(view.get_buffer())
    return view


def _scrolled(child: Gtk.Widget) -> Gtk.ScrolledWindow:
    return Gtk.ScrolledWindow(child=child, hscrollbar_policy=Gtk.PolicyType.NEVER, vexpand=True)


class MarkdownEditor(Gtk.Stack):
    """Stack with an ``edit`` page (source, styled live) and a ``read`` page."""

    __gsignals__ = {"changed": (GObject.SignalFlags.RUN_LAST, None, ())}

    def __init__(self):
        super().__init__(transition_type=Gtk.StackTransitionType.CROSSFADE, transition_duration=120)
        self.edit_view = _text_view()
        self.read_view = _text_view(editable=False, cursor_visible=False)
        self.buffer = self.edit_view.get_buffer()
        self._parsed = markdown.Parsed()
        self._rendered: markdown.Rendered | None = None
        self._pending = 0

        self.add_named(_scrolled(self.edit_view), "edit")
        self.add_named(_scrolled(self.read_view), "read")

        self.buffer.connect("changed", self._on_changed)

        keys = Gtk.EventControllerKey()
        keys.connect("key-pressed", self._on_key)
        self.edit_view.add_controller(keys)

        for view, handler in ((self.edit_view, self._on_edit_click), (self.read_view, self._on_read_click)):
            click = Gtk.GestureClick()
            click.connect("released", handler)
            view.add_controller(click)

        self.read_view.set_cursor_from_name("default")
        motion = Gtk.EventControllerMotion()
        motion.connect("motion", self._on_read_motion)
        self.read_view.add_controller(motion)

    # -- public API -----------------------------------------------------

    @property
    def text(self) -> str:
        start, end = self.buffer.get_bounds()
        return self.buffer.get_text(start, end, True)

    def set_text(self, text: str) -> None:
        self.buffer.begin_irreversible_action()
        self.buffer.set_text(text)
        self.buffer.end_irreversible_action()
        self._refresh()

    def set_swatch(self, sw: Swatch) -> None:
        for view in (self.edit_view, self.read_view):
            _apply_palette(view.get_buffer(), sw)

    @property
    def reading(self) -> bool:
        return self.get_visible_child_name() == "read"

    def show_read(self) -> None:
        self._refresh()
        self.set_visible_child_name("read")

    def show_edit(self, source_offset: int | None = None) -> None:
        self.set_visible_child_name("edit")
        if source_offset is not None:
            self.buffer.place_cursor(self.buffer.get_iter_at_offset(source_offset))
        self.edit_view.grab_focus()

    def wrap_selection(self, marker: str) -> None:
        """Wrap the selection (or insert an empty pair) with ``marker``."""
        buf = self.buffer
        buf.begin_user_action()
        if buf.get_has_selection():
            start, end = buf.get_selection_bounds()
            a, b = start.get_offset(), end.get_offset()
            buf.insert(buf.get_iter_at_offset(b), marker)
            buf.insert(buf.get_iter_at_offset(a), marker)
            buf.select_range(buf.get_iter_at_offset(a + len(marker)), buf.get_iter_at_offset(b + len(marker)))
        else:
            buf.insert_at_cursor(marker * 2)
            it = buf.get_iter_at_mark(buf.get_insert())
            it.backward_chars(len(marker))
            buf.place_cursor(it)
        buf.end_user_action()

    def toggle_checkbox(self, index: int) -> None:
        cb = self._parsed.checkboxes[index]
        buf = self.buffer
        buf.begin_user_action()
        at = buf.get_iter_at_offset(cb.start + 1)
        end = buf.get_iter_at_offset(cb.start + 2)
        buf.delete(at, end)
        buf.insert(buf.get_iter_at_offset(cb.start + 1), " " if cb.checked else "x")
        buf.end_user_action()

    # -- internals ------------------------------------------------------

    def _on_changed(self, _buffer) -> None:
        # Coalesce bursts of edits (e.g. paste) into one restyle.
        if not self._pending:
            self._pending = GLib.idle_add(self._idle_refresh)
        self.emit("changed")

    def _idle_refresh(self) -> bool:
        self._pending = 0
        self._refresh()
        return GLib.SOURCE_REMOVE

    def _refresh(self) -> None:
        if self._pending:
            GLib.source_remove(self._pending)
            self._pending = 0
        text = self.text
        self._parsed = markdown.parse(text)
        _apply_spans(self.buffer, self._parsed.spans, self._parsed.markers)
        self._rendered = markdown.render(text, self._parsed)
        read_buf = self.read_view.get_buffer()
        read_buf.set_text(self._rendered.text)
        _apply_spans(read_buf, self._rendered.spans)

    def _offset_at(self, view: Gtk.TextView, x: float, y: float) -> int | None:
        bx, by = view.window_to_buffer_coords(Gtk.TextWindowType.WIDGET, int(x), int(y))
        ok, it = view.get_iter_at_location(bx, by)
        return it.get_offset() if ok else None

    def _read_target(self, x: float, y: float):
        offset = self._offset_at(self.read_view, x, y)
        if offset is None or not self._rendered:
            return None, None
        for a, b, idx in self._rendered.checkboxes:
            if a <= offset < b:
                return "checkbox", idx
        for sp in self._rendered.spans:
            if sp.tag == "link" and sp.start <= offset < sp.end:
                return "link", sp.data
        return "text", offset

    def _on_read_motion(self, _ctl, x, y) -> None:
        kind, _ = self._read_target(x, y)
        self.read_view.set_cursor_from_name("pointer" if kind in ("checkbox", "link") else "text")

    def _on_read_click(self, gesture, n_press, x, y) -> None:
        if gesture.get_current_button() != Gdk.BUTTON_PRIMARY:
            return
        kind, value = self._read_target(x, y)
        if kind == "checkbox":
            self.toggle_checkbox(value)
            self._refresh()
        elif kind == "link":
            _open_uri(value, self.get_root())
        else:
            offset = self._rendered.to_source(value) if kind == "text" else len(self.text)
            self.show_edit(offset)

    def _on_edit_click(self, gesture, n_press, x, y) -> None:
        if n_press != 1 or self.buffer.get_has_selection():
            return
        offset = self._offset_at(self.edit_view, x, y)
        if offset is None:
            return
        for i, cb in enumerate(self._parsed.checkboxes):
            if cb.start <= offset < cb.end:
                self.toggle_checkbox(i)
                return
        ctrl = gesture.get_current_event_state() & Gdk.ModifierType.CONTROL_MASK
        if ctrl:
            for sp in self._parsed.spans:
                if sp.tag == "link" and sp.start <= offset < sp.end:
                    _open_uri(sp.data, self.get_root())
                    return

    def _on_key(self, _ctl, keyval, _keycode, state) -> bool:
        ctrl = state & Gdk.ModifierType.CONTROL_MASK
        if ctrl and keyval in (Gdk.KEY_b, Gdk.KEY_B):
            self.wrap_selection("**")
            return True
        if ctrl and keyval in (Gdk.KEY_i, Gdk.KEY_I):
            self.wrap_selection("*")
            return True
        if keyval in (Gdk.KEY_Return, Gdk.KEY_KP_Enter) and not state & Gdk.ModifierType.SHIFT_MASK:
            return self._continue_list()
        if keyval in (Gdk.KEY_Tab, Gdk.KEY_ISO_Left_Tab):
            return self._indent(outdent=keyval == Gdk.KEY_ISO_Left_Tab or bool(state & Gdk.ModifierType.SHIFT_MASK))
        return False

    def _current_line(self) -> tuple[Gtk.TextIter, Gtk.TextIter]:
        cursor = self.buffer.get_iter_at_mark(self.buffer.get_insert())
        start = cursor.copy()
        start.set_line_offset(0)
        return start, cursor

    def _continue_list(self) -> bool:
        start, cursor = self._current_line()
        if not cursor.ends_line():
            return False
        prefix = markdown.continue_list(self.buffer.get_text(start, cursor, True))
        if prefix is None:
            return False
        buf = self.buffer
        buf.begin_user_action()
        if prefix == "":  # empty item: end the list
            buf.delete(start, cursor)
        else:
            buf.insert_at_cursor("\n" + prefix)
        buf.end_user_action()
        return True

    def _indent(self, outdent: bool) -> bool:
        start, cursor = self._current_line()
        line_end = start.copy()
        line_end.forward_to_line_end()
        line = self.buffer.get_text(start, line_end, True)
        if markdown.continue_list(line) is None and markdown.continue_list(line + "x") is None:
            return False  # not a list item: let Tab behave normally
        buf = self.buffer
        buf.begin_user_action()
        if outdent:
            n = len(line) - len(line.lstrip(" "))
            if n:
                buf.delete(start, buf.get_iter_at_offset(start.get_offset() + min(2, n)))
        else:
            buf.insert(start, "  ")
        buf.end_user_action()
        return True


def _open_uri(uri: str, parent) -> None:
    Gtk.UriLauncher.new(uri).launch(parent if isinstance(parent, Gtk.Window) else None, None, None)
