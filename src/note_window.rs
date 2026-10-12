//! A single sticky note window.

use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::time::Duration;

use adw::prelude::*;
use gtk::{gdk, gio, glib};

use crate::app::{App, AppInner};
use crate::canvas::Canvas;
use crate::drawing::{Drawing, INK_COLORS, SHAPES, WIDTHS};
use crate::editor::{MarkdownEditor, TEXT_INSET, TEXT_TOP};
use crate::store::{COLORS, Note};
use crate::theme::{PALETTE, swatch};

const SAVE_DELAY: Duration = Duration::from_millis(500);

/// The always-visible tool row: (tool, icon, tooltip). `"shape"` stands for
/// whichever shape is chosen.
const TOOL_ROW: [(&str, &str, &str); 5] = [
    ("text", "rx-text-symbolic", "Text (T)"),
    ("pen", "rx-pen-symbolic", "Pen (P)"),
    ("highlighter", "rx-highlighter-symbolic", "Highlighter (H)"),
    ("shape", "rx-rect-symbolic", "Shapes (S)"),
    ("eraser", "rx-eraser-symbolic", "Eraser (E)"),
];

fn shape_icon(shape: &str) -> &'static str {
    match shape {
        "line" => "rx-line-symbolic",
        "arrow" => "rx-arrow-symbolic",
        "ellipse" => "rx-ellipse-symbolic",
        _ => "rx-rect-symbolic",
    }
}

fn shape_name(shape: &str) -> &'static str {
    match shape {
        "line" => "Line",
        "arrow" => "Arrow",
        "ellipse" => "Ellipse",
        _ => "Rectangle",
    }
}

fn ink_name(color: &str) -> &'static str {
    match color {
        "ink" => "Auto",
        "#e5484d" => "Red",
        "#3e63dd" => "Blue",
        "#30a46c" => "Green",
        "#f76b15" => "Orange",
        _ => "Purple",
    }
}

fn note_btn<W: IsA<gtk::Widget>>(widget: W, reveal: bool) -> W {
    widget.add_css_class("note-btn");
    if reveal {
        widget.add_css_class("reveal");
    }
    widget
}

fn button(icon: &str, tooltip: &str, reveal: bool) -> gtk::Button {
    note_btn(gtk::Button::builder().icon_name(icon).tooltip_text(tooltip).valign(gtk::Align::Center).build(), reveal)
}

fn toggle(icon: &str, tooltip: &str, reveal: bool) -> gtk::ToggleButton {
    note_btn(
        gtk::ToggleButton::builder().icon_name(icon).tooltip_text(tooltip).valign(gtk::Align::Center).build(),
        reveal,
    )
}

fn ink_class(color: &str) -> String {
    format!("ink-{}", color.trim_start_matches('#'))
}

fn is_dark() -> bool {
    adw::StyleManager::default().is_dark()
}

pub struct Inner {
    pub window: adw::ApplicationWindow,
    pub note: Rc<RefCell<Note>>,
    app: Weak<AppInner>,
    pub editor: MarkdownEditor,
    pub canvas: Canvas,
    pub pin_btn: gtk::ToggleButton,
    pub tool_bar: gtk::Box,
    pub placeholder: gtk::Label,
    pub undo_btn: gtk::Button,
    pub redo_btn: gtk::Button,
    /// Text, pen, highlighter, shape and eraser, in the tool row.
    pub tool_buttons: Vec<(&'static str, gtk::ToggleButton)>,
    pub shape_buttons: Vec<(&'static str, gtk::ToggleButton)>,
    pub swatch_buttons: Vec<(&'static str, gtk::ToggleButton)>,
    pub color_btn: gtk::MenuButton,
    pub ink_btn: gtk::MenuButton,
    /// The selected tool (`"text"`, a freehand tool, `"shape"` or `"eraser"`).
    tool: RefCell<&'static str>,
    /// The drawing tool Ctrl+D switches to, and the chosen shape.
    last_draw_tool: RefCell<&'static str>,
    shape: RefCell<&'static str>,
    ink_buttons: Vec<(&'static str, gtk::ToggleButton)>,
    width_buttons: Vec<(f64, gtk::ToggleButton)>,
    clear_btn: gtk::Button,
    close_btn: gtk::Button,
    ink_dot: gtk::Box,
    shape_btn: gtk::ToggleButton,
    save_id: RefCell<Option<glib::SourceId>>,
}

#[derive(Clone)]
pub struct NoteWindow(Rc<Inner>);

impl std::ops::Deref for NoteWindow {
    type Target = Inner;
    fn deref(&self) -> &Inner {
        &self.0
    }
}

impl NoteWindow {
    pub fn new(app: &App, note: Rc<RefCell<Note>>) -> Self {
        let (id, width, height, strokes, pinned) = {
            let n = note.borrow();
            (n.id.clone(), n.width, n.height, n.strokes.clone(), n.pinned)
        };
        let window = adw::ApplicationWindow::builder()
            .application(app.adw())
            .default_width(width)
            .default_height(height)
            .build();
        window.set_size_request(230, 140); // room for the tool row
        window.add_css_class("note");
        // Unique initial title so the compositor (Hyprland) can identify us.
        window.set_title(Some(&initial_title(&id)));

        // Header: drag handle with controls that fade in on hover.
        let start = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        let new_btn = button("list-add-symbolic", "New note (Ctrl+N)", true);
        new_btn.set_action_name(Some("app.new-note"));
        start.append(&new_btn);
        let swatch_grid = gtk::FlowBox::builder()
            .max_children_per_line(4)
            .min_children_per_line(4)
            .selection_mode(gtk::SelectionMode::None)
            .column_spacing(6)
            .row_spacing(6)
            .build();
        swatch_grid.add_css_class("swatch-grid");
        let mut swatch_buttons = Vec::new();
        let mut group: Option<gtk::ToggleButton> = None;
        for (name, light, _) in PALETTE {
            let btn = gtk::ToggleButton::builder().tooltip_text(light.label).build();
            btn.set_group(group.as_ref());
            group.get_or_insert_with(|| btn.clone());
            btn.add_css_class("swatch");
            btn.add_css_class(name);
            swatch_grid.append(&btn);
            swatch_buttons.push((name, btn));
        }
        // A dot made of all note colours reads as "colour" more clearly than an eyedropper.
        let palette_dot = gtk::Box::builder().halign(gtk::Align::Center).valign(gtk::Align::Center).build();
        palette_dot.add_css_class("palette-dot");
        let color_btn = note_btn(
            gtk::MenuButton::builder()
                .child(&palette_dot)
                .tooltip_text("Note colour")
                .valign(gtk::Align::Center)
                .popover(&gtk::Popover::builder().child(&swatch_grid).build())
                .build(),
            true,
        );
        start.append(&color_btn);

        let end = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        let pin_btn = toggle("rx-pin-symbolic", "Keep on top, on every workspace", true);
        pin_btn.set_visible(app.hypr().is_some());
        pin_btn.set_active(pinned);
        end.append(&pin_btn);
        let menu_btn = note_btn(
            gtk::MenuButton::builder()
                .icon_name("view-more-symbolic")
                .tooltip_text("Menu")
                .valign(gtk::Align::Center)
                .menu_model(&menu())
                .build(),
            true,
        );
        end.append(&menu_btn);
        let close_btn = button("window-close-symbolic", "Close note (Ctrl+W)", true);
        end.append(&close_btn);

        let header = gtk::CenterBox::new();
        header.set_start_widget(Some(&start));
        header.set_end_widget(Some(&end));
        header.add_css_class("note-header");
        let handle = gtk::WindowHandle::builder().child(&header).build();

        // Body: text editor with the drawing layer on top.
        let editor = MarkdownEditor::new();
        let canvas = Canvas::new(Drawing::new(strokes));
        let placeholder = gtk::Label::builder()
            .label("Start typing…")
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Start)
            .margin_start(TEXT_INSET)
            .margin_top(TEXT_TOP)
            .can_target(false)
            .build();
        placeholder.add_css_class("placeholder");

        // The tool row, always visible: text, the drawing tools, ink, undo/redo.
        let bar = gtk::Box::builder().spacing(1).halign(gtk::Align::Center).build();
        bar.add_css_class("tool-bar");
        let mut tool_buttons = Vec::new();
        let mut group: Option<gtk::ToggleButton> = None;
        let shape_list =
            gtk::Box::builder().spacing(2).margin_top(4).margin_bottom(4).margin_start(4).margin_end(4).build();
        let mut shape_buttons = Vec::new();
        let mut shape_group: Option<gtk::ToggleButton> = None;
        for shape in SHAPES {
            let btn = toggle(shape_icon(shape), shape_name(shape), false);
            btn.set_group(shape_group.as_ref());
            shape_group.get_or_insert_with(|| btn.clone());
            shape_list.append(&btn);
            shape_buttons.push((shape, btn));
        }
        let shape_menu = note_btn(
            gtk::MenuButton::builder()
                .icon_name("pan-down-symbolic")
                .tooltip_text("Choose a shape")
                .valign(gtk::Align::Center)
                .popover(&gtk::Popover::builder().child(&shape_list).build())
                .build(),
            false,
        );
        shape_menu.add_css_class("split-arrow");
        let mut shape_btn = None;
        for (tool, icon, tooltip) in TOOL_ROW {
            let btn = toggle(icon, tooltip, false);
            btn.set_group(group.as_ref());
            group.get_or_insert_with(|| btn.clone());
            bar.append(&btn);
            if tool == "shape" {
                btn.add_css_class("split-main");
                bar.append(&shape_menu);
                shape_btn = Some(btn.clone());
            }
            tool_buttons.push((tool, btn));
        }
        let shape_btn = shape_btn.expect("a shape tool");
        bar.append(&gtk::Separator::new(gtk::Orientation::Vertical));

        let ink_dot = gtk::Box::builder().halign(gtk::Align::Center).valign(gtk::Align::Center).build();
        ink_dot.add_css_class("ink-dot");
        ink_dot.add_css_class("ink-ink");
        let ink_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(6)
            .margin_end(6)
            .build();
        let inks = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let mut ink_buttons = Vec::new();
        let mut group: Option<gtk::ToggleButton> = None;
        for color in INK_COLORS {
            let btn = gtk::ToggleButton::builder().tooltip_text(ink_name(color)).build();
            btn.set_group(group.as_ref());
            group.get_or_insert_with(|| btn.clone());
            btn.add_css_class("ink");
            btn.add_css_class(&ink_class(color));
            inks.append(&btn);
            ink_buttons.push((color, btn));
        }
        ink_buttons[0].1.set_active(true);
        ink_box.append(&inks);
        let sizes = gtk::Box::builder().spacing(6).homogeneous(true).build();
        let mut width_buttons = Vec::new();
        let mut group: Option<gtk::ToggleButton> = None;
        for (width, label) in WIDTHS.iter().zip(["Fine", "Medium", "Bold"]) {
            let btn = gtk::ToggleButton::with_label(label);
            btn.set_group(group.as_ref());
            group.get_or_insert_with(|| btn.clone());
            btn.set_active(*width == canvas.width.get());
            sizes.append(&btn);
            width_buttons.push((*width, btn));
        }
        ink_box.append(&sizes);
        let clear_btn = gtk::Button::with_label("Clear Drawing");
        clear_btn.add_css_class("destructive-action");
        ink_box.append(&clear_btn);
        let ink_btn = note_btn(
            gtk::MenuButton::builder()
                .child(&ink_dot)
                .tooltip_text("Ink colour & size")
                .valign(gtk::Align::Center)
                .popover(&gtk::Popover::builder().child(&ink_box).build())
                .build(),
            false,
        );
        bar.append(&ink_btn);
        bar.append(&gtk::Separator::new(gtk::Orientation::Vertical));
        let undo_btn = button("edit-undo-symbolic", "Undo (Ctrl+Z)", false);
        let redo_btn = button("edit-redo-symbolic", "Redo (Ctrl+Shift+Z)", false);
        bar.append(&undo_btn);
        bar.append(&redo_btn);

        let body = gtk::Overlay::builder().child(editor.widget()).vexpand(true).build();
        body.add_overlay(&placeholder);
        body.add_overlay(&canvas.area);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&handle);
        content.append(&body);
        content.append(&bar);
        window.set_content(Some(&content));

        let win = NoteWindow(Rc::new(Inner {
            window,
            note,
            app: Rc::downgrade(&app.0),
            editor,
            canvas,
            pin_btn,
            tool_bar: bar,
            placeholder,
            undo_btn,
            redo_btn,
            tool_buttons,
            shape_buttons,
            swatch_buttons,
            color_btn,
            ink_btn,
            tool: RefCell::new("text"),
            last_draw_tool: RefCell::new("pen"),
            shape: RefCell::new("rect"),
            ink_buttons,
            width_buttons,
            clear_btn,
            close_btn,
            ink_dot,
            shape_btn,
            save_id: RefCell::default(),
        }));

        let (text, mode) = {
            let n = win.note.borrow();
            (n.text.clone(), n.mode.clone())
        };
        win.editor.set_text(&text);
        win.apply_color();
        win.shape_buttons[2].1.set_active(true); // rectangle
        win.set_tool(if mode == "draw" { "pen" } else { "text" }, true);
        if !text.trim().is_empty() {
            win.editor.show_read();
        }
        win.update_undo_buttons();
        win.connect_signals();
        win
    }

    fn weak(&self) -> Weak<Inner> {
        Rc::downgrade(&self.0)
    }

    fn app(&self) -> Option<App> {
        self.app.upgrade().map(App)
    }

    pub fn id(&self) -> String {
        self.note.borrow().id.clone()
    }

    pub fn initial_title(&self) -> String {
        initial_title(&self.id())
    }

    /// Wire a signal handler that receives this window (if it still exists).
    fn bind<F: Fn(&NoteWindow) + 'static>(&self, f: F) -> impl Fn() + 'static {
        let weak = self.weak();
        move || {
            if let Some(inner) = weak.upgrade() {
                f(&NoteWindow(inner));
            }
        }
    }

    fn connect_signals(&self) {
        let on = |f: fn(&NoteWindow)| self.bind(f);

        let cb = on(NoteWindow::on_text_changed);
        self.editor.connect_changed(cb);
        let cb = on(NoteWindow::on_drawing_changed);
        self.canvas.connect_changed(cb);

        let cb = on(NoteWindow::on_pin_toggled);
        self.pin_btn.connect_toggled(move |_| cb());
        let cb = on(NoteWindow::hide_note);
        self.close_btn.connect_clicked(move |_| cb());

        for (name, btn) in &self.swatch_buttons {
            let name = *name;
            let cb = self.bind(move |w: &NoteWindow| w.on_swatch(name));
            btn.connect_toggled(move |b| {
                if b.is_active() {
                    cb();
                }
            });
        }
        for (tool, btn) in &self.tool_buttons {
            let tool = *tool;
            let cb = self.bind(move |w: &NoteWindow| {
                if *w.tool.borrow() != tool {
                    w.set_tool(tool, false);
                }
            });
            btn.connect_toggled(move |b| {
                if b.is_active() {
                    cb();
                }
            });
        }
        for (shape, btn) in &self.shape_buttons {
            let shape = *shape;
            let cb = self.bind(move |w: &NoteWindow| w.choose_shape(shape));
            btn.connect_toggled(move |b| {
                if b.is_active() {
                    cb();
                }
            });
        }
        for (color, btn) in &self.ink_buttons {
            let color = *color;
            let cb = self.bind(move |w: &NoteWindow| w.set_ink(color));
            btn.connect_toggled(move |b| {
                if b.is_active() {
                    cb();
                }
            });
        }
        for (width, btn) in &self.width_buttons {
            let width = *width;
            let cb = self.bind(move |w: &NoteWindow| w.canvas.width.set(width));
            btn.connect_toggled(move |b| {
                if b.is_active() {
                    cb();
                }
            });
        }
        let cb = on(|w| w.canvas.clear());
        self.clear_btn.connect_clicked(move |_| cb());
        let cb = on(NoteWindow::undo);
        self.undo_btn.connect_clicked(move |_| cb());
        let cb = on(NoteWindow::redo);
        self.redo_btn.connect_clicked(move |_| cb());
        let cb = on(NoteWindow::update_undo_buttons);
        self.editor.buffer.connect_can_undo_notify(move |_| cb());
        let cb = on(NoteWindow::update_undo_buttons);
        self.editor.buffer.connect_can_redo_notify(move |_| cb());

        let cb = on(NoteWindow::apply_color);
        adw::StyleManager::default().connect_dark_notify(move |_| cb());

        // Closing from the compositor (e.g. Super+Q) hides the note instead of
        // destroying it; it can be reopened from the notes manager.
        let cb = on(NoteWindow::hide_note);
        self.window.connect_close_request(move |_| {
            cb();
            glib::Propagation::Stop
        });
        let cb = on(NoteWindow::on_active_changed);
        self.window.connect_is_active_notify(move |_| cb());

        let hover = gtk::EventControllerMotion::new();
        let window = self.window.downgrade();
        hover.connect_enter(move |_, _, _| {
            if let Some(w) = window.upgrade() {
                w.add_css_class("hover");
            }
        });
        let window = self.window.downgrade();
        hover.connect_leave(move |_| {
            if let Some(w) = window.upgrade() {
                w.remove_css_class("hover");
            }
        });
        self.window.add_controller(hover);

        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = self.weak();
        keys.connect_key_pressed(move |_, key, _, state| {
            if weak.upgrade().is_some_and(|inner| NoteWindow(inner).on_key(key, state)) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.window.add_controller(keys);

        for (name, f) in [
            ("delete", NoteWindow::confirm_delete as fn(&NoteWindow)),
            ("close", NoteWindow::hide_note),
            ("toggle-draw", NoteWindow::toggle_draw),
            ("toggle-read", NoteWindow::toggle_read),
        ] {
            let action = gio::SimpleAction::new(name, None);
            let cb = on(f);
            action.connect_activate(move |_, _| cb());
            self.window.add_action(&action);
        }
    }

    // -- colours --------------------------------------------------------

    fn apply_color(&self) {
        let dark = is_dark();
        let color = self.note.borrow().color.clone();
        for name in COLORS {
            self.window.remove_css_class(name);
        }
        self.window.add_css_class(&color);
        for widget in std::iter::once(self.window.upcast_ref::<gtk::Widget>())
            .chain(self.swatch_buttons.iter().map(|(_, b)| b.upcast_ref::<gtk::Widget>()))
        {
            if dark {
                widget.add_css_class("dark");
            } else {
                widget.remove_css_class("dark");
            }
        }
        self.editor.set_swatch(&swatch(&color, dark));
        if let Some((_, btn)) = self.swatch_buttons.iter().find(|(n, _)| *n == color) {
            btn.set_active(true);
        }
        self.canvas.area.queue_draw();
    }

    fn set_ink(&self, color: &str) {
        let old = self.canvas.color.replace(color.into());
        self.ink_dot.remove_css_class(&ink_class(&old));
        self.ink_dot.add_css_class(&ink_class(color));
    }

    fn on_swatch(&self, name: &str) {
        if self.note.borrow().color == name {
            return;
        }
        self.note.borrow_mut().color = name.into();
        self.apply_color();
        self.schedule_save();
    }

    // -- tools ----------------------------------------------------------

    /// Select a tool: `"text"` types, every other tool draws on the note.
    pub fn set_tool(&self, tool: &str, initial: bool) {
        let Some((tool, btn)) = self.tool_buttons.iter().find(|(t, _)| *t == tool) else { return };
        let tool: &'static str = tool;
        *self.tool.borrow_mut() = tool;
        if !btn.is_active() {
            btn.set_active(true); // the handler sees the tool is already set
        }
        let drawing = tool != "text";
        if drawing {
            *self.last_draw_tool.borrow_mut() = tool;
            let canvas_tool = if tool == "shape" { *self.shape.borrow() } else { tool };
            self.canvas.set_tool(canvas_tool);
        }
        // The note's mode is still saved, so older versions open it the same way.
        self.note.borrow_mut().mode = if drawing { "draw" } else { "text" }.into();
        self.canvas.area.set_can_target(drawing);
        self.ink_btn.set_sensitive(drawing && tool != "eraser");
        if drawing {
            self.editor.show_read();
        } else if !initial {
            self.editor.show_edit(None);
        }
        self.update_placeholder();
        self.update_undo_buttons();
        if !initial {
            self.schedule_save();
        }
    }

    pub fn tool(&self) -> &'static str {
        *self.tool.borrow()
    }

    /// Pick the shape the shape tool draws, and switch to it.
    pub fn choose_shape(&self, shape: &str) {
        let Some(shape) = SHAPES.iter().find(|s| **s == shape) else { return };
        *self.shape.borrow_mut() = shape;
        self.shape_btn.set_icon_name(shape_icon(shape));
        self.shape_btn.set_tooltip_text(Some(&format!("{} (S)", shape_name(shape))));
        if let Some((_, btn)) = self.shape_buttons.iter().find(|(s, _)| s == shape)
            && !btn.is_active()
        {
            btn.set_active(true);
        }
        if self.tool() == "shape" {
            self.canvas.set_tool(shape);
        } else {
            self.set_tool("shape", false);
        }
    }

    /// Ctrl+D: between typing and the last drawing tool.
    fn toggle_draw(&self) {
        let tool = if self.tool() == "text" { *self.last_draw_tool.borrow() } else { "text" };
        self.set_tool(tool, false);
    }

    pub fn mode(&self) -> String {
        self.note.borrow().mode.clone()
    }

    /// Undo in whatever the current tool works on: the text or the drawing.
    fn undo(&self) {
        if self.tool() == "text" {
            self.editor.buffer.undo();
        } else {
            self.canvas.undo();
        }
    }

    fn redo(&self) {
        if self.tool() == "text" {
            self.editor.buffer.redo();
        } else {
            self.canvas.redo();
        }
    }

    fn toggle_read(&self) {
        if self.mode() == "draw" {
            return;
        }
        if self.editor.reading() {
            self.editor.show_edit(None);
        } else {
            self.editor.show_read();
        }
    }

    fn update_placeholder(&self) {
        let n = self.note.borrow();
        self.placeholder.set_visible(n.text.is_empty() && n.mode == "text" && n.strokes.is_empty());
    }

    // -- events ---------------------------------------------------------

    fn on_text_changed(&self) {
        let text = self.editor.text();
        self.note.borrow_mut().text = text;
        self.update_placeholder();
        self.update_title();
        self.schedule_save();
    }

    fn on_drawing_changed(&self) {
        let strokes = self.canvas.drawing.borrow().strokes.clone();
        self.note.borrow_mut().strokes = strokes;
        self.update_undo_buttons();
        self.update_placeholder();
        self.schedule_save();
    }

    fn update_undo_buttons(&self) {
        let (undo, redo) = if self.tool() == "text" {
            (self.editor.buffer.can_undo(), self.editor.buffer.can_redo())
        } else {
            let d = self.canvas.drawing.borrow();
            (d.can_undo(), d.can_redo())
        };
        self.undo_btn.set_sensitive(undo);
        self.redo_btn.set_sensitive(redo);
    }

    fn update_title(&self) {
        // Keep the initial title until the compositor has seen it.
        let hypr = self.app().is_some_and(|a| a.hypr().is_some());
        if !hypr || self.window.is_mapped() {
            self.window.set_title(Some(&self.note.borrow().title()));
        }
    }

    fn on_active_changed(&self) {
        if self.window.is_active() {
            return;
        }
        let (mode, has_text) = {
            let n = self.note.borrow();
            (n.mode.clone(), !n.text.trim().is_empty())
        };
        if mode == "text" && has_text {
            self.editor.show_read();
        }
        self.save_now();
    }

    fn on_key(&self, key: gdk::Key, state: gdk::ModifierType) -> bool {
        use gdk::Key;
        let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
        let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
        match key {
            Key::e | Key::E if ctrl => self.toggle_read(),
            Key::d | Key::D if ctrl => self.toggle_draw(),
            Key::w | Key::W if ctrl => self.hide_note(),
            // Single-letter tool keys only while drawing: in the text tool they type.
            _ if self.tool() != "text" => match key {
                Key::z | Key::Z if ctrl && shift => self.canvas.redo(),
                Key::z | Key::Z if ctrl => self.canvas.undo(),
                Key::y | Key::Y if ctrl => self.canvas.redo(),
                Key::Escape => self.set_tool("text", false),
                Key::t | Key::p | Key::h | Key::s | Key::e if !ctrl => {
                    let tool = match key {
                        Key::t => "text",
                        Key::p => "pen",
                        Key::h => "highlighter",
                        Key::s => "shape",
                        _ => "eraser",
                    };
                    self.select_tool(tool);
                }
                _ => return false,
            },
            Key::Escape if !self.editor.reading() && !self.note.borrow().text.trim().is_empty() => {
                self.editor.show_read();
            }
            _ => return false,
        }
        true
    }

    /// Select a tool as if its button was clicked; a shape name picks that shape.
    pub fn select_tool(&self, tool: &str) {
        if SHAPES.contains(&tool) {
            self.choose_shape(tool);
        } else if let Some((_, btn)) = self.tool_buttons.iter().find(|(t, _)| *t == tool) {
            btn.set_active(true);
        }
    }

    pub fn select_color(&self, color: &str) {
        if let Some((_, btn)) = self.swatch_buttons.iter().find(|(c, _)| *c == color) {
            btn.set_active(true);
        }
    }

    fn on_pin_toggled(&self) {
        self.note.borrow_mut().pinned = self.pin_btn.is_active();
        if let Some(app) = self.app() {
            app.apply_pin(self);
        }
        self.schedule_save();
    }

    // -- persistence & lifecycle ----------------------------------------

    pub fn schedule_save(&self) {
        if let Some(id) = self.save_id.borrow_mut().take() {
            id.remove();
        }
        let cb = self.bind(|w| {
            w.save_id.borrow_mut().take();
            w.save_now();
        });
        let id = glib::timeout_add_local_once(SAVE_DELAY, cb);
        *self.save_id.borrow_mut() = Some(id);
    }

    pub fn save_now(&self) {
        if let Some(id) = self.save_id.borrow_mut().take() {
            id.remove();
        }
        let Some(app) = self.app() else { return };
        if app.hypr().is_none() && self.window.width() > 0 {
            let mut n = self.note.borrow_mut();
            (n.width, n.height) = (self.window.width(), self.window.height());
        }
        app.save_note(&self.note);
    }

    pub fn show_note(&self) {
        self.note.borrow_mut().visible = true;
        self.window.present();
        if let Some(app) = self.app() {
            app.save_note(&self.note);
        }
    }

    pub fn hide_note(&self) {
        if let Some(app) = self.app() {
            app.sync_geometry(self);
        }
        self.note.borrow_mut().visible = false;
        self.save_now();
        self.window.set_visible(false);
    }

    pub fn confirm_delete(&self) {
        if let Some(app) = self.app() {
            app.confirm_delete(&self.id(), self.window.upcast_ref());
        }
    }
}

pub fn initial_title(id: &str) -> String {
    format!("realer-xpad-{id}")
}

fn menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let section = gio::Menu::new();
    section.append(Some("New Note"), Some("app.new-note"));
    section.append(Some("All Notes"), Some("app.manager"));
    section.append(Some("Hide All Notes"), Some("app.hide-all"));
    menu.append_section(None, &section);
    let section = gio::Menu::new();
    section.append(Some("Delete Note"), Some("win.delete"));
    menu.append_section(None, &section);
    menu
}
