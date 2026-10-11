//! Note persistence.
//!
//! Each note is stored as two files in the data directory:
//!
//! * `<id>.md`   – the note's Markdown text, readable with any editor
//! * `<id>.json` – metadata (colour, size, position, drawing strokes, ...)
//!
//! Deleted notes are moved to `trash/` instead of being removed outright.
//! The format is the same one the original Python version used.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::drawing::Stroke;

pub const COLORS: [&str; 7] = ["yellow", "peach", "pink", "lavender", "sky", "mint", "graphite"];
pub const DEFAULT_COLOR: &str = "yellow";
pub const DEFAULT_SIZE: (i32, i32) = (300, 300);

pub fn now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// A random 128-bit hex id, like Python's `uuid4().hex`.
pub fn new_id() -> String {
    let mut bytes = [0u8; 16];
    if fs::File::open("/dev/urandom").and_then(|mut f| io::Read::read_exact(&mut f, &mut bytes)).is_err() {
        // Fall back to time + address entropy; ids only need to be unique locally.
        let seed = now().to_bits() ^ (&bytes as *const _ as u64);
        bytes[..8].copy_from_slice(&seed.to_le_bytes());
        bytes[8..].copy_from_slice(&seed.rotate_left(29).wrapping_mul(0x9E37_79B9_7F4A_7C15).to_le_bytes());
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn default_data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("REALER_XPAD_DATA") {
        return PathBuf::from(dir);
    }
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share"));
    base.join("realer-xpad")
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Note {
    pub id: String,
    #[serde(skip)]
    pub text: String,
    pub color: String,
    pub width: i32,
    pub height: i32,
    /// Only known/restorable on compositors that expose it (Hyprland).
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub pinned: bool,
    pub visible: bool,
    /// `"text"` or `"draw"`.
    pub mode: String,
    pub strokes: Vec<Stroke>,
    pub created: f64,
    pub updated: f64,
}

impl Default for Note {
    fn default() -> Self {
        let t = now();
        Note {
            id: new_id(),
            text: String::new(),
            color: DEFAULT_COLOR.into(),
            width: DEFAULT_SIZE.0,
            height: DEFAULT_SIZE.1,
            x: None,
            y: None,
            pinned: false,
            visible: true,
            mode: "text".into(),
            strokes: Vec::new(),
            created: t,
            updated: t,
        }
    }
}

impl Note {
    pub fn title(&self) -> String {
        for line in self.text.lines() {
            let line =
                line.trim().trim_start_matches(['#', '>', '-', '*', '+', ' ']).replace("[ ] ", "").replace("[x] ", "");
            let line = line.trim();
            if !line.is_empty() {
                return line.chars().take(60).collect();
            }
        }
        if self.strokes.is_empty() { "Empty note" } else { "Drawing" }.into()
    }

    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty() && self.strokes.is_empty()
    }
}

pub struct NoteStore {
    pub dir: PathBuf,
}

impl NoteStore {
    pub fn new(dir: impl Into<PathBuf>) -> io::Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        Ok(NoteStore { dir })
    }

    fn md(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.md"))
    }

    fn json(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    pub fn load_all(&self) -> Vec<Note> {
        let mut notes: Vec<Note> = fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                (path.extension()? == "json").then(|| self.load(path.file_stem()?.to_str()?))?
            })
            .collect();
        notes.sort_by(|a, b| a.created.total_cmp(&b.created));
        notes
    }

    pub fn load(&self, id: &str) -> Option<Note> {
        let meta = fs::read_to_string(self.json(id)).ok()?;
        let mut note: Note = serde_json::from_str(&meta).ok()?;
        note.id = id.into();
        note.text = fs::read_to_string(self.md(id)).unwrap_or_default();
        if !COLORS.contains(&note.color.as_str()) {
            note.color = DEFAULT_COLOR.into();
        }
        Some(note)
    }

    pub fn save(&self, note: &mut Note, touch: bool) -> io::Result<()> {
        if touch {
            note.updated = now();
        }
        atomic_write(&self.md(&note.id), &note.text)?;
        let meta = serde_json::to_string_pretty(note).map_err(io::Error::other)?;
        atomic_write(&self.json(&note.id), &meta)
    }

    pub fn delete(&self, id: &str) -> io::Result<()> {
        let trash = self.dir.join("trash");
        fs::create_dir_all(&trash)?;
        for path in [self.md(id), self.json(id)] {
            if path.exists() {
                fs::rename(&path, trash.join(path.file_name().unwrap()))?;
            }
        }
        Ok(())
    }

    pub fn restore(&self, id: &str) -> Option<Note> {
        let trash = self.dir.join("trash");
        for name in [format!("{id}.md"), format!("{id}.json")] {
            if trash.join(&name).exists() {
                fs::rename(trash.join(&name), self.dir.join(&name)).ok()?;
            }
        }
        self.load(id)
    }
}

fn atomic_write(path: &Path, content: &str) -> io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, NoteStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = NoteStore::new(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn save_and_load_roundtrip() {
        let (_d, store) = store();
        let stroke = Stroke::new("pen", "ink", 2.0, vec![(1.0, 2.0, 1.0)]);
        let mut note = Note {
            text: "# Hi\nthere".into(),
            color: "mint".into(),
            x: Some(10),
            y: Some(20),
            strokes: vec![stroke],
            ..Default::default()
        };
        store.save(&mut note, true).unwrap();
        assert_eq!(fs::read_to_string(store.md(&note.id)).unwrap(), "# Hi\nthere");
        assert_eq!(store.load(&note.id).unwrap(), note);
    }

    #[test]
    fn load_all_sorted_by_creation() {
        let (_d, store) = store();
        let mut a = Note { text: "a".into(), created: 2.0, ..Default::default() };
        let mut b = Note { text: "b".into(), created: 1.0, ..Default::default() };
        store.save(&mut a, true).unwrap();
        store.save(&mut b, true).unwrap();
        let texts: Vec<_> = store.load_all().into_iter().map(|n| n.text).collect();
        assert_eq!(texts, ["b", "a"]);
    }

    #[test]
    fn delete_moves_to_trash_and_restore() {
        let (_d, store) = store();
        let mut note = Note { text: "bye".into(), ..Default::default() };
        store.save(&mut note, true).unwrap();
        store.delete(&note.id).unwrap();
        assert!(store.load_all().is_empty());
        assert!(store.dir.join("trash").join(format!("{}.md", note.id)).exists());
        assert_eq!(store.restore(&note.id).unwrap().text, "bye");
    }

    #[test]
    fn corrupt_and_unknown_fields_are_tolerated() {
        let (_d, store) = store();
        fs::write(store.dir.join("bad.json"), "{not json").unwrap();
        fs::write(store.dir.join("old.json"), r#"{"color": "neon", "future_field": 1}"#).unwrap();
        let notes = store.load_all();
        assert_eq!(notes.len(), 1);
        assert_eq!((notes[0].id.as_str(), notes[0].color.as_str(), notes[0].text.as_str()), ("old", "yellow", ""));
    }

    #[test]
    fn reads_notes_written_by_the_python_version() {
        let (_d, store) = store();
        let meta = r#"{"id": "abc", "color": "sky", "width": 280, "height": 250, "x": null, "y": null,
            "pinned": true, "visible": false, "mode": "draw",
            "strokes": [{"tool": "pen", "color": "ink", "width": 2.0, "points": [[1.0, 2.0, 0.5]]}],
            "created": 1.5, "updated": 2.5}"#;
        fs::write(store.dir.join("abc.json"), meta).unwrap();
        fs::write(store.dir.join("abc.md"), "- [ ] hi").unwrap();
        let note = store.load("abc").unwrap();
        assert_eq!((note.color.as_str(), note.width, note.pinned, note.visible), ("sky", 280, true, false));
        assert_eq!(note.strokes[0].points, [(1.0, 2.0, 0.5)]);
        assert_eq!(note.text, "- [ ] hi");
    }

    #[test]
    fn titles() {
        let title = |text: &str| Note { text: text.into(), ..Default::default() }.title();
        assert_eq!(title("\n## Shopping list\n- milk"), "Shopping list");
        assert_eq!(title("- [ ] call mom"), "call mom");
        assert_eq!(title(""), "Empty note");
        let drawing = Note { strokes: vec![Stroke::new("pen", "ink", 1.0, vec![])], ..Default::default() };
        assert_eq!(drawing.title(), "Drawing");
    }

    #[test]
    fn timestamps_roundtrip_exactly() {
        for i in 0..1000 {
            let note = Note { created: 1.791748070436144e9 + i as f64 * 0.000_137, ..Default::default() };
            let back: Note = serde_json::from_str(&serde_json::to_string(&note).unwrap()).unwrap();
            assert_eq!(back.created.to_bits(), note.created.to_bits(), "{}", note.created);
        }
    }

    #[test]
    fn ids_are_unique_hex() {
        let (a, b) = (new_id(), new_id());
        assert_ne!(a, b);
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
