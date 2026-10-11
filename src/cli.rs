//! Command-line help, shared by the binary and the running instance.

use crate::APP_NAME;

pub const COMMANDS: [(&str, &str); 6] = [
    ("show", "Show all open notes (default)"),
    ("new", "Create a new note"),
    ("toggle", "Hide all notes, or show them if hidden"),
    ("hide", "Hide all notes"),
    ("manager", "Open the notes manager"),
    ("quit", "Save everything and quit"),
];

pub fn usage() -> String {
    let mut out =
        format!("Usage: realer-xpad [COMMAND]\n\n{APP_NAME} – sticky notes with Markdown and drawing.\n\nCommands:\n");
    for (name, desc) in COMMANDS {
        out += &format!("  {name:<9} {desc}\n");
    }
    out
}
