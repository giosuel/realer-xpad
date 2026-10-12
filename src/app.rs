//! Application: single instance, CLI commands, note lifecycle.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::{gio, glib};

use crate::hypr::Hyprland;
use crate::manager::Manager;
use crate::note_window::NoteWindow;
use crate::store::{Note, NoteStore, default_data_dir};
use crate::{APP_ID, APP_NAME, VERSION, cli, theme, tray, xpad};

const GEOMETRY_POLL_S: u32 = 3;
const PLACE_RETRIES: u32 = 20;

/// An app action: name, handler and optional keyboard accelerator.
type ActionEntry = (&'static str, fn(&App), Option<&'static str>);

pub struct AppInner {
    app: adw::Application,
    initial_store: RefCell<Option<NoteStore>>,
    store: RefCell<Option<NoteStore>>,
    hypr: RefCell<Option<Hyprland>>,
    notes: RefCell<HashMap<String, Rc<RefCell<Note>>>>,
    windows: RefCell<HashMap<String, NoteWindow>>,
    manager: RefCell<Option<Manager>>,
    /// Keeps the app running with every window closed, like xpad, until Quit.
    hold: RefCell<Option<gio::ApplicationHoldGuard>>,
    tray: RefCell<Option<tray::Tray>>,
    /// Where to look for xpad notes to import.
    xpad_dir: RefCell<PathBuf>,
}

#[derive(Clone)]
pub struct App(pub(crate) Rc<AppInner>);

impl App {
    /// `unique: false` and `app_id: None` let tests run beside a real instance.
    pub fn new(store: Option<NoteStore>, unique: bool, app_id: Option<&str>) -> Self {
        let mut flags = gio::ApplicationFlags::HANDLES_COMMAND_LINE;
        if !unique {
            flags |= gio::ApplicationFlags::NON_UNIQUE;
        }
        let app = adw::Application::builder().flags(flags).build();
        if let Some(id) = app_id {
            app.set_application_id(Some(id));
        }
        let this = App(Rc::new(AppInner {
            app,
            initial_store: RefCell::new(store),
            store: RefCell::default(),
            hypr: RefCell::default(),
            notes: RefCell::default(),
            windows: RefCell::default(),
            manager: RefCell::default(),
            hold: RefCell::default(),
            tray: RefCell::default(),
            xpad_dir: RefCell::new(xpad::default_dir()),
        }));
        let weak = Rc::downgrade(&this.0);
        this.0.app.connect_startup(move |_| {
            if let Some(inner) = weak.upgrade() {
                App(inner).startup();
            }
        });
        let weak = Rc::downgrade(&this.0);
        this.0.app.connect_command_line(move |_, cmdline| match weak.upgrade() {
            Some(inner) => App(inner).command_line(cmdline),
            None => glib::ExitCode::FAILURE,
        });
        this
    }

    pub fn adw(&self) -> &adw::Application {
        &self.0.app
    }

    pub fn hypr(&self) -> Option<Hyprland> {
        self.0.hypr.borrow().clone()
    }

    pub fn run(&self, args: &[String]) -> glib::ExitCode {
        self.0.app.run_with_args(args)
    }

    /// Register without running the main loop (runs startup). Used by tests.
    pub fn register(&self) {
        self.0.app.register(gio::Cancellable::NONE).expect("register application");
    }

    // -- lifecycle ------------------------------------------------------

    fn startup(&self) {
        theme::load_css();
        gtk::Window::set_default_icon_name(APP_ID);
        let store = self.0.initial_store.borrow_mut().take();
        let store = store.unwrap_or_else(|| NoteStore::new(default_data_dir()).expect("create data directory"));
        *self.0.hypr.borrow_mut() = Hyprland::detect();
        self.load_notes(store);

        let actions: [ActionEntry; 8] = [
            ("new-note", App::new_note, Some("<Ctrl>n")),
            ("show-all", App::show_all, None),
            ("hide-all", App::hide_all, None),
            ("manager", App::open_manager, None),
            ("import-xpad", App::import_xpad_and_report, None),
            ("open-folder", App::open_folder, None),
            ("about", App::about, None),
            ("quit", App::quit_app, Some("<Ctrl>q")),
        ];
        for (name, f, accel) in actions {
            let action = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(&self.0);
            action.connect_activate(move |_, _| {
                if let Some(inner) = weak.upgrade() {
                    f(&App(inner));
                }
            });
            self.0.app.add_action(&action);
            if let Some(accel) = accel {
                self.0.app.set_accels_for_action(&format!("app.{name}"), &[accel]);
            }
        }
        *self.0.hold.borrow_mut() = Some(self.0.app.hold());
        self.start_tray();
        if self.hypr().is_some() {
            let weak = Rc::downgrade(&self.0);
            glib::timeout_add_seconds_local(GEOMETRY_POLL_S, move || match weak.upgrade() {
                Some(inner) => {
                    App(inner).poll_geometry();
                    glib::ControlFlow::Continue
                }
                None => glib::ControlFlow::Break,
            });
        }
    }

    fn command_line(&self, cmdline: &gio::ApplicationCommandLine) -> glib::ExitCode {
        let args: Vec<String> = cmdline.arguments().iter().skip(1).map(|a| a.to_string_lossy().into_owned()).collect();
        let cmd = args.first().map_or("show", String::as_str);
        match cmd {
            "-h" | "--help" | "help" => {
                cmdline.print_literal(&cli::usage());
                return glib::ExitCode::SUCCESS;
            }
            "-v" | "--version" => {
                cmdline.print_literal(&format!("{APP_NAME} {VERSION}\n"));
                return glib::ExitCode::SUCCESS;
            }
            _ if !cli::COMMANDS.iter().any(|(c, _)| *c == cmd) => {
                cmdline.printerr_literal(&format!("Unknown command: {cmd}\n\n{}", cli::usage()));
                return glib::ExitCode::FAILURE;
            }
            _ => {}
        }
        if cmd == "import-xpad" {
            let dir = self.xpad_dir();
            match self.import_xpad() {
                Ok(n) => {
                    let notes = if n == 1 { "note" } else { "notes" };
                    cmdline.print_literal(&format!("Imported {n} {notes} from {}\n", dir.display()));
                }
                Err(err) => {
                    cmdline.printerr_literal(&format!("Could not import from {}: {err}\n", dir.display()));
                    return glib::ExitCode::FAILURE;
                }
            }
            self.run_command("show");
            return glib::ExitCode::SUCCESS;
        }
        self.run_command(cmd);
        glib::ExitCode::SUCCESS
    }

    pub fn run_command(&self, cmd: &str) {
        match cmd {
            "new" => self.new_note(),
            "toggle" if self.any_visible() => self.hide_all(),
            "toggle" => self.show_all(),
            "hide" => self.hide_all(),
            "manager" => self.open_manager(),
            "quit" => self.quit_app(),
            _ => {
                self.show_all();
                if !self.any_visible() {
                    if self.0.notes.borrow().is_empty() {
                        if !self.offer_xpad_import() {
                            self.new_note();
                        }
                    } else {
                        self.open_manager();
                    }
                }
            }
        }
    }

    pub fn quit_app(&self) {
        for win in self.windows() {
            if win.window.is_visible() {
                self.sync_geometry(&win);
            }
            win.save_now();
        }
        self.0.tray.borrow_mut().take();
        self.0.hold.borrow_mut().take();
        self.0.app.quit();
    }

    /// The tray icon, unless REALER_XPAD_NO_TRAY is set or there's no session bus.
    fn start_tray(&self) {
        if std::env::var_os("REALER_XPAD_NO_TRAY").is_some() {
            return;
        }
        let Ok(connection) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else { return };
        let weak = Rc::downgrade(&self.0);
        let on_action = move |action| {
            let Some(app) = weak.upgrade().map(App) else { return };
            match action {
                tray::Action::Toggle => app.run_command("toggle"),
                tray::Action::NewNote => app.new_note(),
                tray::Action::ShowAll => app.run_command("show"),
                tray::Action::HideAll => app.hide_all(),
                tray::Action::Manager => app.open_manager(),
                tray::Action::Quit => app.quit_app(),
            }
        };
        match tray::start(&connection, on_action) {
            Ok(tray) => *self.0.tray.borrow_mut() = Some(tray),
            Err(err) => eprintln!("realer-xpad: no tray icon: {err}"),
        }
    }

    // -- notes ----------------------------------------------------------

    /// (Re)load all notes from `store`, closing any open windows.
    pub fn load_notes(&self, store: NoteStore) {
        for win in self.windows() {
            win.window.destroy();
        }
        self.0.windows.borrow_mut().clear();
        let notes = store.load_all().into_iter().map(|n| (n.id.clone(), Rc::new(RefCell::new(n)))).collect();
        *self.0.notes.borrow_mut() = notes;
        *self.0.store.borrow_mut() = Some(store);
    }

    pub fn store_dir(&self) -> std::path::PathBuf {
        self.0.store.borrow().as_ref().map(|s| s.dir.clone()).unwrap_or_default()
    }

    pub fn load_note_from_disk(&self, id: &str) -> Option<Note> {
        self.0.store.borrow().as_ref()?.load(id)
    }

    /// All notes, oldest first.
    pub fn notes(&self) -> Vec<Rc<RefCell<Note>>> {
        let mut notes: Vec<_> = self.0.notes.borrow().values().cloned().collect();
        notes.sort_by(|a, b| a.borrow().created.total_cmp(&b.borrow().created));
        notes
    }

    /// Open note windows, oldest note first.
    pub fn windows(&self) -> Vec<NoteWindow> {
        let mut wins: Vec<_> = self.0.windows.borrow().values().cloned().collect();
        wins.sort_by(|a, b| a.note.borrow().created.total_cmp(&b.note.borrow().created));
        wins
    }

    pub fn window_for(&self, id: &str) -> Option<NoteWindow> {
        if let Some(win) = self.0.windows.borrow().get(id) {
            return Some(win.clone());
        }
        let note = self.0.notes.borrow().get(id).cloned()?;
        let win = NoteWindow::new(self, note);
        let weak = Rc::downgrade(&self.0);
        let id_owned = id.to_string();
        win.window.connect_map(move |_| {
            if let Some(inner) = weak.upgrade() {
                App(inner).on_note_mapped(&id_owned);
            }
        });
        self.0.windows.borrow_mut().insert(id.into(), win.clone());
        Some(win)
    }

    pub fn new_note(&self) {
        let mut note = Note::default();
        if !self.0.windows.borrow().is_empty() {
            // inherit the colour of the most recently used note
            let notes = self.notes();
            if let Some(last) = notes.iter().max_by(|a, b| a.borrow().updated.total_cmp(&b.borrow().updated)) {
                note.color = last.borrow().color.clone();
            }
        }
        let id = note.id.clone();
        let note = Rc::new(RefCell::new(note));
        self.0.notes.borrow_mut().insert(id.clone(), note.clone());
        self.save_note(&note);
        if let Some(win) = self.window_for(&id) {
            win.show_note();
            win.editor.show_edit(None);
        }
        self.changed();
    }

    pub fn show_note(&self, id: &str) {
        if let Some(win) = self.window_for(id) {
            win.show_note();
        }
        self.changed();
    }

    pub fn toggle_note(&self, id: &str) {
        let visible = self.0.notes.borrow().get(id).is_some_and(|n| n.borrow().visible);
        if let Some(win) = self.window_for(id) {
            if visible { win.hide_note() } else { win.show_note() }
        }
        self.changed();
    }

    pub fn show_all(&self) {
        for note in self.notes() {
            let (id, visible) = (note.borrow().id.clone(), note.borrow().visible);
            if visible && let Some(win) = self.window_for(&id) {
                win.window.present();
            }
        }
    }

    /// Temporarily hide windows; notes stay "open" for the next show/toggle.
    pub fn hide_all(&self) {
        for win in self.windows() {
            if win.window.is_visible() {
                self.sync_geometry(&win);
                win.save_now();
                win.window.set_visible(false);
            }
        }
    }

    pub fn any_visible(&self) -> bool {
        self.windows().iter().any(|w| w.window.is_visible())
    }

    pub fn delete_note(&self, id: &str) {
        if let Some(store) = self.0.store.borrow().as_ref()
            && let Err(err) = store.delete(id)
        {
            // Keep showing it: it would come back on the next start anyway.
            eprintln!("realer-xpad: could not delete note {id}: {err}");
            return;
        }
        self.0.notes.borrow_mut().remove(id);
        let win = self.0.windows.borrow_mut().remove(id);
        if let Some(win) = win {
            win.window.destroy();
        }
        self.changed();
    }

    /// Delete a note, asking first unless it's empty. The question appears over
    /// `parent`, the window the user asked from (the note or the notes manager).
    pub fn confirm_delete(&self, id: &str, parent: &gtk::Widget) {
        let Some(empty) = self.0.notes.borrow().get(id).map(|n| n.borrow().is_empty()) else { return };
        if empty {
            self.delete_note(id);
            return;
        }
        let dialog = adw::AlertDialog::new(Some("Delete Note?"), Some("The note will be moved to the trash folder."));
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("delete", "Delete");
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        let (id, app) = (id.to_string(), Rc::downgrade(&self.0));
        dialog.connect_response(None, move |_, response| {
            if response == "delete"
                && let Some(app) = app.upgrade()
            {
                App(app).delete_note(&id);
            }
        });
        dialog.present(Some(parent));
    }

    pub fn save_note(&self, note: &Rc<RefCell<Note>>) {
        let id = note.borrow().id.clone();
        if !self.0.notes.borrow().contains_key(&id) {
            return; // deleted meanwhile
        }
        let wrote = match self.0.store.borrow().as_ref().map(|store| store.save(&mut note.borrow_mut())) {
            Some(Ok(wrote)) => wrote,
            Some(Err(err)) => {
                eprintln!("realer-xpad: could not save note {id}: {err}");
                false
            }
            None => false,
        };
        if wrote {
            self.changed();
        }
    }

    fn changed(&self) {
        if let Some(m) = self.manager() {
            m.refresh();
        }
    }

    // -- xpad import ----------------------------------------------------

    pub fn xpad_dir(&self) -> PathBuf {
        self.0.xpad_dir.borrow().clone()
    }

    /// Look for xpad notes somewhere else. Used by tests.
    pub fn set_xpad_dir(&self, dir: &Path) {
        *self.0.xpad_dir.borrow_mut() = dir.to_path_buf();
    }

    /// Copy new xpad notes into the store and show the visible ones.
    /// Returns how many notes were imported.
    pub fn import_xpad(&self) -> std::io::Result<usize> {
        let notes = match self.0.store.borrow().as_ref() {
            Some(store) => xpad::import(store, &self.xpad_dir())?,
            None => return Ok(0),
        };
        for note in &notes {
            self.0.notes.borrow_mut().insert(note.id.clone(), Rc::new(RefCell::new(note.clone())));
        }
        for note in notes.iter().filter(|n| n.visible) {
            if let Some(win) = self.window_for(&note.id) {
                win.show_note();
            }
        }
        self.changed();
        Ok(notes.len())
    }

    /// The "Import from xpad" menu entry: import, then say what happened.
    fn import_xpad_and_report(&self) {
        let (heading, body) = match self.import_xpad() {
            Ok(0) => {
                ("No New xpad Notes".to_string(), format!("Nothing to import from {}.", self.xpad_dir().display()))
            }
            Ok(n) => (
                format!("Imported {n} Note{}", if n == 1 { "" } else { "s" }),
                "Your xpad notes are copied over. xpad itself is unchanged.".into(),
            ),
            Err(err) => ("Import Failed".into(), err.to_string()),
        };
        let dialog = adw::AlertDialog::new(Some(&heading), Some(&body));
        dialog.add_response("ok", "OK");
        dialog.present(self.dialog_parent().as_ref());
    }

    /// On a first start without notes, offer to import xpad's notes (once).
    /// Returns whether the offer was shown.
    pub fn offer_xpad_import(&self) -> bool {
        let count = match self.0.store.borrow().as_ref() {
            Some(store) if !xpad::was_offered(store) => xpad::pending(store, &self.xpad_dir()).len(),
            _ => 0,
        };
        if count == 0 {
            return false;
        }
        if let Some(store) = self.0.store.borrow().as_ref() {
            let _ = xpad::mark_offered(store);
        }
        let plural = if count == 1 { "" } else { "s" };
        let dialog = adw::AlertDialog::new(
            Some("Import Your xpad Notes?"),
            Some(&format!(
                "Found {count} note{plural} from xpad. They are copied over with their colours and sizes; xpad itself stays unchanged."
            )),
        );
        let import_label = format!("Import {count} Note{plural}");
        dialog.add_responses(&[("skip", "Start Fresh"), ("import", import_label.as_str())]);
        dialog.set_response_appearance("import", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("import"));
        dialog.set_close_response("skip");
        let weak = Rc::downgrade(&self.0);
        dialog.connect_response(None, move |_, response| {
            let Some(app) = weak.upgrade().map(App) else { return };
            let imported = response == "import" && app.import_xpad().is_ok_and(|n| n > 0);
            if !imported {
                app.new_note();
            }
        });
        // Over the (empty) notes manager, which then lists the imported notes.
        self.open_manager();
        dialog.present(self.dialog_parent().as_ref());
        true
    }

    fn dialog_parent(&self) -> Option<gtk::Widget> {
        self.manager()
            .map(|m| m.window.clone().upcast::<gtk::Widget>())
            .or_else(|| self.0.app.active_window().map(|w| w.upcast::<gtk::Widget>()))
    }

    // -- manager & misc -------------------------------------------------

    /// The open notes manager. Closing its window destroys it, which detaches it
    /// from the application; our own reference would otherwise keep it around.
    pub fn manager(&self) -> Option<Manager> {
        let manager = self.0.manager.borrow().clone()?;
        if manager.window.application().is_none() {
            self.0.manager.borrow_mut().take();
            return None;
        }
        Some(manager)
    }

    pub fn open_manager(&self) {
        let existing = self.manager();
        let manager = existing.unwrap_or_else(|| {
            let m = Manager::new(self);
            *self.0.manager.borrow_mut() = Some(m.clone());
            m
        });
        manager.refresh();
        manager.window.present();
    }

    fn open_folder(&self) {
        let file = gio::File::for_path(self.store_dir());
        let parent = self.manager().map(|m| m.window.clone().upcast::<gtk::Window>());
        gtk::FileLauncher::new(Some(&file)).launch(parent.as_ref(), gio::Cancellable::NONE, |_| {});
    }

    fn about(&self) {
        let about = adw::AboutDialog::builder()
            .application_name(APP_NAME)
            .version(VERSION)
            .developer_name("giosuel")
            .application_icon(APP_ID)
            .license_type(gtk::License::Agpl30)
            .website("https://realer-xpad.giosuel.ch/")
            .issue_url("https://github.com/giosuel/realer-xpad/issues")
            .comments("Sticky notes with Markdown and drawing.\nPowered by Rust 🦀")
            .build();
        about.present(self.dialog_parent().as_ref());
    }

    // -- Hyprland -------------------------------------------------------

    fn on_note_mapped(&self, id: &str) {
        let Some(hypr) = self.hypr() else { return };
        let Some(win) = self.0.windows.borrow().get(id).cloned() else { return };
        let title = win.initial_title();
        let mut attempts = 0;
        glib::timeout_add_local(Duration::from_millis(50), move || {
            attempts += 1;
            let Some(client) = hypr.find(&title, None) else {
                return if attempts < PLACE_RETRIES { glib::ControlFlow::Continue } else { glib::ControlFlow::Break };
            };
            let note = win.note.borrow().clone();
            let address = client["address"].as_str().unwrap_or_default();
            hypr.place(address, note.x.zip(note.y), note.width, note.height);
            if note.pinned {
                let mut floating = client.clone();
                floating["floating"] = true.into();
                hypr.set_pinned(&floating, true);
            }
            win.window.set_title(Some(&note.title()));
            glib::ControlFlow::Break
        });
    }

    pub fn apply_pin(&self, win: &NoteWindow) {
        if let Some(hypr) = self.hypr()
            && let Some(client) = hypr.find(&win.initial_title(), None)
        {
            hypr.set_pinned(&client, win.note.borrow().pinned);
        }
    }

    /// Copy the window's compositor geometry into its note. Returns true if changed.
    pub fn sync_geometry(&self, win: &NoteWindow) -> bool {
        let Some(hypr) = self.hypr() else { return false };
        let Some(client) = hypr.find(&win.initial_title(), None) else { return false };
        let (x, y, w, h) = Hyprland::geometry(&client);
        let mut note = win.note.borrow_mut();
        if (note.x, note.y, note.width, note.height) == (Some(x), Some(y), w, h) {
            return false;
        }
        (note.x, note.y, note.width, note.height) = (Some(x), Some(y), w, h);
        true
    }

    fn poll_geometry(&self) {
        for win in self.windows() {
            if win.window.is_visible() && self.sync_geometry(&win) {
                self.save_note(&win.note);
            }
        }
    }
}

pub fn run() -> glib::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    App::new(None, true, Some(APP_ID)).run(&args)
}
