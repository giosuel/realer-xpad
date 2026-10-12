//! Widget-level tests. They need a Wayland/X display: run via
//! `tools/headless.sh cargo test --test ui`. Without a display they are skipped.
//!
//! GTK may only be used from one thread, so this is a custom harness
//! (`harness = false`) that runs every test sequentially on the main thread.

use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::time::{Duration, Instant};

use adw::prelude::*;
use gtk::{gio, glib, graphene};
use realer_xpad::app::App;
use realer_xpad::drawing::Stroke;
use realer_xpad::editor::MarkdownEditor;
use realer_xpad::note_window::NoteWindow;
use realer_xpad::store::{Note, NoteStore};

/// Run the main loop for a little while.
fn pump(ms: u64) {
    let ctx = glib::MainContext::default();
    let until = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < until {
        ctx.iteration(false);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn tags_at(buffer: &gtk::TextBuffer, offset: i32) -> Vec<String> {
    buffer.iter_at_offset(offset).tags().iter().filter_map(|t| t.name()).map(String::from).collect()
}

fn read_text(ed: &MarkdownEditor) -> String {
    let buf = ed.read_view.buffer();
    let (a, b) = buf.bounds();
    buf.text(&a, &b, true).into()
}

/// Widget coordinates of the character at `offset` in a text view.
fn coords(view: &gtk::TextView, offset: i32) -> (f64, f64) {
    let rect = view.iter_location(&view.buffer().iter_at_offset(offset));
    let (x, y) = view.buffer_to_window_coords(gtk::TextWindowType::Widget, rect.x() + 2, rect.y() + rect.height() / 2);
    (x as f64, y as f64)
}

struct Ctx {
    app: App,
    _dir: tempfile::TempDir,
}

impl Ctx {
    fn new(app: &App) -> Self {
        let dir = tempfile::tempdir().unwrap();
        app.load_notes(NoteStore::new(dir.path()).unwrap());
        app.set_xpad_dir(&dir.path().join("no-xpad")); // never the real ~/.config/xpad
        Ctx { app: app.clone(), _dir: dir }
    }

    fn new_window(&self) -> NoteWindow {
        self.app.new_note();
        self.app.windows().last().unwrap().clone()
    }
}

// -- editor ------------------------------------------------------------------

fn editor_styles_source_live(_: &Ctx) {
    let ed = MarkdownEditor::new();
    ed.set_text("# Title\nsome **bold** text");
    assert!(tags_at(&ed.buffer, 3).contains(&"h1".into()));
    assert!(tags_at(&ed.buffer, 0).contains(&"marker".into()));
    let bold = ed.text().find("bold").unwrap() as i32;
    assert!(tags_at(&ed.buffer, bold).contains(&"bold".into()));
}

fn editor_restyles_after_typing(_: &Ctx) {
    let ed = MarkdownEditor::new();
    ed.set_text("hello");
    ed.buffer.insert(&mut ed.buffer.start_iter(), "## ");
    pump(50);
    assert!(tags_at(&ed.buffer, 4).contains(&"h2".into()));
}

fn read_view_renders_clean_text(_: &Ctx) {
    let ed = MarkdownEditor::new();
    ed.set_text("- [ ] task\n**b**");
    ed.show_read();
    assert_eq!(read_text(&ed), "☐ task\nb");
}

fn toggle_checkbox_is_undoable(_: &Ctx) {
    let ed = MarkdownEditor::new();
    ed.set_text("- [ ] a");
    ed.toggle_checkbox(0);
    assert_eq!(ed.text(), "- [x] a");
    ed.buffer.undo();
    assert_eq!(ed.text(), "- [ ] a");
}

fn list_continuation_and_exit(_: &Ctx) {
    let ed = MarkdownEditor::new();
    ed.set_text("- one");
    ed.buffer.place_cursor(&ed.buffer.end_iter());
    assert!(ed.continue_list());
    assert_eq!(ed.text(), "- one\n- ");
    assert!(ed.continue_list()); // Enter on the empty item ends the list
    assert_eq!(ed.text(), "- one\n");
}

fn wrap_selection_bold(_: &Ctx) {
    let ed = MarkdownEditor::new();
    ed.set_text("make this bold");
    ed.buffer.select_range(&ed.buffer.iter_at_offset(10), &ed.buffer.iter_at_offset(14));
    ed.wrap_selection("**");
    assert_eq!(ed.text(), "make this **bold**");
}

fn indent_list_item(_: &Ctx) {
    let ed = MarkdownEditor::new();
    ed.set_text("- item");
    ed.buffer.place_cursor(&ed.buffer.end_iter());
    assert!(ed.indent(false));
    assert_eq!(ed.text(), "  - item");
    assert!(ed.indent(true));
    assert_eq!(ed.text(), "- item");
    ed.set_text("plain");
    assert!(!ed.indent(false));
}

// -- note windows --------------------------------------------------------------

fn new_note_is_saved_and_shown(c: &Ctx) {
    let win = c.new_window();
    pump(50);
    assert!(win.window.is_visible());
    assert_eq!(c.app.notes().len(), 1);
    assert!(c.app.load_note_from_disk(&win.id()).is_some());
}

fn typing_saves_text(c: &Ctx) {
    let win = c.new_window();
    win.editor.buffer.set_text("# Saved");
    win.save_now();
    assert_eq!(c.app.load_note_from_disk(&win.id()).unwrap().text, "# Saved");
    assert_eq!(win.window.title().unwrap(), "Saved");
}

fn color_change(c: &Ctx) {
    let win = c.new_window();
    win.select_color("mint");
    assert_eq!(win.note.borrow().color, "mint");
    assert!(win.window.has_css_class("mint") && !win.window.has_css_class("yellow"));
}

fn tools_and_strokes(c: &Ctx) {
    let win = c.new_window();
    assert_eq!(win.tool(), "text", "typing is the default");
    assert!(!win.canvas.area.can_target() && win.tool_bar.is_visible());
    assert!(!win.ink_btn.is_sensitive(), "ink only matters for drawing");
    win.select_tool("pen");
    assert_eq!(win.mode(), "draw");
    assert!(win.canvas.area.can_target() && win.tool_bar.is_visible() && win.ink_btn.is_sensitive());

    win.canvas.begin(0.0, 0.0, 1.0, false);
    win.canvas.update(20.0, 20.0, 0.5);
    win.canvas.update(40.0, 0.0, 1.0);
    win.canvas.end();
    assert_eq!(win.note.borrow().strokes.len(), 1);
    assert!(win.undo_btn.is_sensitive());
    win.canvas.undo();
    assert!(win.note.borrow().strokes.is_empty() && win.redo_btn.is_sensitive());

    win.select_tool("text");
    assert_eq!(win.mode(), "text");
    assert!(!win.canvas.area.can_target() && win.tool_bar.is_visible());
}

fn shapes_are_drawn_by_dragging(c: &Ctx) {
    let win = c.new_window();
    win.select_tool("shape");
    assert_eq!(win.tool(), "shape");
    win.canvas.begin(10.0, 10.0, 1.0, false);
    win.canvas.update(30.0, 20.0, 1.0);
    win.canvas.update(80.0, 50.0, 1.0); // only the end point moves
    win.canvas.end();
    let stroke = win.note.borrow().strokes[0].clone();
    assert_eq!((stroke.tool.as_str(), stroke.points.len()), ("rect", 2), "rectangle by default");
    assert_eq!((stroke.points[1].0, stroke.points[1].1), (80.0, 50.0));

    win.shape_buttons.iter().find(|(s, _)| *s == "arrow").unwrap().1.emit_clicked();
    assert_eq!(win.tool(), "shape");
    win.canvas.begin(10.0, 80.0, 1.0, false);
    win.canvas.update(90.0, 80.0, 1.0);
    win.canvas.end();
    assert_eq!(win.note.borrow().strokes[1].tool, "arrow");

    win.canvas.begin(50.0, 50.0, 1.0, false); // a click, not a drag
    win.canvas.end();
    assert_eq!(win.note.borrow().strokes.len(), 2, "no tiny shapes from clicks");

    // Choosing a shape while typing switches to the shape tool.
    win.select_tool("text");
    win.select_tool("ellipse");
    assert_eq!(win.tool(), "shape");

    // Even when it's the shape already chosen, picked from the menu.
    win.select_tool("pen");
    win.shape_buttons.iter().find(|(s, _)| *s == "ellipse").unwrap().1.emit_clicked();
    assert_eq!(win.tool(), "shape");

    // An ellipse with no height would be invisible: it isn't added.
    win.canvas.begin(10.0, 10.0, 1.0, false);
    win.canvas.update(110.0, 11.0, 1.0);
    win.canvas.end();
    assert_eq!(win.note.borrow().strokes.len(), 2);
}

fn undo_follows_the_tool(c: &Ctx) {
    let win = c.new_window();
    win.editor.buffer.set_text("first");
    win.editor.buffer.insert(&mut win.editor.buffer.end_iter(), " second");
    pump(20);
    assert!(win.undo_btn.is_sensitive(), "text tool: undo edits the text");
    win.undo_btn.emit_clicked();
    assert_eq!(win.editor.text(), "first");
    win.select_tool("pen");
    assert!(!win.undo_btn.is_sensitive(), "pen: nothing drawn yet");
    win.canvas.begin(0.0, 0.0, 1.0, false);
    win.canvas.update(20.0, 20.0, 1.0);
    win.canvas.end();
    win.undo_btn.emit_clicked();
    assert!(win.note.borrow().strokes.is_empty());
    assert_eq!(win.editor.text(), "first", "and leaves the text alone");
}

fn tool_keys_only_while_drawing(c: &Ctx) {
    let win = c.new_window();
    use gtk::glib::translate::IntoGlib;
    let key = |k: gtk::gdk::Key, mods: gtk::gdk::ModifierType| {
        let keys = win.window.observe_controllers();
        let controller =
            (0..keys.n_items()).filter_map(|i| keys.item(i).and_downcast::<gtk::EventControllerKey>()).next().unwrap();
        controller.emit_by_name::<bool>("key-pressed", &[&k.into_glib(), &0u32, &mods])
    };
    assert!(!key(gtk::gdk::Key::p, gtk::gdk::ModifierType::empty()), "in the text tool, P types");
    assert_eq!(win.tool(), "text");
    assert!(key(gtk::gdk::Key::d, gtk::gdk::ModifierType::CONTROL_MASK));
    assert_eq!(win.tool(), "pen", "Ctrl+D draws");
    assert!(key(gtk::gdk::Key::s, gtk::gdk::ModifierType::empty()));
    assert_eq!(win.tool(), "shape");
    assert!(key(gtk::gdk::Key::Escape, gtk::gdk::ModifierType::empty()));
    assert_eq!(win.tool(), "text");
    assert!(key(gtk::gdk::Key::d, gtk::gdk::ModifierType::CONTROL_MASK));
    assert_eq!(win.tool(), "shape", "Ctrl+D returns to the last drawing tool");

    // With a dialog open, keys are the dialog's: Esc must not switch tools.
    win.editor.buffer.set_text("keep me");
    win.confirm_delete();
    pump(20);
    assert!(win.window.visible_dialog().is_some());
    assert!(!key(gtk::gdk::Key::Escape, gtk::gdk::ModifierType::empty()));
    assert!(!key(gtk::gdk::Key::w, gtk::gdk::ModifierType::CONTROL_MASK));
    assert_eq!(win.tool(), "shape");
    assert!(win.window.is_visible());
    win.window.visible_dialog().unwrap().close();
    pump(20);
}

fn note_saved_in_draw_mode_reopens_in_draw_mode(c: &Ctx) {
    let note = Note { mode: "draw".into(), ..Default::default() };
    let id = note.id.clone();
    let win = NoteWindow::new(&c.app, Rc::new(RefCell::new(note)));
    assert!(win.tool() == "pen" && win.canvas.area.can_target());
    assert_eq!(win.id(), id);
    win.window.destroy();
}

fn canvas_pointer_erasing(c: &Ctx) {
    let win = c.new_window();
    win.select_tool("pen");
    let canvas = &win.canvas;
    canvas.begin(10.0, 10.0, 1.0, false);
    for i in 1..=10 {
        canvas.update(10.0 + i as f64 * 8.0, 10.0 + i as f64 * 2.0, 1.0);
    }
    canvas.end();
    assert_eq!(win.note.borrow().strokes[0].points.len(), 11);

    win.select_tool("eraser");
    canvas.begin(50.0, 52.0, 1.0, false); // far from the stroke: nothing erased
    canvas.end();
    assert_eq!(win.note.borrow().strokes.len(), 1);
    canvas.begin(10.0, 10.0, 1.0, false);
    canvas.update(40.0, 10.0, 1.0);
    canvas.end();
    assert!(win.note.borrow().strokes.is_empty());
    canvas.undo();
    assert_eq!(win.note.borrow().strokes.len(), 1);

    win.select_tool("pen"); // the stylus' eraser tip erases with any tool
    canvas.begin(10.0, 10.0, 1.0, true);
    canvas.end();
    assert!(win.note.borrow().strokes.is_empty());
}

fn canvas_renders(c: &Ctx) {
    let win = c.new_window();
    win.canvas.drawing.borrow_mut().add(Stroke::new(
        "highlighter",
        "#e5484d",
        4.0,
        vec![(10., 10., 1.), (100., 10., 1.)],
    ));
    win.canvas.drawing.borrow_mut().add(Stroke::new(
        "pen",
        "ink",
        2.0,
        vec![(10., 40., 0.2), (50., 60., 0.9), (90., 40., 0.5)],
    ));
    win.canvas.area.queue_draw();
    pump(200);
    let (w, h) = (win.window.width() as f32, win.window.height() as f32);
    let snap = gtk::Snapshot::new();
    gtk::WidgetPaintable::new(Some(&win.canvas.area)).snapshot(&snap, w as f64, h as f64);
    let node = snap.to_node().expect("canvas produced render nodes");
    let texture = win.window.renderer().unwrap().render_texture(node, Some(&graphene::Rect::new(0., 0., w, h)));
    assert!(texture.width() > 0);
}

fn placeholder_lines_up_with_the_cursor(c: &Ctx) {
    let win = c.new_window();
    assert!(win.placeholder.is_visible() && !win.editor.reading());
    pump(150);
    // Where the cursor of the empty note is drawn vs. where the hint starts,
    // both in window coordinates.
    let view = &win.editor.edit_view;
    let rect = view.iter_location(&view.buffer().start_iter());
    let (bx, by) = view.buffer_to_window_coords(gtk::TextWindowType::Widget, rect.x(), rect.y());
    let caret = view.compute_point(&win.window, &graphene::Point::new(bx as f32, by as f32)).unwrap();
    let hint = win.placeholder.compute_point(&win.window, &graphene::Point::new(0.0, 0.0)).unwrap();
    assert!(
        (caret.x() - hint.x()).abs() <= 1.0 && (caret.y() - hint.y()).abs() <= 1.0,
        "cursor at {:?}, placeholder at {:?}",
        (caret.x(), caret.y()),
        (hint.x(), hint.y())
    );
}

fn bundled_icons_are_available(_: &Ctx) {
    let theme = gtk::IconTheme::for_display(&gtk::gdk::Display::default().unwrap());
    for icon in [
        "rx-pen-symbolic",
        "rx-highlighter-symbolic",
        "rx-eraser-symbolic",
        "rx-line-symbolic",
        "rx-arrow-symbolic",
        "rx-rect-symbolic",
        "rx-ellipse-symbolic",
        "rx-text-symbolic",
        "rx-pin-symbolic",
        realer_xpad::APP_ID,
    ] {
        assert!(theme.has_icon(icon), "missing icon {icon}");
    }
}

fn hide_and_show(c: &Ctx) {
    let win = c.new_window();
    win.hide_note();
    assert!(!win.window.is_visible() && !win.note.borrow().visible);
    assert!(!c.app.load_note_from_disk(&win.id()).unwrap().visible);
    c.app.toggle_note(&win.id());
    assert!(win.window.is_visible() && win.note.borrow().visible);
}

fn toggle_command_hides_and_restores(c: &Ctx) {
    c.app.new_note();
    c.app.new_note();
    c.app.run_command("toggle");
    assert!(!c.app.any_visible());
    assert!(c.app.notes().iter().all(|n| n.borrow().visible)); // still "open", just hidden
    c.app.run_command("toggle");
    assert!(c.app.windows().iter().all(|w| w.window.is_visible()));
}

fn manager_shows_notes_hidden_by_hide_all(c: &Ctx) {
    let win = c.new_window();
    win.editor.buffer.set_text("hidden for now");
    c.app.hide_all();
    c.app.open_manager();
    let m = c.app.manager().unwrap();
    let eye = m.toggle_button(&win.id()).unwrap();
    assert_eq!(eye.tooltip_text().as_deref(), Some("Show"), "not on screen, so the button shows it");
    eye.emit_clicked();
    pump(20);
    assert!(win.window.is_visible(), "shown, not hidden for good");
    assert!(win.note.borrow().visible);
    assert_eq!(m.toggle_button(&win.id()).unwrap().tooltip_text().as_deref(), Some("Hide"));
    m.window.close();
}

fn delete_empty_note_skips_confirmation(c: &Ctx) {
    let win = c.new_window();
    let id = win.id();
    win.confirm_delete();
    assert!(c.app.notes().is_empty());
    assert!(c.app.store_dir().join("trash").join(format!("{id}.json")).exists());
}

fn existing_notes_open_on_show(c: &Ctx) {
    let dir = tempfile::tempdir().unwrap();
    let store = NoteStore::new(dir.path()).unwrap();
    store.save(&mut Note { text: "open".into(), ..Default::default() }).unwrap();
    store.save(&mut Note { text: "closed".into(), visible: false, ..Default::default() }).unwrap();
    c.app.load_notes(store);
    c.app.run_command("show");
    let shown: Vec<String> =
        c.app.windows().iter().filter(|w| w.window.is_visible()).map(|w| w.note.borrow().text.clone()).collect();
    assert_eq!(shown, ["open"]);
}

fn read_view_checkbox_click(c: &Ctx) {
    let win = c.new_window();
    win.editor.set_text("- [ ] call mom\n- [ ] buy milk");
    win.editor.show_read();
    pump(200);
    let offset = read_text(&win.editor).find("buy").map(|b| read_text(&win.editor)[..b].chars().count()).unwrap();
    let (x, y) = coords(&win.editor.read_view, offset as i32 - 2); // the ☐ before "buy"
    win.editor.read_click(x, y);
    assert_eq!(win.editor.text(), "- [ ] call mom\n- [x] buy milk");
    assert!(win.editor.reading()); // toggling does not leave the read view
}

fn read_view_text_click_enters_edit_at_position(c: &Ctx) {
    let win = c.new_window();
    win.editor.set_text("**hello** world");
    win.editor.show_read();
    pump(200);
    let (x, y) = coords(&win.editor.read_view, 6); // "w"
    win.editor.read_click(x, y);
    assert!(!win.editor.reading());
    let cursor = win.editor.buffer.iter_at_mark(&win.editor.buffer.get_insert()).offset() as usize;
    assert_eq!(win.editor.text().chars().nth(cursor), Some('w'));
}

fn manager_lists_and_filters(c: &Ctx) {
    let a = c.new_window();
    let b = c.new_window();
    a.editor.buffer.set_text("# Groceries\n- milk");
    b.editor.buffer.set_text("# Work");
    c.app.open_manager();
    let m = c.app.manager().unwrap();
    assert_eq!(m.row_ids().len(), 2);
    m.search.set_text("milk");
    m.refresh();
    assert_eq!(m.row_ids(), [a.id()]);
    m.search.set_text("nothing-matches");
    m.refresh();
    assert_eq!(m.stack.visible_child_name().unwrap(), "empty");
    m.window.close();
    pump(20);
    assert!(c.app.manager().is_none(), "a closed manager is forgotten");
    c.app.open_manager();
    let reopened = c.app.manager().unwrap();
    assert!(reopened.window.is_visible());
    assert_eq!(reopened.row_ids().len(), 2, "and a fresh one lists the notes");
    reopened.window.close();
}

// -- xpad import ---------------------------------------------------------------

/// Two xpad notes: a visible one in xpad's default yellow, a hidden light blue one.
fn xpad_dir_with_notes() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let pads = [
        ("AAAAAA", "Buy milk", "follow_color 1\nhidden 0"),
        ("BBBBBB", "Old idea", "follow_color 0\nback rgb(150,200,255)\nhidden 1"),
    ];
    for (id, text, info) in pads {
        std::fs::write(dir.path().join(format!("content-{id}")), text).unwrap();
        let info = format!("width 320\nheight 240\n{info}\ncontent content-{id}\n");
        std::fs::write(dir.path().join(format!("info-{id}")), info).unwrap();
    }
    dir
}

fn visible_texts(app: &App) -> Vec<String> {
    app.windows().iter().filter(|w| w.window.is_visible()).map(|w| w.note.borrow().text.clone()).collect()
}

fn import_xpad_shows_imported_notes(c: &Ctx) {
    let xpad = xpad_dir_with_notes();
    c.app.set_xpad_dir(xpad.path());
    assert_eq!(c.app.import_xpad().unwrap(), 2);
    assert_eq!(visible_texts(&c.app), ["Buy milk"]);
    let mut notes: Vec<(String, String, bool, i32)> = c
        .app
        .notes()
        .iter()
        .map(|n| {
            let n = n.borrow();
            (n.text.clone(), n.color.clone(), n.visible, n.width)
        })
        .collect();
    notes.sort();
    assert_eq!(notes, [("Buy milk".into(), "yellow".into(), true, 320), ("Old idea".into(), "sky".into(), false, 320)]);
    assert_eq!(c.app.import_xpad().unwrap(), 0, "importing again adds nothing");
}

/// The offer shown on a first start, as a dialog over the notes manager.
fn xpad_offer(app: &App) -> adw::AlertDialog {
    let manager = app.manager().expect("the notes manager opens behind the offer");
    manager.window.visible_dialog().expect("the import offer is shown").downcast().unwrap()
}

fn first_start_offers_xpad_import(c: &Ctx) {
    let xpad = xpad_dir_with_notes();
    c.app.set_xpad_dir(xpad.path());
    c.app.run_command("show");
    pump(50);
    let dialog = xpad_offer(&c.app);
    assert!(c.app.windows().is_empty(), "no blank note behind the offer");
    dialog.emit_by_name::<()>("response", &[&"import"]);
    pump(50);
    assert_eq!(c.app.notes().len(), 2);
    assert_eq!(visible_texts(&c.app), ["Buy milk"]);
    let manager = c.app.manager().unwrap();
    assert_eq!(manager.row_ids().len(), 2, "the manager lists the imported notes");
    manager.window.destroy();
}

fn declining_xpad_import_starts_fresh_and_never_asks_again(c: &Ctx) {
    let xpad = xpad_dir_with_notes();
    c.app.set_xpad_dir(xpad.path());
    c.app.run_command("show");
    pump(50);
    xpad_offer(&c.app).emit_by_name::<()>("response", &[&"skip"]);
    pump(50);
    assert_eq!(visible_texts(&c.app), [""], "a blank note to start with");
    c.app.manager().unwrap().window.destroy();
    pump(20);

    let id = c.app.windows()[0].id();
    c.app.delete_note(&id);
    c.app.run_command("show");
    pump(50);
    assert!(c.app.manager().is_none(), "not offered a second time");
    assert_eq!(visible_texts(&c.app), [""]);
}

fn deleting_a_hidden_note_from_the_manager_asks_there(c: &Ctx) {
    let dir = tempfile::tempdir().unwrap();
    let store = NoteStore::new(dir.path()).unwrap();
    let mut note = Note { text: "Old shopping list".into(), visible: false, ..Default::default() };
    store.save(&mut note).unwrap();
    c.app.load_notes(store);
    c.app.open_manager();
    let m = c.app.manager().unwrap();
    m.delete_button(&note.id).expect("a delete button").emit_clicked();
    pump(50);
    let dialog: adw::AlertDialog =
        m.window.visible_dialog().expect("the question appears over the manager").downcast().unwrap();
    assert!(c.app.windows().is_empty(), "no hidden note window was opened to ask");
    dialog.emit_by_name::<()>("response", &[&"delete"]);
    pump(50);
    assert!(c.app.notes().is_empty());
    assert!(m.row_ids().is_empty());
    assert!(dir.path().join("trash").join(format!("{}.md", note.id)).exists());
    m.window.close();
}

// -- tray icon -----------------------------------------------------------------

/// Call a D-Bus method without blocking the main loop (the app answers on it).
fn dbus_call(
    conn: &gio::DBusConnection,
    dest: &str,
    path: &str,
    iface: &str,
    method: &str,
    args: glib::Variant,
) -> glib::Variant {
    let result = Rc::new(RefCell::new(None));
    let slot = result.clone();
    conn.call(
        Some(dest),
        path,
        iface,
        method,
        Some(&args),
        None,
        gio::DBusCallFlags::NONE,
        2000,
        gio::Cancellable::NONE,
        move |r| *slot.borrow_mut() = Some(r),
    );
    let until = Instant::now() + Duration::from_secs(3);
    while result.borrow().is_none() && Instant::now() < until {
        pump(10);
    }
    let reply = result.borrow_mut().take().expect("a D-Bus reply in time");
    reply.unwrap_or_else(|e| panic!("{iface}.{method}: {e}"))
}

fn tray_icon_registers_and_works(c: &Ctx) {
    let conn = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).expect("a session bus");
    // A stand-in for the tray (Waybar, KDE...): the watcher items register with.
    let node = gio::DBusNodeInfo::for_xml(
        r#"<node><interface name="org.kde.StatusNotifierWatcher"><method name="RegisterStatusNotifierItem">
           <arg type="s" direction="in"/></method></interface></node>"#,
    )
    .unwrap();
    let registered = Rc::new(RefCell::new(None::<String>));
    let slot = registered.clone();
    let watcher = conn
        .register_object("/StatusNotifierWatcher", &node.lookup_interface("org.kde.StatusNotifierWatcher").unwrap())
        .method_call(move |_, _, _, _, _, params, invocation| {
            *slot.borrow_mut() = params.child_value(0).str().map(String::from);
            invocation.return_value(None);
        })
        .build()
        .unwrap();
    let owner = gio::bus_own_name_on_connection(
        &conn,
        "org.kde.StatusNotifierWatcher",
        gio::BusNameOwnerFlags::NONE,
        |_, _| {},
        |_, _| {},
    );
    let until = Instant::now() + Duration::from_secs(3);
    while registered.borrow().is_none() && Instant::now() < until {
        pump(10);
    }
    let item = registered.borrow().clone().expect("the app registers its tray icon");
    assert_eq!(Some(item.clone()), conn.unique_name().map(String::from), "registered under the app's bus name");

    let get = |prop: &str| {
        let reply = dbus_call(
            &conn,
            &item,
            "/StatusNotifierItem",
            "org.freedesktop.DBus.Properties",
            "Get",
            ("org.kde.StatusNotifierItem", prop).to_variant(),
        );
        reply.child_value(0).as_variant().unwrap()
    };
    assert_eq!(get("Title").str(), Some("Realer Xpad"));
    let pixmaps = get("IconPixmap").get::<Vec<(i32, i32, Vec<u8>)>>().unwrap();
    assert!(!pixmaps.is_empty(), "the logo, as pixels");
    assert_eq!(get("IconName").str(), Some(""), "pixels only, so no tray can fail to look the name up");
    for (w, h, data) in &pixmaps {
        assert_eq!(data.len(), (*w * *h * 4) as usize);
        assert!(data.chunks(4).any(|p| p[0] > 0), "not fully transparent");
    }

    // Left click hides the notes, and shows them again.
    let win = c.new_window();
    pump(50);
    let click = || {
        dbus_call(
            &conn,
            &item,
            "/StatusNotifierItem",
            "org.kde.StatusNotifierItem",
            "Activate",
            (0i32, 0i32).to_variant(),
        )
    };
    click();
    assert!(!win.window.is_visible());
    click();
    assert!(win.window.is_visible());

    // The menu, as the tray reads it (GDBus checks the reply types).
    let reply = dbus_call(
        &conn,
        &item,
        "/MenuBar",
        "com.canonical.dbusmenu",
        "GetLayout",
        (0i32, -1i32, Vec::<String>::new()).to_variant(),
    );
    assert_eq!(reply.type_().as_str(), "(u(ia{sv}av))");
    assert_eq!(reply.child_value(1).child_value(2).n_children(), 6);

    // The menu's "New Note".
    let notes = c.app.notes().len();
    let event = (1i32, "clicked", glib::Variant::from_variant(&"".to_variant()), 0u32).to_variant();
    dbus_call(&conn, &item, "/MenuBar", "com.canonical.dbusmenu", "Event", event);
    assert_eq!(c.app.notes().len(), notes + 1);

    gio::bus_unown_name(owner);
    conn.unregister_object(watcher).unwrap();
}

/// A named UI test.
type UiTest = (&'static str, fn(&Ctx));

fn main() {
    // Only inside tools/headless.sh: in a desktop session the tests would open
    // notes and tray icons there.
    if std::env::var_os("REALER_XPAD_HEADLESS").is_none() {
        println!("skipped: run with tools/headless.sh");
        return;
    }
    let data = tempfile::tempdir().unwrap();
    // SAFETY: set before any other thread exists.
    unsafe {
        std::env::set_var("REALER_XPAD_NO_HYPR", "1");
        std::env::set_var("REALER_XPAD_DATA", data.path());
    }
    let app = App::new(None, false, None);
    app.register(); // runs startup on this thread

    let tests: &[UiTest] = &[
        ("editor_styles_source_live", editor_styles_source_live),
        ("editor_restyles_after_typing", editor_restyles_after_typing),
        ("read_view_renders_clean_text", read_view_renders_clean_text),
        ("toggle_checkbox_is_undoable", toggle_checkbox_is_undoable),
        ("list_continuation_and_exit", list_continuation_and_exit),
        ("wrap_selection_bold", wrap_selection_bold),
        ("indent_list_item", indent_list_item),
        ("new_note_is_saved_and_shown", new_note_is_saved_and_shown),
        ("typing_saves_text", typing_saves_text),
        ("color_change", color_change),
        ("tools_and_strokes", tools_and_strokes),
        ("shapes_are_drawn_by_dragging", shapes_are_drawn_by_dragging),
        ("undo_follows_the_tool", undo_follows_the_tool),
        ("tool_keys_only_while_drawing", tool_keys_only_while_drawing),
        ("note_saved_in_draw_mode_reopens_in_draw_mode", note_saved_in_draw_mode_reopens_in_draw_mode),
        ("canvas_pointer_erasing", canvas_pointer_erasing),
        ("canvas_renders", canvas_renders),
        ("placeholder_lines_up_with_the_cursor", placeholder_lines_up_with_the_cursor),
        ("bundled_icons_are_available", bundled_icons_are_available),
        ("hide_and_show", hide_and_show),
        ("toggle_command_hides_and_restores", toggle_command_hides_and_restores),
        ("manager_shows_notes_hidden_by_hide_all", manager_shows_notes_hidden_by_hide_all),
        ("delete_empty_note_skips_confirmation", delete_empty_note_skips_confirmation),
        ("existing_notes_open_on_show", existing_notes_open_on_show),
        ("read_view_checkbox_click", read_view_checkbox_click),
        ("read_view_text_click_enters_edit_at_position", read_view_text_click_enters_edit_at_position),
        ("manager_lists_and_filters", manager_lists_and_filters),
        ("deleting_a_hidden_note_from_the_manager_asks_there", deleting_a_hidden_note_from_the_manager_asks_there),
        ("tray_icon_registers_and_works", tray_icon_registers_and_works),
        ("import_xpad_shows_imported_notes", import_xpad_shows_imported_notes),
        ("first_start_offers_xpad_import", first_start_offers_xpad_import),
        (
            "declining_xpad_import_starts_fresh_and_never_asks_again",
            declining_xpad_import_starts_fresh_and_never_asks_again,
        ),
    ];
    let mut failed = Vec::new();
    for (name, test) in tests {
        let ctx = Ctx::new(&app);
        let ok = catch_unwind(AssertUnwindSafe(|| test(&ctx))).is_ok();
        println!("test {name} ... {}", if ok { "ok" } else { "FAILED" });
        if !ok {
            failed.push(*name);
        }
        drop(ctx);
        pump(20);
    }
    println!("\nui tests: {} passed, {} failed", tests.len() - failed.len(), failed.len());
    if !failed.is_empty() {
        println!("failures: {}", failed.join(", "));
        std::process::exit(1);
    }
}
