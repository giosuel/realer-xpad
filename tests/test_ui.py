"""Widget-level tests. They need a display: run via ``tools/headless.sh pytest``."""

import os

import pytest

if not (os.environ.get("WAYLAND_DISPLAY") or os.environ.get("DISPLAY")):
    pytest.skip("no display (run with tools/headless.sh)", allow_module_level=True)

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
gi.require_version("Graphene", "1.0")
from gi.repository import GLib, Graphene, Gtk

from realer_xpad.app import App
from realer_xpad.drawing import Stroke
from realer_xpad.md_editor import MarkdownEditor
from realer_xpad.store import Note, NoteStore


def pump(ms=50):
    """Run the main loop for a little while."""
    ctx = GLib.MainContext.default()
    done = []
    GLib.timeout_add(ms, lambda: done.append(1))
    while not done:
        ctx.iteration(True)


def tags_at(buffer, offset):
    return {t.props.name for t in buffer.get_iter_at_offset(offset).get_tags()}


@pytest.fixture(scope="session")
def shared_app():
    # A GApplication can only be registered once per process, so all tests
    # share one instance and get a fresh store each.
    os.environ["REALER_XPAD_NO_HYPR"] = "1"
    app = App(unique=False, app_id=None)
    app.register(None)  # runs startup
    return app


@pytest.fixture
def app(shared_app, tmp_path):
    shared_app.load_notes(NoteStore(tmp_path / "notes"))
    yield shared_app
    if shared_app.manager:
        shared_app.manager.destroy()
    shared_app.load_notes(NoteStore(tmp_path / "notes"))
    pump()


# -- editor ----------------------------------------------------------------

def test_editor_styles_source_live():
    ed = MarkdownEditor()
    ed.set_text("# Title\nsome **bold** text")
    assert "h1" in tags_at(ed.buffer, 3)
    assert "marker" in tags_at(ed.buffer, 0)
    bold = ed.text.index("bold")
    assert "bold" in tags_at(ed.buffer, bold)


def test_editor_restyles_after_typing():
    ed = MarkdownEditor()
    ed.set_text("hello")
    ed.buffer.insert(ed.buffer.get_start_iter(), "## ")
    pump()
    assert "h2" in tags_at(ed.buffer, 4)


def test_read_view_renders_clean_text():
    ed = MarkdownEditor()
    ed.set_text("- [ ] task\n**b**")
    ed.show_read()
    buf = ed.read_view.get_buffer()
    assert buf.get_text(*buf.get_bounds(), True) == "☐ task\nb"


def test_toggle_checkbox_is_undoable():
    ed = MarkdownEditor()
    ed.set_text("- [ ] a")
    ed.toggle_checkbox(0)
    assert ed.text == "- [x] a"
    ed.buffer.undo()
    assert ed.text == "- [ ] a"


def test_list_continuation_and_exit():
    ed = MarkdownEditor()
    ed.set_text("- one")
    ed.buffer.place_cursor(ed.buffer.get_end_iter())
    assert ed._continue_list()
    assert ed.text == "- one\n- "
    assert ed._continue_list()  # Enter on the empty item ends the list
    assert ed.text == "- one\n"


def test_wrap_selection_bold():
    ed = MarkdownEditor()
    ed.set_text("make this bold")
    b = ed.buffer
    b.select_range(b.get_iter_at_offset(10), b.get_iter_at_offset(14))
    ed.wrap_selection("**")
    assert ed.text == "make this **bold**"


def test_indent_list_item():
    ed = MarkdownEditor()
    ed.set_text("- item")
    ed.buffer.place_cursor(ed.buffer.get_end_iter())
    assert ed._indent(outdent=False)
    assert ed.text == "  - item"
    assert ed._indent(outdent=True)
    assert ed.text == "- item"
    ed.set_text("plain")
    assert not ed._indent(outdent=False)


# -- note windows ----------------------------------------------------------

def test_new_note_is_saved_and_shown(app):
    app.new_note()
    pump()
    (note,) = app.notes()
    win = app.windows[note.id]
    assert win.get_visible()
    assert app.store.load(note.id) is not None


def test_typing_saves_text(app):
    app.new_note()
    win = next(iter(app.windows.values()))
    win.editor.buffer.set_text("# Saved")
    win.save_now()
    assert app.store.load(win.note.id).text == "# Saved"
    assert win.get_title() == "Saved"


def test_color_change(app):
    app.new_note()
    win = next(iter(app.windows.values()))
    win._swatch_buttons["mint"].set_active(True)
    assert win.note.color == "mint"
    assert win.has_css_class("mint") and not win.has_css_class("yellow")


def test_draw_mode_and_strokes(app):
    app.new_note()
    win = next(iter(app.windows.values()))
    assert not win.canvas.get_can_target()
    win.mode_btn.set_active(True)
    assert win.note.mode == "draw" and win.canvas.get_can_target()
    assert win.draw_bar.get_reveal_child()

    win.drawing.add(Stroke("pen", "ink", 2.0, [(0, 0, 1), (20, 20, 0.5), (40, 0, 1)]))
    win.canvas.emit("changed")
    assert len(win.note.strokes) == 1 and win.undo_btn.get_sensitive()
    win.canvas.undo()
    assert win.note.strokes == [] and win.redo_btn.get_sensitive()

    win.mode_btn.set_active(False)
    assert win.note.mode == "text" and not win.canvas.get_can_target()


def test_canvas_renders(app):
    app.new_note()
    win = next(iter(app.windows.values()))
    win.drawing.add(Stroke("highlighter", "#e5484d", 4.0, [(10, 10, 1), (100, 10, 1)]))
    win.drawing.add(Stroke("pen", "ink", 2.0, [(10, 40, 0.2), (50, 60, 0.9), (90, 40, 0.5)]))
    win.canvas.invalidate()
    pump(200)
    w, h = win.get_width(), win.get_height()
    snap = Gtk.Snapshot()
    Gtk.WidgetPaintable.new(win.canvas).snapshot(snap, w, h)
    texture = win.get_renderer().render_texture(snap.to_node(), Graphene.Rect().init(0, 0, w, h))
    assert texture.get_width() > 0


def test_hide_and_show(app):
    app.new_note()
    win = next(iter(app.windows.values()))
    win.hide_note()
    assert not win.get_visible() and not win.note.visible
    assert not app.store.load(win.note.id).visible
    app.toggle_note(win.note.id)
    assert win.get_visible() and win.note.visible


def test_toggle_command_hides_and_restores(app):
    app.new_note()
    app.new_note()
    app.run_command("toggle")
    assert not app.any_visible()
    assert all(n.visible for n in app.notes())  # still "open", just hidden
    app.run_command("toggle")
    assert all(w.get_visible() for w in app.windows.values())


def test_delete_empty_note_skips_confirmation(app):
    app.new_note()
    win = next(iter(app.windows.values()))
    note_id = win.note.id
    win.confirm_delete()
    assert app.notes() == []
    assert (app.store.dir / "trash" / f"{note_id}.json").exists()


def test_existing_notes_open_on_show(app, tmp_path):
    store = NoteStore(tmp_path / "existing")
    store.save(Note(text="open"))
    store.save(Note(text="closed", visible=False))
    app.load_notes(store)
    app.run_command("show")
    shown = {w.note.text for w in app.windows.values() if w.get_visible()}
    assert shown == {"open"}


def test_manager_lists_and_filters(app):
    app.new_note()
    app.new_note()
    a, b = app.windows.values()
    a.editor.buffer.set_text("# Groceries\n- milk")
    b.editor.buffer.set_text("# Work")
    app.open_manager()
    rows = lambda: [r for r in app.manager.list]  # noqa: E731
    assert len(rows()) == 2
    app.manager.search.set_text("milk")
    app.manager.refresh()
    assert [r.note_id for r in rows()] == [a.note.id]
    app.manager.search.set_text("nothing-matches")
    app.manager.refresh()
    assert app.manager.stack.get_visible_child_name() == "empty"
