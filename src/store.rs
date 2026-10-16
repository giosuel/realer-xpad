//! Note persistence.
//!
//! Each note is stored as two files in the data directory:
//!
//! * `<id>.md`   – the note's Markdown text, readable with any editor
//! * `<id>.json` – metadata (colour, size, position, drawing strokes, ...)
//!
//! Deleted notes are moved to `trash/` instead of being removed outright.
//! Files are only rewritten when their content changed. If a note's `.md`
//! was edited elsewhere while the app also changed it, the other version is
//! kept in `trash/` rather than overwritten.
//! The format is the same one the original Python version used.

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::drawing::Stroke;

/// App settings, beside the notes so they follow `REALER_XPAD_DATA`.
const SETTINGS: &str = "settings.conf";

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
    /// What the files on disk hold, as of the last load or save.
    #[serde(skip)]
    pub disk: Option<OnDisk>,
}

/// A note's last loaded or saved state: text, strokes and metadata as JSON.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OnDisk {
    text: String,
    strokes: String,
    meta: String,
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
            disk: None,
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
        fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
        Ok(NoteStore { dir })
    }

    fn md(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.md"))
    }

    fn json(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    /// A setting from `settings.conf` (`key=value` lines) in the notes folder.
    pub fn setting(&self, key: &str) -> Option<String> {
        let text = fs::read_to_string(self.dir.join(SETTINGS)).ok()?;
        text.lines().find_map(|l| l.split_once('=').filter(|(k, _)| k.trim() == key).map(|(_, v)| v.trim().to_string()))
    }

    pub fn set_setting(&self, key: &str, value: &str) -> io::Result<()> {
        let path = self.dir.join(SETTINGS);
        let old = fs::read_to_string(&path).unwrap_or_default();
        let mut lines: Vec<String> =
            old.lines().filter(|l| l.split_once('=').is_none_or(|(k, _)| k.trim() != key)).map(String::from).collect();
        lines.push(format!("{key}={value}"));
        atomic_write(&path, &(lines.join("\n") + "\n"))
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
        note.text = match fs::read_to_string(self.md(id)) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => String::new(),
            Err(err) => {
                // Not UTF-8, or unreadable: leave it alone rather than load it
                // empty and later save that over it.
                eprintln!("realer-xpad: skipping note {id}: {}: {err}", self.md(id).display());
                return None;
            }
        };
        if !COLORS.contains(&note.color.as_str()) {
            note.color = DEFAULT_COLOR.into();
        }
        note.disk = Some(OnDisk { text: note.text.clone(), strokes: strokes_json(&note)?, meta: meta_json(&note)? });
        Some(note)
    }

    /// Write whatever changed. Changing the text or drawing updates `updated`;
    /// colour, size, position and visibility don't. Returns whether anything
    /// was written.
    pub fn save(&self, note: &mut Note) -> io::Result<bool> {
        let strokes = strokes_json(note).ok_or_else(|| io::Error::other("strokes"))?;
        let disk = note.disk.take();
        let edited = disk.as_ref().is_some_and(|d| d.text != note.text || d.strokes != strokes);
        if edited {
            note.updated = now();
        }
        let result = self.write(note, disk.as_ref(), strokes);
        if result.is_err() {
            note.disk = disk; // try again next time
        }
        result
    }

    fn write(&self, note: &mut Note, disk: Option<&OnDisk>, strokes: String) -> io::Result<bool> {
        let md = self.md(&note.id);
        let mut wrote = false;
        if disk.is_none_or(|d| d.text != note.text) {
            if let Some(d) = disk
                && let Ok(current) = fs::read_to_string(&md)
                && current != d.text
            {
                // Edited elsewhere since we last read it: keep that version too.
                self.trash_copy(&md, &format!("{}-{}.md", note.id, now() as u64))?;
            }
            atomic_write(&md, &note.text)?;
            wrote = true;
        }
        let meta = meta_json(note).ok_or_else(|| io::Error::other("metadata"))?;
        if disk.is_none_or(|d| d.meta != meta) {
            atomic_write(&self.json(&note.id), &meta)?;
            wrote = true;
        }
        note.disk = Some(OnDisk { text: note.text.clone(), strokes, meta });
        Ok(wrote)
    }

    fn trash(&self) -> io::Result<PathBuf> {
        let trash = self.dir.join("trash");
        fs::DirBuilder::new().recursive(true).mode(0o700).create(&trash)?;
        Ok(trash)
    }

    fn trash_copy(&self, path: &Path, name: &str) -> io::Result<()> {
        fs::copy(path, self.trash()?.join(name)).map(drop)
    }

    pub fn delete(&self, id: &str) -> io::Result<()> {
        let trash = self.trash()?;
        // Metadata first: a note without its .json no longer loads, while a
        // .json without its .md would load as an empty note.
        for path in [self.json(id), self.md(id)] {
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

fn strokes_json(note: &Note) -> Option<String> {
    serde_json::to_string(&note.strokes).ok()
}

fn meta_json(note: &Note) -> Option<String> {
    serde_json::to_string_pretty(note).ok()
}

/// Write to a temporary file, flush it to disk, then rename it over `path`, so a
/// crash leaves either the old or the new content, never an empty file.
pub(crate) fn atomic_write(path: &Path, content: &str) -> io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let mut file = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    fs::rename(&tmp, path)?;
    if let Some(dir) = path.parent() {
        fs::File::open(dir)?.sync_all()?;
    }
    Ok(())
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
        store.save(&mut note).unwrap();
        assert_eq!(fs::read_to_string(store.md(&note.id)).unwrap(), "# Hi\nthere");
        assert_eq!(store.load(&note.id).unwrap(), note);
    }

    #[test]
    fn settings_are_kept_beside_the_notes() {
        let (_d, store) = store();
        assert_eq!(store.setting("text-scale"), None);
        store.set_setting("text-scale", "1.25").unwrap();
        store.set_setting("other", "x").unwrap();
        store.set_setting("text-scale", "0.9").unwrap();
        assert_eq!(store.setting("text-scale").as_deref(), Some("0.9"));
        assert_eq!(store.setting("other").as_deref(), Some("x"));
        assert!(store.load_all().is_empty(), "settings are not a note");
    }

    #[test]
    fn load_all_sorted_by_creation() {
        let (_d, store) = store();
        let mut a = Note { text: "a".into(), created: 2.0, ..Default::default() };
        let mut b = Note { text: "b".into(), created: 1.0, ..Default::default() };
        store.save(&mut a).unwrap();
        store.save(&mut b).unwrap();
        let texts: Vec<_> = store.load_all().into_iter().map(|n| n.text).collect();
        assert_eq!(texts, ["b", "a"]);
    }

    #[test]
    fn delete_moves_to_trash_and_restore() {
        let (_d, store) = store();
        let mut note = Note { text: "bye".into(), ..Default::default() };
        store.save(&mut note).unwrap();
        store.delete(&note.id).unwrap();
        assert!(store.load_all().is_empty());
        assert!(store.dir.join("trash").join(format!("{}.md", note.id)).exists());
        assert_eq!(store.restore(&note.id).unwrap().text, "bye");
    }

    #[test]
    fn unreadable_text_is_skipped_and_left_alone() {
        let (_d, store) = store();
        let mut note = Note { text: "x".into(), ..Default::default() };
        store.save(&mut note).unwrap();
        fs::write(store.md(&note.id), b"caf\xe9 latin-1").unwrap();
        assert!(store.load_all().is_empty(), "not loaded as an empty note");
        assert_eq!(fs::read(store.md(&note.id)).unwrap(), b"caf\xe9 latin-1");
    }

    #[test]
    fn only_changes_are_written_and_only_edits_count_as_edits() {
        let (_d, store) = store();
        let mut note = Note { text: "a".into(), updated: 1.0, ..Default::default() };
        assert!(store.save(&mut note).unwrap(), "a new note is written");
        assert!(!store.save(&mut note).unwrap(), "nothing changed");
        note.width = 400;
        assert!(store.save(&mut note).unwrap());
        assert_eq!(note.updated, 1.0, "resizing isn't an edit");
        note.text = "b".into();
        store.save(&mut note).unwrap();
        assert!(note.updated > 1.0);
        let mut loaded = store.load(&note.id).unwrap();
        assert!(!store.save(&mut loaded).unwrap(), "loading then saving changes nothing");
    }

    #[test]
    fn edits_made_elsewhere_are_kept() {
        let (_d, store) = store();
        let mut note = Note { text: "mine".into(), ..Default::default() };
        store.save(&mut note).unwrap();
        fs::write(store.md(&note.id), "edited in vim").unwrap();
        note.width = 500;
        store.save(&mut note).unwrap();
        assert_eq!(fs::read_to_string(store.md(&note.id)).unwrap(), "edited in vim", "text untouched");
        note.text = "mine, changed".into();
        store.save(&mut note).unwrap();
        let kept: Vec<_> = fs::read_dir(store.dir.join("trash")).unwrap().flatten().map(|e| e.path()).collect();
        assert_eq!(kept.len(), 1);
        assert_eq!(fs::read_to_string(&kept[0]).unwrap(), "edited in vim");
    }

    #[test]
    fn notes_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let store = NoteStore::new(d.path().join("notes")).unwrap();
        let mut note = Note::default();
        store.save(&mut note).unwrap();
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&store.dir), 0o700);
        assert_eq!(mode(&store.md(&note.id)), 0o600);
        assert_eq!(mode(&store.json(&note.id)), 0o600);
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
