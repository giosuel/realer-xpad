//! Notes manager: search, reopen, show/hide and delete notes.

use std::rc::{Rc, Weak};

use adw::prelude::*;
use gtk::gio;

use crate::app::{App, AppInner};
use crate::markdown;
use crate::store::{Note, now};

fn ago(ts: f64) -> String {
    let delta = (now() - ts).max(0.0);
    match delta {
        d if d < 60.0 => "just now".into(),
        d if d < 3600.0 => format!("{} min ago", (d / 60.0) as u64),
        d if d < 86400.0 => format!("{} h ago", (d / 3600.0) as u64),
        d if d < 86400.0 * 7.0 => format!("{} d ago", (d / 86400.0) as u64),
        _ => gtk::glib::DateTime::from_unix_local(ts as i64)
            .and_then(|t| t.format("%d %b %Y"))
            .map(String::from)
            .unwrap_or_default(),
    }
}

fn preview(text: &str) -> String {
    let rendered = markdown::render(text, &markdown::parse(text)).text;
    let lines: Vec<&str> = rendered.lines().map(str::trim).filter(|l| !l.is_empty()).skip(1).take(3).collect();
    lines.join(" · ").chars().take(120).collect()
}

pub struct Inner {
    pub window: adw::ApplicationWindow,
    pub search: gtk::SearchEntry,
    pub list: gtk::ListBox,
    pub stack: gtk::Stack,
    empty: adw::StatusPage,
    app: Weak<AppInner>,
}

#[derive(Clone)]
pub struct Manager(Rc<Inner>);

impl std::ops::Deref for Manager {
    type Target = Inner;
    fn deref(&self) -> &Inner {
        &self.0
    }
}

impl Manager {
    pub fn new(app: &App) -> Self {
        let window = adw::ApplicationWindow::builder()
            .application(app.adw())
            .title("Realer Xpad")
            .default_width(380)
            .default_height(540)
            .build();
        window.add_css_class("manager");

        let header = adw::HeaderBar::new();
        let new_btn = gtk::Button::builder().icon_name("list-add-symbolic").tooltip_text("New note").build();
        new_btn.set_action_name(Some("app.new-note"));
        header.pack_start(&new_btn);
        let menu = gio::Menu::new();
        menu.append(Some("Import from xpad…"), Some("app.import-xpad"));
        menu.append(Some("Open Notes Folder"), Some("app.open-folder"));
        menu.append(Some("About Realer Xpad"), Some("app.about"));
        menu.append(Some("Quit"), Some("app.quit"));
        header.pack_end(&gtk::MenuButton::builder().icon_name("open-menu-symbolic").menu_model(&menu).build());

        let search = gtk::SearchEntry::builder()
            .placeholder_text("Search notes")
            .hexpand(true)
            .margin_start(12)
            .margin_end(12)
            .margin_top(6)
            .margin_bottom(6)
            .build();
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .margin_start(12)
            .margin_end(12)
            .margin_bottom(12)
            .valign(gtk::Align::Start)
            .build();
        list.add_css_class("boxed-list-separate");
        let empty = adw::StatusPage::builder().icon_name("rx-text-symbolic").title("No Notes").build();
        let stack = gtk::Stack::new();
        let scroller =
            gtk::ScrolledWindow::builder().child(&list).vexpand(true).hscrollbar_policy(gtk::PolicyType::Never).build();
        stack.add_named(&scroller, Some("list"));
        stack.add_named(&empty, Some("empty"));

        let bottom = gtk::Box::builder()
            .spacing(6)
            .margin_start(12)
            .margin_end(12)
            .margin_top(6)
            .margin_bottom(12)
            .homogeneous(true)
            .build();
        for (label, action) in [("Show All", "app.show-all"), ("Hide All", "app.hide-all")] {
            let btn = gtk::Button::with_label(label);
            btn.set_action_name(Some(action));
            bottom.append(&btn);
        }

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&search);
        content.append(&stack);
        let view = adw::ToolbarView::new();
        view.set_content(Some(&content));
        view.add_top_bar(&header);
        view.add_bottom_bar(&bottom);
        window.set_content(Some(&view));

        let manager = Manager(Rc::new(Inner { window, search, list, stack, empty, app: Rc::downgrade(&app.0) }));
        let weak = Rc::downgrade(&manager.0);
        manager.search.connect_search_changed(move |_| {
            if let Some(inner) = weak.upgrade() {
                Manager(inner).refresh();
            }
        });
        let weak = Rc::downgrade(&manager.0);
        manager.list.connect_row_activated(move |_, row| {
            if let Some(app) = weak.upgrade().and_then(|m| m.app.upgrade()) {
                App(app).show_note(&row.widget_name());
            }
        });
        manager
    }

    pub fn refresh(&self) {
        let Some(app) = self.app.upgrade().map(App) else { return };
        let query = self.search.text().trim().to_lowercase();
        self.list.remove_all();
        let mut notes: Vec<Note> = app.notes().iter().map(|n| n.borrow().clone()).collect();
        notes.sort_by(|a, b| b.updated.total_cmp(&a.updated));
        let shown: Vec<&Note> =
            notes.iter().filter(|n| query.is_empty() || n.text.to_lowercase().contains(&query)).collect();
        for note in &shown {
            self.list.append(&self.row(note, app.is_shown(&note.id)));
        }
        self.stack.set_visible_child_name(if shown.is_empty() { "empty" } else { "list" });
        if !notes.is_empty() && shown.is_empty() {
            self.empty.set_title("No Matches");
            self.empty.set_description(None);
        } else {
            self.empty.set_title("No Notes");
            self.empty.set_description(Some("Create a note with the + button"));
        }
    }

    /// Note ids of the rows currently listed, top to bottom.
    pub fn row_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        let mut child = self.list.first_child();
        while let Some(row) = child {
            ids.push(row.widget_name().to_string());
            child = row.next_sibling();
        }
        ids
    }

    /// A row for `note`; `shown` is whether its window is on screen right now
    /// (after Hide All, notes are still open but not shown).
    fn row(&self, note: &Note, shown: bool) -> gtk::ListBoxRow {
        let dot = gtk::Box::builder().valign(gtk::Align::Start).margin_top(4).build();
        dot.add_css_class("swatch");
        dot.add_css_class(&note.color);
        if adw::StyleManager::default().is_dark() {
            dot.add_css_class("dark");
        }

        let texts = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(2).hexpand(true).build();
        let title =
            gtk::Label::builder().label(note.title()).xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).build();
        title.add_css_class("card-title");
        texts.append(&title);
        let preview = preview(&note.text);
        if !preview.is_empty() {
            let label =
                gtk::Label::builder().label(preview).xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).build();
            label.add_css_class("card-preview");
            texts.append(&label);
        }
        let mut meta = if shown { String::new() } else { "Hidden · ".into() };
        meta += &format!("Edited {}", ago(note.updated));
        if !note.strokes.is_empty() {
            meta += " · ✎ drawing";
        }
        let meta_label = gtk::Label::builder().label(meta).xalign(0.0).build();
        meta_label.add_css_class("card-meta");
        texts.append(&meta_label);

        let eye = gtk::Button::builder()
            .icon_name(if shown { "view-reveal-symbolic" } else { "view-conceal-symbolic" })
            .tooltip_text(if shown { "Hide" } else { "Show" })
            .valign(gtk::Align::Center)
            .build();
        eye.add_css_class("flat");
        let trash = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .tooltip_text("Delete")
            .valign(gtk::Align::Center)
            .build();
        trash.add_css_class("flat");
        for (btn, delete) in [(&eye, false), (&trash, true)] {
            let (app, id, window) = (self.app.clone(), note.id.clone(), self.window.downgrade());
            btn.connect_clicked(move |_| {
                let Some(app) = app.upgrade().map(App) else { return };
                if !delete {
                    app.toggle_note(&id);
                } else if let Some(window) = window.upgrade() {
                    // Ask here, not over the note: it may be hidden.
                    app.confirm_delete(&id, window.upcast_ref());
                }
            });
        }

        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row_box.add_css_class("note-card");
        row_box.append(&dot);
        row_box.append(&texts);
        row_box.append(&eye);
        row_box.append(&trash);
        let row = gtk::ListBoxRow::builder().child(&row_box).activatable(true).build();
        row.set_widget_name(&note.id);
        trash.set_widget_name("delete");
        eye.set_widget_name("toggle");
        row
    }

    /// The delete button of the row for note `id`. Used by tests.
    pub fn delete_button(&self, id: &str) -> Option<gtk::Button> {
        self.row_button(id, "delete")
    }

    /// The show/hide button of the row for note `id`. Used by tests.
    pub fn toggle_button(&self, id: &str) -> Option<gtk::Button> {
        self.row_button(id, "toggle")
    }

    fn row_button(&self, id: &str, name: &str) -> Option<gtk::Button> {
        let mut child = self.list.first_child();
        while let Some(row) = child {
            if row.widget_name() == id {
                let row_box = row.first_child()?;
                let mut widget = row_box.first_child();
                while let Some(w) = widget {
                    if w.widget_name() == name {
                        return w.downcast().ok();
                    }
                    widget = w.next_sibling();
                }
            }
            child = row.next_sibling();
        }
        None
    }
}
