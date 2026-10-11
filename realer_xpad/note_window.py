"""A single sticky note window."""

from __future__ import annotations

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gdk, Gio, GLib, Gtk

from .canvas import DrawingCanvas
from .drawing import INK_COLORS, TOOLS, WIDTHS, Drawing
from .md_editor import MarkdownEditor
from .store import COLORS, Note
from .theme import PALETTE, swatch

SAVE_DELAY_MS = 500
TOOL_ICONS = {"pen": "rx-pen-symbolic", "highlighter": "rx-highlighter-symbolic", "eraser": "rx-eraser-symbolic"}
TOOL_NAMES = {"pen": "Pen", "highlighter": "Highlighter", "eraser": "Eraser"}
INK_NAMES = {"ink": "Auto", "#e5484d": "Red", "#3e63dd": "Blue", "#30a46c": "Green",
             "#f76b15": "Orange", "#8e4ec6": "Purple"}


def _button(icon: str, tooltip: str, *, toggle=False, reveal=True, **kw) -> Gtk.Button:
    cls = Gtk.ToggleButton if toggle else Gtk.Button
    btn = cls(icon_name=icon, tooltip_text=tooltip, valign=Gtk.Align.CENTER, **kw)
    btn.add_css_class("note-btn")
    if reveal:
        btn.add_css_class("reveal")
    return btn


class NoteWindow(Adw.ApplicationWindow):
    def __init__(self, app, note: Note):
        super().__init__(application=app)
        self.app = app
        self.note = note
        self.drawing = Drawing.from_json(note.strokes)
        self._save_id = 0
        self._swatch_buttons: dict[str, Gtk.ToggleButton] = {}

        self.add_css_class("note")
        self.set_default_size(note.width, note.height)
        self.set_size_request(180, 140)
        # Unique initial title so the compositor (Hyprland) can identify us.
        self.set_title(self.initial_title)

        self._build()
        self.editor.set_text(note.text)
        self._apply_color()
        self._set_mode(note.mode, initial=True)
        if note.text.strip():
            self.editor.show_read()

        Adw.StyleManager.get_default().connect("notify::dark", lambda *_: self._apply_color())
        self.connect("close-request", self._on_close_request)
        self.connect("notify::is-active", self._on_active_changed)

        hover = Gtk.EventControllerMotion()
        hover.connect("enter", lambda *_: self.add_css_class("hover"))
        hover.connect("leave", lambda *_: self.remove_css_class("hover"))
        self.add_controller(hover)

        keys = Gtk.EventControllerKey(propagation_phase=Gtk.PropagationPhase.CAPTURE)
        keys.connect("key-pressed", self._on_key)
        self.add_controller(keys)

        self._install_actions()

    @property
    def initial_title(self) -> str:
        return f"realer-xpad-{self.note.id}"

    # -- UI construction ------------------------------------------------

    def _build(self) -> None:
        # Header: drag handle with controls that fade in on hover.
        start = Gtk.Box(spacing=2)
        new_btn = _button("list-add-symbolic", "New note (Ctrl+N)", action_name="app.new-note")
        start.append(new_btn)
        self.color_btn = Gtk.MenuButton(icon_name="color-select-symbolic", tooltip_text="Note colour",
                                        valign=Gtk.Align.CENTER, popover=self._color_popover())
        self.color_btn.add_css_class("note-btn")
        self.color_btn.add_css_class("reveal")
        start.append(self.color_btn)

        end = Gtk.Box(spacing=2)
        self.mode_btn = _button("rx-draw-symbolic", "Draw (Ctrl+D)", toggle=True)
        self.mode_btn.connect("toggled", lambda b: self._set_mode("draw" if b.get_active() else "text"))
        end.append(self.mode_btn)
        self.pin_btn = _button("view-pin-symbolic", "Keep on top, on every workspace", toggle=True,
                               visible=self.app.hypr is not None)
        self.pin_btn.set_active(self.note.pinned)
        self.pin_btn.connect("toggled", self._on_pin_toggled)
        end.append(self.pin_btn)
        menu_btn = Gtk.MenuButton(icon_name="view-more-symbolic", tooltip_text="Menu",
                                  valign=Gtk.Align.CENTER, menu_model=self._menu())
        menu_btn.add_css_class("note-btn")
        menu_btn.add_css_class("reveal")
        end.append(menu_btn)
        close_btn = _button("window-close-symbolic", "Close note (Ctrl+W)")
        close_btn.connect("clicked", lambda *_: self.hide_note())
        end.append(close_btn)

        header = Gtk.CenterBox(start_widget=start, end_widget=end)
        header.add_css_class("note-header")
        handle = Gtk.WindowHandle(child=header)

        # Body: text editor with the drawing layer on top.
        self.editor = MarkdownEditor()
        self.editor.connect("changed", self._on_text_changed)
        self.canvas = DrawingCanvas(self.drawing)
        self.canvas.connect("changed", self._on_drawing_changed)
        self.placeholder = Gtk.Label(label="Start typing… Markdown works",
                                     halign=Gtk.Align.START, valign=Gtk.Align.START, can_target=False)
        self.placeholder.add_css_class("placeholder")
        body = Gtk.Overlay(child=self.editor, vexpand=True)
        body.add_overlay(self.placeholder)
        body.add_overlay(self.canvas)
        self.draw_bar = Gtk.Revealer(child=self._draw_bar(), reveal_child=False, valign=Gtk.Align.END,
                                     transition_type=Gtk.RevealerTransitionType.CROSSFADE)
        body.add_overlay(self.draw_bar)

        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        box.append(handle)
        box.append(body)
        self.set_content(box)

    def _color_popover(self) -> Gtk.Popover:
        grid = Gtk.FlowBox(max_children_per_line=4, min_children_per_line=4, selection_mode=Gtk.SelectionMode.NONE,
                           column_spacing=6, row_spacing=6)
        grid.add_css_class("swatch-grid")
        group = None
        for name in COLORS:
            btn = Gtk.ToggleButton(tooltip_text=PALETTE[name][0].label, group=group)
            group = group or btn
            btn.add_css_class("swatch")
            btn.add_css_class(name)
            btn.connect("toggled", self._on_swatch_toggled, name)
            self._swatch_buttons[name] = btn
            grid.append(btn)
        return Gtk.Popover(child=grid)

    def _menu(self) -> Gio.Menu:
        menu = Gio.Menu()
        section = Gio.Menu()
        section.append("New Note", "app.new-note")
        section.append("All Notes", "app.manager")
        section.append("Hide All Notes", "app.hide-all")
        menu.append_section(None, section)
        section = Gio.Menu()
        section.append("Delete Note", "win.delete")
        menu.append_section(None, section)
        return menu

    def _draw_bar(self) -> Gtk.Widget:
        bar = Gtk.Box(spacing=2, halign=Gtk.Align.CENTER)
        bar.add_css_class("draw-bar")
        self.tool_buttons: dict[str, Gtk.ToggleButton] = {}
        group = None
        for tool in TOOLS:
            btn = _button(TOOL_ICONS[tool], TOOL_NAMES[tool], toggle=True, reveal=False, group=group)
            group = group or btn
            btn.connect("toggled", lambda b, t=tool: b.get_active() and self.canvas.set_tool(t))
            self.tool_buttons[tool] = btn
            bar.append(btn)
        self.tool_buttons["pen"].set_active(True)

        bar.append(Gtk.Separator(orientation=Gtk.Orientation.VERTICAL))
        self.ink_dot = Gtk.Box(halign=Gtk.Align.CENTER, valign=Gtk.Align.CENTER)
        self.ink_dot.add_css_class("ink-dot")
        self.ink_dot.add_css_class("ink-ink")
        ink_btn = Gtk.MenuButton(child=self.ink_dot, tooltip_text="Ink colour & size",
                                 valign=Gtk.Align.CENTER, popover=self._ink_popover())
        ink_btn.add_css_class("note-btn")
        bar.append(ink_btn)
        bar.append(Gtk.Separator(orientation=Gtk.Orientation.VERTICAL))

        self.undo_btn = _button("edit-undo-symbolic", "Undo (Ctrl+Z)", reveal=False)
        self.undo_btn.connect("clicked", lambda *_: self.canvas.undo())
        self.redo_btn = _button("edit-redo-symbolic", "Redo (Ctrl+Shift+Z)", reveal=False)
        self.redo_btn.connect("clicked", lambda *_: self.canvas.redo())
        bar.append(self.undo_btn)
        bar.append(self.redo_btn)
        self._update_undo_buttons()
        return bar

    def _ink_popover(self) -> Gtk.Popover:
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8, margin_top=6, margin_bottom=6,
                      margin_start=6, margin_end=6)
        colors = Gtk.Box(spacing=6)
        group = None
        for color in INK_COLORS:
            btn = Gtk.ToggleButton(tooltip_text=INK_NAMES.get(color, color), group=group)
            group = group or btn
            btn.add_css_class("ink")
            btn.add_css_class("ink-" + color.lstrip("#"))
            btn.connect("toggled", lambda b, c=color: b.get_active() and self._set_ink(c))
            colors.append(btn)
        group.set_active(True)
        box.append(colors)

        sizes = Gtk.Box(spacing=6, homogeneous=True)
        group = None
        for width, label in zip(WIDTHS, ("Fine", "Medium", "Bold")):
            btn = Gtk.ToggleButton(label=label, group=group)
            group = group or btn
            btn.connect("toggled", lambda b, w=width: b.get_active() and setattr(self.canvas, "width", w))
            btn.set_active(width == self.canvas.width)
            sizes.append(btn)
        box.append(sizes)

        clear = Gtk.Button(label="Clear Drawing")
        clear.add_css_class("destructive-action")
        clear.connect("clicked", lambda *_: self.canvas.clear())
        box.append(clear)
        return Gtk.Popover(child=box)

    def _install_actions(self) -> None:
        for name, callback in (
            ("delete", lambda *_: self.confirm_delete()),
            ("close", lambda *_: self.hide_note()),
            ("toggle-draw", lambda *_: self.mode_btn.set_active(not self.mode_btn.get_active())),
            ("toggle-read", lambda *_: self._toggle_read()),
        ):
            action = Gio.SimpleAction.new(name, None)
            action.connect("activate", callback)
            self.add_action(action)

    # -- colours --------------------------------------------------------

    def _apply_color(self) -> None:
        dark = Adw.StyleManager.get_default().get_dark()
        for name in COLORS:
            self.remove_css_class(name)
        self.add_css_class(self.note.color)
        if dark:
            self.add_css_class("dark")
        else:
            self.remove_css_class("dark")
        for btn in self._swatch_buttons.values():
            btn.add_css_class("dark") if dark else btn.remove_css_class("dark")
        sw = swatch(self.note.color, dark)
        self.editor.set_swatch(sw)
        self._swatch_buttons[self.note.color].set_active(True)
        self.canvas.invalidate()

    def _set_ink(self, color: str) -> None:
        self.ink_dot.remove_css_class("ink-" + self.canvas.color.lstrip("#"))
        self.canvas.color = color
        self.ink_dot.add_css_class("ink-" + color.lstrip("#"))

    def _on_swatch_toggled(self, btn: Gtk.ToggleButton, name: str) -> None:
        if btn.get_active() and name != self.note.color:
            self.note.color = name
            self._apply_color()
            self.schedule_save()

    # -- modes ----------------------------------------------------------

    def _set_mode(self, mode: str, initial: bool = False) -> None:
        drawing = mode == "draw"
        self.note.mode = mode
        if self.mode_btn.get_active() != drawing:
            self.mode_btn.set_active(drawing)
        self.mode_btn.set_icon_name("rx-text-symbolic" if drawing else "rx-draw-symbolic")
        self.mode_btn.set_tooltip_text("Back to text (Ctrl+D)" if drawing else "Draw (Ctrl+D)")
        self.canvas.set_can_target(drawing)
        self.draw_bar.set_reveal_child(drawing)
        if drawing:
            self.editor.show_read()
        elif not initial:
            self.editor.show_edit()
        self._update_placeholder()
        if not initial:
            self.schedule_save()

    def _toggle_read(self) -> None:
        if self.note.mode == "draw":
            return
        if self.editor.reading:
            self.editor.show_edit()
        else:
            self.editor.show_read()

    def _update_placeholder(self) -> None:
        self.placeholder.set_visible(not self.note.text and self.note.mode == "text" and not self.drawing.strokes)

    # -- events ---------------------------------------------------------

    def _on_text_changed(self, _editor) -> None:
        self.note.text = self.editor.text
        self._update_placeholder()
        self._update_title()
        self.schedule_save()

    def _on_drawing_changed(self, _canvas) -> None:
        self.note.strokes = self.drawing.to_json()
        self._update_undo_buttons()
        self._update_placeholder()
        self.schedule_save()

    def _update_undo_buttons(self) -> None:
        self.undo_btn.set_sensitive(self.drawing.can_undo)
        self.redo_btn.set_sensitive(self.drawing.can_redo)

    def _update_title(self) -> None:
        # Keep the initial title until the compositor has seen it.
        if self.app.hypr is None or self.get_mapped():
            self.set_title(self.note.title)

    def _on_active_changed(self, *_args) -> None:
        if not self.is_active() and self.note.mode == "text" and self.note.text.strip():
            self.editor.show_read()
        if not self.is_active():
            self.save_now()

    def _on_key(self, _ctl, keyval, _code, state) -> bool:
        ctrl = bool(state & Gdk.ModifierType.CONTROL_MASK)
        shift = bool(state & Gdk.ModifierType.SHIFT_MASK)
        if ctrl and keyval in (Gdk.KEY_e, Gdk.KEY_E):
            self._toggle_read()
            return True
        if ctrl and keyval in (Gdk.KEY_d, Gdk.KEY_D):
            self.mode_btn.set_active(not self.mode_btn.get_active())
            return True
        if ctrl and keyval in (Gdk.KEY_w, Gdk.KEY_W):
            self.hide_note()
            return True
        if self.note.mode == "draw":
            if ctrl and keyval in (Gdk.KEY_z, Gdk.KEY_Z):
                self.canvas.redo() if shift else self.canvas.undo()
                return True
            if ctrl and keyval in (Gdk.KEY_y, Gdk.KEY_Y):
                self.canvas.redo()
                return True
            if keyval == Gdk.KEY_Escape:
                self.mode_btn.set_active(False)
                return True
            shortcuts = {Gdk.KEY_p: "pen", Gdk.KEY_h: "highlighter", Gdk.KEY_e: "eraser"}
            if not ctrl and keyval in shortcuts:
                self.tool_buttons[shortcuts[keyval]].set_active(True)
                return True
        elif keyval == Gdk.KEY_Escape and not self.editor.reading and self.note.text.strip():
            self.editor.show_read()
            return True
        return False

    def _on_pin_toggled(self, btn: Gtk.ToggleButton) -> None:
        self.note.pinned = btn.get_active()
        self.app.apply_pin(self)
        self.schedule_save()

    def _on_close_request(self, *_args) -> bool:
        # Closing from the compositor (e.g. Super+Q) hides the note instead of
        # destroying it; it can be reopened from the notes manager.
        self.hide_note()
        return True

    # -- persistence & lifecycle ----------------------------------------

    def schedule_save(self) -> None:
        if self._save_id:
            GLib.source_remove(self._save_id)
        self._save_id = GLib.timeout_add(SAVE_DELAY_MS, self._save_timeout)

    def _save_timeout(self) -> bool:
        self._save_id = 0
        self.save_now()
        return GLib.SOURCE_REMOVE

    def save_now(self) -> None:
        if self._save_id:
            GLib.source_remove(self._save_id)
            self._save_id = 0
        if self.app.hypr is None and self.get_width() > 0:
            self.note.width, self.note.height = self.get_width(), self.get_height()
        self.app.save_note(self.note)

    def show_note(self) -> None:
        self.note.visible = True
        self.present()
        self.app.save_note(self.note, touch=False)

    def hide_note(self) -> None:
        self.app.sync_geometry(self)
        self.note.visible = False
        self.save_now()
        self.set_visible(False)

    def confirm_delete(self) -> None:
        if not self.note.text.strip() and not self.drawing.strokes:
            self.app.delete_note(self)
            return
        dialog = Adw.AlertDialog(heading="Delete Note?",
                                 body="The note will be moved to the trash folder.")
        dialog.add_response("cancel", "Cancel")
        dialog.add_response("delete", "Delete")
        dialog.set_response_appearance("delete", Adw.ResponseAppearance.DESTRUCTIVE)
        dialog.set_default_response("cancel")
        dialog.connect("response", lambda _d, r: r == "delete" and self.app.delete_note(self))
        dialog.present(self)
