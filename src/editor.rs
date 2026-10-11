//! Markdown editor: a live-styled edit view plus a clean rendered read view.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use gtk::prelude::*;
use gtk::{gdk, glib, pango};

use crate::markdown::{self, Parsed, Rendered};
use crate::theme::{Swatch, rgba};

const MARGIN: i32 = 16;

fn create_tags(buffer: &gtk::TextBuffer) {
    // Creation order is priority order: later tags win on conflicts.
    let table = buffer.tag_table();
    let tag = |name: &str| gtk::TextTag::builder().name(name);
    let tags = [
        tag("h1").scale(1.55).weight(800).pixels_above_lines(4).pixels_below_lines(2).build(),
        tag("h2").scale(1.3).weight(750).pixels_above_lines(3).pixels_below_lines(1).build(),
        tag("h3").scale(1.12).weight(700).pixels_above_lines(2).build(),
        tag("bold").weight(700).build(),
        tag("italic").style(pango::Style::Italic).build(),
        tag("strike").strikethrough(true).build(),
        tag("quote").style(pango::Style::Italic).left_margin(MARGIN + 12).build(),
        tag("done").strikethrough(true).build(),
        tag("code").family("monospace").scale(0.92).build(),
        tag("code_block").family("monospace").scale(0.92).build(),
        tag("link").underline(pango::Underline::Single).build(),
        tag("marker").build(),
        tag("fence").family("monospace").scale(0.85).build(),
        tag("hr").build(),
        tag("bullet").weight(700).build(),
        tag("checkbox").weight(700).build(),
    ];
    for t in &tags {
        table.add(t);
    }
}

fn apply_palette(buffer: &gtk::TextBuffer, sw: &Swatch) {
    let table = buffer.tag_table();
    let (dim, faint, tint) = (rgba(sw.fg, 0.5), rgba(sw.fg, 0.32), rgba(sw.fg, 0.08));
    let quote = rgba(sw.fg, 0.75);
    let get = |name: &str| table.lookup(name).expect("tag exists");
    for name in ["marker", "fence", "hr"] {
        get(name).set_foreground(Some(&faint));
    }
    get("done").set_foreground(Some(&dim));
    get("quote").set_foreground(Some(&quote));
    get("code").set_background(Some(&tint));
    get("code_block").set_paragraph_background(Some(&tint));
    for name in ["link", "bullet", "checkbox"] {
        get(name).set_foreground(Some(sw.accent));
    }
}

fn apply_spans(buffer: &gtk::TextBuffer, spans: &[markdown::Span], markers: &[markdown::Marker]) {
    let (start, end) = buffer.bounds();
    buffer.remove_all_tags(&start, &end);
    let tag = |name: &str, a: usize, b: usize| {
        buffer.apply_tag_by_name(name, &buffer.iter_at_offset(a as i32), &buffer.iter_at_offset(b as i32));
    };
    for sp in spans {
        tag(sp.tag, sp.start, sp.end);
    }
    for mk in markers {
        let name = if ["checkbox", "bullet", "fence", "hr"].contains(&mk.tag) { mk.tag } else { "marker" };
        tag(name, mk.start, mk.end);
    }
}

fn text_view(editable: bool) -> gtk::TextView {
    let view = gtk::TextView::builder()
        .wrap_mode(gtk::WrapMode::WordChar)
        .left_margin(MARGIN)
        .right_margin(MARGIN)
        .top_margin(2)
        .bottom_margin(MARGIN)
        .editable(editable)
        .cursor_visible(editable)
        .build();
    view.add_css_class("note-text");
    create_tags(&view.buffer());
    view
}

fn scrolled(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder().child(child).hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).build()
}

pub struct Inner {
    pub stack: gtk::Stack,
    pub edit_view: gtk::TextView,
    pub read_view: gtk::TextView,
    pub buffer: gtk::TextBuffer,
    parsed: RefCell<Parsed>,
    rendered: RefCell<Rendered>,
    pending: RefCell<Option<glib::SourceId>>,
    on_changed: RefCell<Vec<Box<dyn Fn()>>>,
}

/// Stack with an `edit` page (source, styled live) and a `read` page.
#[derive(Clone)]
pub struct MarkdownEditor(Rc<Inner>);

impl std::ops::Deref for MarkdownEditor {
    type Target = Inner;
    fn deref(&self) -> &Inner {
        &self.0
    }
}

enum ReadTarget {
    Checkbox(usize),
    Link(String),
    Text(usize),
}

impl Default for MarkdownEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownEditor {
    pub fn new() -> Self {
        let stack =
            gtk::Stack::builder().transition_type(gtk::StackTransitionType::Crossfade).transition_duration(120).build();
        let edit_view = text_view(true);
        let read_view = text_view(false);
        stack.add_named(&scrolled(&edit_view), Some("edit"));
        stack.add_named(&scrolled(&read_view), Some("read"));

        let editor = MarkdownEditor(Rc::new(Inner {
            buffer: edit_view.buffer(),
            stack,
            edit_view,
            read_view,
            parsed: RefCell::default(),
            rendered: RefCell::default(),
            pending: RefCell::default(),
            on_changed: RefCell::default(),
        }));
        editor.connect_signals();
        editor
    }

    fn weak(&self) -> Weak<Inner> {
        Rc::downgrade(&self.0)
    }

    fn upgrade(weak: &Weak<Inner>) -> Option<Self> {
        weak.upgrade().map(MarkdownEditor)
    }

    fn connect_signals(&self) {
        let weak = self.weak();
        self.buffer.connect_changed(move |_| {
            if let Some(ed) = Self::upgrade(&weak) {
                ed.on_buffer_changed();
            }
        });

        let keys = gtk::EventControllerKey::new();
        let weak = self.weak();
        keys.connect_key_pressed(move |_, key, _, state| {
            if Self::upgrade(&weak).is_some_and(|ed| ed.on_key(key, state)) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.edit_view.add_controller(keys);

        let click = gtk::GestureClick::new();
        let weak = self.weak();
        click.connect_released(move |g, n_press, x, y| {
            if let Some(ed) = Self::upgrade(&weak) {
                let ctrl = g.current_event_state().contains(gdk::ModifierType::CONTROL_MASK);
                ed.edit_click(n_press, x, y, ctrl);
            }
        });
        self.edit_view.add_controller(click);

        let click = gtk::GestureClick::new();
        let weak = self.weak();
        click.connect_released(move |g, _, x, y| {
            if let Some(ed) = Self::upgrade(&weak)
                && g.current_button() == gdk::BUTTON_PRIMARY
            {
                ed.read_click(x, y);
            }
        });
        self.read_view.add_controller(click);

        let motion = gtk::EventControllerMotion::new();
        let weak = self.weak();
        motion.connect_motion(move |_, x, y| {
            if let Some(ed) = Self::upgrade(&weak) {
                let pointer = matches!(ed.read_target(x, y), Some(ReadTarget::Checkbox(_) | ReadTarget::Link(_)));
                ed.read_view.set_cursor_from_name(Some(if pointer { "pointer" } else { "text" }));
            }
        });
        self.read_view.add_controller(motion);
    }

    // -- public API -----------------------------------------------------

    pub fn widget(&self) -> &gtk::Stack {
        &self.stack
    }

    pub fn text(&self) -> String {
        let (start, end) = self.buffer.bounds();
        self.buffer.text(&start, &end, true).into()
    }

    pub fn connect_changed(&self, f: impl Fn() + 'static) {
        self.on_changed.borrow_mut().push(Box::new(f));
    }

    pub fn set_text(&self, text: &str) {
        self.buffer.begin_irreversible_action();
        self.buffer.set_text(text);
        self.buffer.end_irreversible_action();
        self.refresh();
    }

    pub fn set_swatch(&self, sw: &Swatch) {
        apply_palette(&self.buffer, sw);
        apply_palette(&self.read_view.buffer(), sw);
    }

    pub fn reading(&self) -> bool {
        self.stack.visible_child_name().as_deref() == Some("read")
    }

    pub fn show_read(&self) {
        self.refresh();
        self.stack.set_visible_child_name("read");
    }

    pub fn show_edit(&self, source_offset: Option<usize>) {
        self.stack.set_visible_child_name("edit");
        if let Some(offset) = source_offset {
            self.buffer.place_cursor(&self.buffer.iter_at_offset(offset as i32));
        }
        self.edit_view.grab_focus();
    }

    pub fn rendered_text(&self) -> String {
        self.rendered.borrow().text.clone()
    }

    /// Wrap the selection (or insert an empty pair) with `marker`.
    pub fn wrap_selection(&self, marker: &str) {
        let buf = &self.buffer;
        let n = marker.chars().count() as i32;
        buf.begin_user_action();
        if let Some((start, end)) = buf.selection_bounds() {
            let (a, b) = (start.offset(), end.offset());
            buf.insert(&mut buf.iter_at_offset(b), marker);
            buf.insert(&mut buf.iter_at_offset(a), marker);
            buf.select_range(&buf.iter_at_offset(a + n), &buf.iter_at_offset(b + n));
        } else {
            buf.insert_at_cursor(&marker.repeat(2));
            let mut it = buf.iter_at_mark(&buf.get_insert());
            it.backward_chars(n);
            buf.place_cursor(&it);
        }
        buf.end_user_action();
    }

    pub fn toggle_checkbox(&self, index: usize) {
        let Some(cb) = self.parsed.borrow().checkboxes.get(index).copied() else { return };
        let buf = &self.buffer;
        buf.begin_user_action();
        let at = cb.start as i32 + 1;
        buf.delete(&mut buf.iter_at_offset(at), &mut buf.iter_at_offset(at + 1));
        buf.insert(&mut buf.iter_at_offset(at), if cb.checked { " " } else { "x" });
        buf.end_user_action();
    }

    // -- internals ------------------------------------------------------

    fn on_buffer_changed(&self) {
        // Coalesce bursts of edits (e.g. paste) into one restyle.
        if self.pending.borrow().is_none() {
            let weak = self.weak();
            let id = glib::idle_add_local_once(move || {
                if let Some(ed) = Self::upgrade(&weak) {
                    ed.pending.borrow_mut().take();
                    ed.refresh();
                }
            });
            *self.pending.borrow_mut() = Some(id);
        }
        for f in self.on_changed.borrow().iter() {
            f();
        }
    }

    pub fn refresh(&self) {
        if let Some(id) = self.pending.borrow_mut().take() {
            id.remove();
        }
        let text = self.text();
        let parsed = markdown::parse(&text);
        apply_spans(&self.buffer, &parsed.spans, &parsed.markers);
        let rendered = markdown::render(&text, &parsed);
        let read_buf = self.read_view.buffer();
        read_buf.set_text(&rendered.text);
        apply_spans(&read_buf, &rendered.spans, &[]);
        *self.parsed.borrow_mut() = parsed;
        *self.rendered.borrow_mut() = rendered;
    }

    fn offset_at(view: &gtk::TextView, x: f64, y: f64) -> Option<usize> {
        let (bx, by) = view.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
        view.iter_at_location(bx, by).map(|it| it.offset() as usize)
    }

    fn read_target(&self, x: f64, y: f64) -> Option<ReadTarget> {
        let offset = Self::offset_at(&self.read_view, x, y)?;
        let rendered = self.rendered.borrow();
        if let Some(&(_, _, i)) = rendered.checkboxes.iter().find(|(a, b, _)| *a <= offset && offset < *b) {
            return Some(ReadTarget::Checkbox(i));
        }
        if let Some(sp) = rendered.spans.iter().find(|s| s.tag == "link" && s.start <= offset && offset < s.end) {
            return sp.data.clone().map(ReadTarget::Link);
        }
        Some(ReadTarget::Text(offset))
    }

    /// Click in the read view at widget coordinates `(x, y)`.
    pub fn read_click(&self, x: f64, y: f64) {
        match self.read_target(x, y) {
            Some(ReadTarget::Checkbox(i)) => {
                self.toggle_checkbox(i);
                self.refresh();
            }
            Some(ReadTarget::Link(uri)) => self.open_uri(&uri),
            Some(ReadTarget::Text(offset)) => {
                let src = self.rendered.borrow().to_source(offset);
                self.show_edit(Some(src));
            }
            None => self.show_edit(Some(self.text().chars().count())),
        }
    }

    /// Click in the edit view: toggles checkboxes, Ctrl+click opens links.
    pub fn edit_click(&self, n_press: i32, x: f64, y: f64, ctrl: bool) {
        if n_press != 1 || self.buffer.has_selection() {
            return;
        }
        let Some(offset) = Self::offset_at(&self.edit_view, x, y) else { return };
        let hit = self.parsed.borrow().checkboxes.iter().position(|cb| cb.start <= offset && offset < cb.end);
        if let Some(i) = hit {
            self.toggle_checkbox(i);
            return;
        }
        if ctrl {
            let link = self
                .parsed
                .borrow()
                .spans
                .iter()
                .find(|s| s.tag == "link" && s.start <= offset && offset < s.end)
                .and_then(|s| s.data.clone());
            if let Some(uri) = link {
                self.open_uri(&uri);
            }
        }
    }

    fn open_uri(&self, uri: &str) {
        let window = self.stack.root().and_downcast::<gtk::Window>();
        gtk::UriLauncher::new(uri).launch(window.as_ref(), gtk::gio::Cancellable::NONE, |_| {});
    }

    fn on_key(&self, key: gdk::Key, state: gdk::ModifierType) -> bool {
        let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
        let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
        match key {
            gdk::Key::b | gdk::Key::B if ctrl => {
                self.wrap_selection("**");
                true
            }
            gdk::Key::i | gdk::Key::I if ctrl => {
                self.wrap_selection("*");
                true
            }
            gdk::Key::Return | gdk::Key::KP_Enter if !shift => self.continue_list(),
            gdk::Key::Tab => self.indent(shift),
            gdk::Key::ISO_Left_Tab => self.indent(true),
            _ => false,
        }
    }

    fn current_line(&self) -> (gtk::TextIter, gtk::TextIter) {
        let cursor = self.buffer.iter_at_mark(&self.buffer.get_insert());
        let mut start = cursor;
        start.set_line_offset(0);
        (start, cursor)
    }

    /// Enter at the end of a list item: continue (or end) the list.
    pub fn continue_list(&self) -> bool {
        let (mut start, mut cursor) = self.current_line();
        if !cursor.ends_line() {
            return false;
        }
        let line = self.buffer.text(&start, &cursor, true);
        let Some(prefix) = markdown::continue_list(&line) else { return false };
        self.buffer.begin_user_action();
        if prefix.is_empty() {
            self.buffer.delete(&mut start, &mut cursor); // empty item: end the list
        } else {
            self.buffer.insert_at_cursor(&format!("\n{prefix}"));
        }
        self.buffer.end_user_action();
        true
    }

    /// Tab / Shift+Tab on a list item: indent or outdent by two spaces.
    pub fn indent(&self, outdent: bool) -> bool {
        let (start, _) = self.current_line();
        let mut line_end = start;
        if !line_end.ends_line() {
            line_end.forward_to_line_end();
        }
        let line = self.buffer.text(&start, &line_end, true).to_string();
        if markdown::continue_list(&line).is_none() && markdown::continue_list(&format!("{line}x")).is_none() {
            return false; // not a list item: let Tab behave normally
        }
        let buf = &self.buffer;
        let at = start.offset();
        buf.begin_user_action();
        if outdent {
            let n = (line.len() - line.trim_start_matches(' ').len()).min(2) as i32;
            if n > 0 {
                buf.delete(&mut buf.iter_at_offset(at), &mut buf.iter_at_offset(at + n));
            }
        } else {
            buf.insert(&mut buf.iter_at_offset(at), "  ");
        }
        buf.end_user_action();
        true
    }
}
