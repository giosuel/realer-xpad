use realer_xpad::{APP_NAME, VERSION, app, cli};

fn main() -> gtk::glib::ExitCode {
    // Answer these without starting GTK, so they work without a display.
    match std::env::args().nth(1).as_deref() {
        Some("-h" | "--help" | "help") => print!("{}", cli::usage()),
        Some("-v" | "--version") => println!("{APP_NAME} {VERSION}"),
        _ => return app::run(),
    }
    gtk::glib::ExitCode::SUCCESS
}
