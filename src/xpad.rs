//! Import notes from xpad.
//!
//! xpad keeps its notes in `$XDG_CONFIG_HOME/xpad`:
//!
//! * `info-XXXXXX`    – one per note, `key value` lines: `width`, `height`, `x`, `y`,
//!   `sticky`, `hidden`, `follow_color`, `back`, `text`, `content`, ...
//! * `content-XXXXXX` – the note's plain text, named by the `content` key
//! * `default-style`  – default colours, used by notes with `follow_color 1`
//!
//! Importing copies notes over and never changes xpad's files. The names of
//! imported info files are remembered in `.xpad-imported` in our data directory,
//! so importing again only picks up new notes.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::store::{DEFAULT_SIZE, Note, NoteStore, now};

/// xpad's built-in note colour (a light yellow), used when nothing else is set.
const XPAD_DEFAULT_BACK: (u8, u8, u8) = (255, 238, 153);
const MARKER: &str = ".xpad-imported";

pub fn default_dir() -> PathBuf {
    // Inside Flatpak, XDG_CONFIG_HOME is the app's own folder; xpad's notes are
    // in the host's (see `--filesystem=xdg-config/xpad:ro`).
    let config = if std::env::var_os("FLATPAK_ID").is_some() {
        std::env::var_os("HOST_XDG_CONFIG_HOME")
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
    };
    let base = config
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
    base.join("xpad")
}

/// A note as xpad stores it.
#[derive(Clone, Debug, PartialEq)]
pub struct XpadNote {
    /// Name of the `info-XXXXXX` file, which identifies the note.
    pub info: String,
    pub text: String,
    pub back: (u8, u8, u8),
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub sticky: bool,
    pub hidden: bool,
    /// Last change of the note's text, in seconds since the epoch.
    pub modified: Option<f64>,
}

/// The value of `key` in an xpad `key value` file. Like xpad, the first match wins.
fn value<'a>(file: &'a str, key: &str) -> Option<&'a str> {
    file.lines().find_map(|line| line.strip_prefix(key)?.strip_prefix(' ')).map(str::trim)
}

fn flag(file: &str, key: &str, default: bool) -> bool {
    value(file, key).and_then(|v| v.parse::<i64>().ok()).map_or(default, |v| v != 0)
}

/// Parse a colour the way xpad writes it (`rgb(r,g,b)`, `rgba(r,g,b,a)`) or as `#rgb`/`#rrggbb`.
pub fn parse_color(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        let digits: Vec<u8> = hex.chars().map(|c| c.to_digit(16).map(|d| d as u8)).collect::<Option<_>>()?;
        return match digits.len() {
            3 => Some((digits[0] * 17, digits[1] * 17, digits[2] * 17)),
            6 | 12 => {
                let n = digits.len() / 3;
                let channel = |i: usize| digits[i * n] * 16 + digits[i * n + 1];
                Some((channel(0), channel(1), channel(2)))
            }
            _ => None,
        };
    }
    let inner = s.strip_prefix("rgba(").or_else(|| s.strip_prefix("rgb("))?.strip_suffix(')')?;
    let parts: Vec<f64> = inner.split(',').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
    let [r, g, b] = parts.get(..3)? else { return None };
    let channel = |v: f64| v.clamp(0.0, 255.0).round() as u8;
    Some((channel(*r), channel(*g), channel(*b)))
}

/// The palette colour closest to an xpad background, by hue. Greys, and
/// colours too dark or too light to have a clear hue, become graphite.
pub fn nearest_color(rgb: (u8, u8, u8)) -> &'static str {
    let (h, s, l) = hsl(rgb);
    if s < 0.15 || !(0.12..=0.97).contains(&l) {
        return "graphite";
    }
    // Hues of the palette's accent colours (see theme.rs).
    const HUES: [(&str, f64); 6] =
        [("peach", 21.0), ("yellow", 42.0), ("mint", 151.0), ("sky", 211.0), ("lavender", 256.0), ("pink", 336.0)];
    let distance = |hue: f64| {
        let d = (h - hue).abs();
        d.min(360.0 - d)
    };
    HUES.iter().min_by(|a, b| distance(a.1).total_cmp(&distance(b.1))).map_or("yellow", |c| c.0)
}

fn hsl((r, g, b): (u8, u8, u8)) -> (f64, f64, f64) {
    let [r, g, b] = [r, g, b].map(|c| c as f64 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, s, l)
}

/// All notes in an xpad config directory, oldest first. Unreadable files are skipped.
pub fn read_notes(dir: &Path) -> Vec<XpadNote> {
    let style = fs::read_to_string(dir.join("default-style")).unwrap_or_default();
    // The default colour, unless the user picked "use theme colours" (use_back 0).
    let default_back = if flag(&style, "use_back", true) { value(&style, "back").and_then(parse_color) } else { None }
        .unwrap_or(XPAD_DEFAULT_BACK);

    let mut notes: Vec<XpadNote> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if !name.starts_with("info-") || name.ends_with('~') {
                return None; // xpad skips backup files too
            }
            let info = fs::read_to_string(entry.path()).ok()?;
            let content = value(&info, "content").map(|c| dir.join(c));
            let text = content.as_ref().and_then(|c| fs::read_to_string(c).ok()).unwrap_or_default();
            let modified = content
                .and_then(|c| fs::metadata(c).ok()?.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs_f64());
            // A pad's own colours apply only when it doesn't follow the default
            // style (or is locked, which xpad treats the same way).
            let own_colors = !flag(&info, "follow_color", true) || flag(&info, "locked", false);
            let back = own_colors.then(|| value(&info, "back").and_then(parse_color)).flatten().unwrap_or(default_back);
            let size = |key| value(&info, key).and_then(|v| v.parse::<i32>().ok()).filter(|v| *v > 0);
            Some(XpadNote {
                info: name,
                text,
                back,
                width: size("width"),
                height: size("height"),
                sticky: flag(&info, "sticky", false),
                hidden: flag(&info, "hidden", false),
                modified,
            })
        })
        .collect();
    notes.sort_by(|a, b| a.modified.unwrap_or(0.0).total_cmp(&b.modified.unwrap_or(0.0)).then(a.info.cmp(&b.info)));
    notes
}

impl XpadNote {
    pub fn to_note(&self) -> Note {
        let time = self.modified.unwrap_or_else(now);
        Note {
            text: self.text.clone(),
            color: nearest_color(self.back).into(),
            width: self.width.unwrap_or(DEFAULT_SIZE.0).max(160),
            height: self.height.unwrap_or(DEFAULT_SIZE.1).max(120),
            pinned: self.sticky,
            visible: !self.hidden,
            created: time,
            updated: time,
            ..Default::default()
        }
    }
}

fn imported(store: &NoteStore) -> HashSet<String> {
    let marker = fs::read_to_string(store.dir.join(MARKER)).unwrap_or_default();
    marker.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()
}

/// Whether the user was already offered (or ran) an xpad import.
pub fn was_offered(store: &NoteStore) -> bool {
    store.dir.join(MARKER).exists()
}

/// Remember that the user was offered an import, so they're not asked again.
pub fn mark_offered(store: &NoteStore) -> io::Result<()> {
    let marker = store.dir.join(MARKER);
    if !marker.exists() {
        fs::write(marker, "")?;
    }
    Ok(())
}

/// xpad notes in `dir` that have text and haven't been imported yet.
pub fn pending(store: &NoteStore, dir: &Path) -> Vec<XpadNote> {
    let done = imported(store);
    read_notes(dir).into_iter().filter(|n| !n.text.trim().is_empty() && !done.contains(&n.info)).collect()
}

/// Copy the pending xpad notes in `dir` into `store`. Returns the new notes.
pub fn import(store: &NoteStore, dir: &Path) -> io::Result<Vec<Note>> {
    let mut notes = Vec::new();
    let mut done: Vec<String> = imported(store).into_iter().collect();
    for xpad in pending(store, dir) {
        let mut note = xpad.to_note();
        store.save(&mut note, false)?;
        done.push(xpad.info);
        notes.push(note);
    }
    done.sort();
    fs::write(store.dir.join(MARKER), done.iter().map(|d| format!("{d}\n")).collect::<String>())?;
    Ok(notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Files as written by xpad 5.8 (Debian 13).
    const DEFAULT_STYLE: &str = "decorations 0\nheight 300\nwidth 300\nconfirm_destroy 1\nback rgb(255,238,153)\n\
        use_back 1\ntext rgb(0,0,0)\nuse_text 1\nfontname Sans 9\n";

    fn info(content: &str, extra: &str) -> String {
        format!(
            "width 377\nheight 280\nx 490\ny 362\nfollow_font 1\nfollow_color 1\nsticky 0\nhidden 0\n\
             back rgb(238,238,236)\ntext rgb(0,0,0)\nfontname Sans 9\ncontent {content}\n{extra}"
        )
    }

    fn xpad_dir(notes: &[(&str, &str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("default-style"), DEFAULT_STYLE).unwrap();
        for (id, text, info_text) in notes {
            fs::write(dir.path().join(format!("content-{id}")), text).unwrap();
            fs::write(dir.path().join(format!("info-{id}")), info_text).unwrap();
        }
        dir
    }

    fn store() -> (tempfile::TempDir, NoteStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = NoteStore::new(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn reads_values_like_xpad() {
        let file = "width 300\nfollow_color 0\nback rgb(1,2,3)\nfontname Sans Bold 9\nwidth 999\n";
        assert_eq!(value(file, "width"), Some("300"), "first match wins");
        assert_eq!(value(file, "fontname"), Some("Sans Bold 9"));
        assert_eq!(value(file, "font"), None, "keys match whole words");
        assert!(!flag(file, "follow_color", true));
        assert!(flag(file, "missing", true));
    }

    #[test]
    fn parses_colors() {
        assert_eq!(parse_color("rgb(255,238,153)"), Some((255, 238, 153)));
        assert_eq!(parse_color("rgba(10, 20, 30, 0.5)"), Some((10, 20, 30)));
        assert_eq!(parse_color("#fe9"), Some((255, 238, 153)));
        assert_eq!(parse_color("#ffee99"), Some((255, 238, 153)));
        assert_eq!(parse_color("#ffffeeee9999"), Some((255, 238, 153)));
        assert_eq!(parse_color("teal"), None);
        assert_eq!(parse_color("rgb(1,2)"), None);
    }

    #[test]
    fn maps_colors_to_the_palette() {
        let cases = [
            ((255, 238, 153), "yellow"), // xpad's default
            ((238, 238, 236), "graphite"),
            ((0, 0, 0), "graphite"),
            ((255, 255, 255), "graphite"),
            ((255, 180, 120), "peach"),
            ((255, 170, 200), "pink"),
            ((200, 180, 255), "lavender"),
            ((150, 200, 255), "sky"),
            ((150, 230, 180), "mint"),
            ((0, 160, 0), "mint"),
        ];
        for (rgb, color) in cases {
            assert_eq!(nearest_color(rgb), color, "{rgb:?}");
        }
    }

    #[test]
    fn reads_xpad_notes() {
        let dir = xpad_dir(&[
            ("AAAAAA", "Buy milk", &info("content-AAAAAA", "")),
            (
                "BBBBBB",
                "Ideas\n- one",
                "width 200\nheight 150\nfollow_color 0\nsticky 1\nhidden 1\nback rgb(150,200,255)\ncontent content-BBBBBB\n",
            ),
        ]);
        fs::write(dir.path().join("info-AAAAAA~"), "content content-AAAAAA\n").unwrap();
        let mut notes = read_notes(dir.path());
        notes.sort_by(|a, b| a.info.cmp(&b.info));
        assert_eq!(notes.len(), 2, "backup files are skipped");
        let (a, b) = (&notes[0], &notes[1]);
        assert_eq!((a.text.as_str(), a.back, a.width, a.height), ("Buy milk", (255, 238, 153), Some(377), Some(280)));
        assert_eq!((a.sticky, a.hidden), (false, false));
        assert_eq!((b.text.as_str(), b.back, b.sticky, b.hidden), ("Ideas\n- one", (150, 200, 255), true, true));
        assert!(a.modified.is_some());
    }

    #[test]
    fn follows_a_customised_default_style() {
        let dir = xpad_dir(&[("AAAAAA", "hi", &info("content-AAAAAA", ""))]);
        fs::write(dir.path().join("default-style"), "back rgb(200,180,255)\nuse_back 1\n").unwrap();
        assert_eq!(read_notes(dir.path())[0].back, (200, 180, 255));
        fs::write(dir.path().join("default-style"), "back rgb(200,180,255)\nuse_back 0\n").unwrap();
        assert_eq!(read_notes(dir.path())[0].back, XPAD_DEFAULT_BACK, "theme colours: fall back to xpad yellow");
    }

    #[test]
    fn converts_to_notes() {
        let xpad = XpadNote {
            info: "info-X".into(),
            text: "- [ ] call mom".into(),
            back: (255, 170, 200),
            width: Some(100),
            height: None,
            sticky: true,
            hidden: true,
            modified: Some(1_700_000_000.0),
        };
        let note = xpad.to_note();
        assert_eq!((note.text.as_str(), note.color.as_str()), ("- [ ] call mom", "pink"));
        assert_eq!((note.width, note.height), (160, DEFAULT_SIZE.1), "sizes are kept usable");
        assert_eq!((note.pinned, note.visible), (true, false));
        assert_eq!((note.created, note.updated), (1_700_000_000.0, 1_700_000_000.0));
        assert_eq!((note.x, note.y), (None, None), "positions from X11 don't carry over");
    }

    #[test]
    fn imports_once_and_skips_empty_notes() {
        let dir = xpad_dir(&[
            ("AAAAAA", "Buy milk", &info("content-AAAAAA", "")),
            ("EMPTY0", "  \n", &info("content-EMPTY0", "")),
        ]);
        let (_d, store) = store();
        assert!(!was_offered(&store));
        let notes = import(&store, dir.path()).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(store.load_all()[0].text, "Buy milk");
        assert!(was_offered(&store));
        assert_eq!(fs::read_to_string(dir.path().join("content-AAAAAA")).unwrap(), "Buy milk", "xpad files untouched");

        assert!(import(&store, dir.path()).unwrap().is_empty(), "nothing new the second time");
        fs::write(dir.path().join("content-CCCCCC"), "New one").unwrap();
        fs::write(dir.path().join("info-CCCCCC"), info("content-CCCCCC", "")).unwrap();
        assert_eq!(pending(&store, dir.path()).len(), 1);
        assert_eq!(import(&store, dir.path()).unwrap()[0].text, "New one");
        assert_eq!(store.load_all().len(), 2);
    }

    #[test]
    fn deleted_imports_stay_deleted() {
        let dir = xpad_dir(&[("AAAAAA", "Buy milk", &info("content-AAAAAA", ""))]);
        let (_d, store) = store();
        let note = import(&store, dir.path()).unwrap().remove(0);
        store.delete(&note.id).unwrap();
        assert!(import(&store, dir.path()).unwrap().is_empty());
    }

    #[test]
    fn missing_or_broken_xpad_dir() {
        let (_d, store) = store();
        assert!(read_notes(Path::new("/nonexistent/xpad")).is_empty());
        assert!(import(&store, Path::new("/nonexistent/xpad")).unwrap().is_empty());
        let dir = xpad_dir(&[]);
        fs::write(dir.path().join("info-NOCONT"), "width 300\n").unwrap();
        assert_eq!(read_notes(dir.path())[0].text, "", "a pad without content is empty, not an error");
    }
}
