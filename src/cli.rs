//! Command-line help, shared by the binary and the running instance.

use crate::APP_NAME;

pub const COMMANDS: [(&str, &str); 7] = [
    ("show", "Show all open notes (default)"),
    ("new", "Create a new note"),
    ("toggle", "Hide all notes, or show them if hidden"),
    ("hide", "Hide all notes"),
    ("manager", "Open the notes manager"),
    ("import-xpad", "Import notes from xpad (~/.config/xpad)"),
    ("quit", "Save everything and quit"),
];

pub fn usage() -> String {
    let mut out =
        format!("Usage: realer-xpad [COMMAND]\n\n{APP_NAME} – sticky notes with Markdown and drawing.\n\nCommands:\n");
    for (name, desc) in COMMANDS {
        out += &format!("  {name:<12} {desc}\n");
    }
    out
}
