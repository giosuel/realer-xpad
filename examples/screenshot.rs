//! Render demo notes to PNGs (and a collage) for docs and visual checks.
//!
//! Run inside a Wayland session (e.g. headless sway):
//!
//! ```sh
//! tools/headless.sh cargo run --example screenshot -- docs [--dark]
//! ```

use std::cell::RefCell;
use std::f64::consts::PI;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::{glib, graphene};
use realer_xpad::app::App;
use realer_xpad::drawing::{Point, Stroke};
use realer_xpad::store::{Note, NoteStore};

const SHOPPING: &str = "# Weekend\n- [x] Coffee beans\n- [x] Oat milk\n- [ ] Sourdough\n- [ ] **Basil** for pesto\n";
const IDEAS: &str = "## Project ideas\nThings to try with *Realer Xpad*:\n\n1. Markdown that **renders live**\n\
2. Draw with a ~~mouse~~ pen tablet\n3. Pin notes on [Hyprland](https://hyprland.org)\n\n\
> Ideas are cheap, shipping is not.\n\nRun `realer-xpad new` from a keybind.\n";

/// Interpolate like real pointer input (which reports every few pixels).
fn densify(corners: &[(f64, f64)]) -> Vec<Point> {
    let mut pts = Vec::new();
    for w in corners.windows(2) {
        let ((ax, ay), (bx, by)) = (w[0], w[1]);
        let n = (((bx - ax).hypot(by - ay) / 4.0) as usize).max(1);
        pts.extend((0..n).map(|i| (ax + (bx - ax) * i as f64 / n as f64, ay + (by - ay) * i as f64 / n as f64, 1.0)));
    }
    let last = corners[corners.len() - 1];
    pts.push((last.0, last.1, 1.0));
    pts
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Vec<Point> {
    densify(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y + 1.0)])
}

fn wave(x0: f64, y0: f64, len: f64, amp: f64, pressure: bool) -> Vec<Point> {
    (0..=40)
        .map(|i| {
            let t = i as f64 / 40.0;
            let p = if pressure { 0.3 + 0.7 * (PI * t).sin() } else { 1.0 };
            (x0 + len * t, y0 + amp * (t * PI * 3.0).sin(), p)
        })
        .collect()
}

fn demo_notes() -> Vec<Note> {
    let sketch = vec![
        Stroke::new("pen", "ink", 2.0, rect(24.0, 70.0, 120.0, 80.0)),
        Stroke::new("pen", "ink", 2.0, rect(160.0, 70.0, 100.0, 36.0)),
        Stroke::new("pen", "ink", 2.0, rect(160.0, 116.0, 100.0, 34.0)),
        Stroke::new("highlighter", "#f76b15", 4.0, wave(30.0, 185.0, 220.0, 4.0, false)),
        Stroke::new("pen", "#3e63dd", 4.0, wave(40.0, 230.0, 200.0, 14.0, true)),
        Stroke::new("pen", "#e5484d", 4.0, densify(&[(70.0, 110.0), (80.0, 122.0), (104.0, 92.0)])),
    ];
    let note = |text: &str, color: &str, w, h, created| Note {
        text: text.into(),
        color: color.into(),
        width: w,
        height: h,
        created,
        ..Default::default()
    };
    vec![
        note(SHOPPING, "yellow", 280, 250, 1.0),
        note(IDEAS, "lavender", 320, 390, 2.0),
        Note { mode: "draw".into(), strokes: sketch, ..note("### Layout sketch\n", "mint", 290, 330, 3.0) },
        note("", "sky", 260, 200, 4.0),
    ]
}

fn render_widget(widget: &impl IsA<gtk::Widget>, path: &Path) {
    let (w, h) = (widget.width(), widget.height());
    let snap = gtk::Snapshot::new();
    gtk::WidgetPaintable::new(Some(widget)).snapshot(&snap, w as f64, h as f64);
    let node = snap.to_node().expect("something to render");
    let renderer = widget.native().unwrap().renderer().unwrap();
    let texture = renderer.render_texture(node, Some(&graphene::Rect::new(0.0, 0.0, w as f32, h as f32)));
    texture.save_to_png(path).expect("write png");
}

fn collage(shots: &[PathBuf], dest: &Path, dark: bool) {
    let bg = if dark { ("#1b1d26", "#2a2140") } else { ("#e8ecf7", "#f6e9f2") };
    let shadow = ["(", "+clone", "-background", "black", "-shadow", "35x12+0+8", ")", "+swap", "-background", "none"]
        .into_iter()
        .chain(["-layers", "merge", "+repage"])
        .map(String::from)
        .collect::<Vec<_>>();
    let mut args: Vec<String> = vec!["-background".into(), "none".into()];
    let framed = |args: &mut Vec<String>, img: &Path| {
        args.push("(".into());
        args.push(img.display().to_string());
        args.extend(shadow.iter().cloned());
        args.push(")".into());
    };
    for img in &shots[..3] {
        framed(&mut args, img);
    }
    args.extend(["-gravity", "north", "+smush", "-30"].map(String::from));
    framed(&mut args, &shots[3]);
    framed(&mut args, &shots[4]);
    args.extend(["+smush", "-30", "-append", "-gravity", "center", "(", "+clone", "-alpha", "off"].map(String::from));
    args.extend(["-fill".into(), bg.0.into(), "-colorize".into(), "100".into(), "-sparse-color".into()]);
    args.extend(["barycentric".into(), format!("0,0 {} %[fx:w],%[fx:h] {}", bg.0, bg.1), ")".into()]);
    args.extend(["+swap", "-compose", "over", "-composite", "-bordercolor"].map(String::from));
    args.extend([bg.0.into(), "-border".into(), "36".into(), dest.display().to_string()]);
    if let Err(err) = Command::new("magick").args(&args).status() {
        eprintln!("collage skipped (ImageMagick not found): {err}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dark = args.iter().any(|a| a == "--dark");
    let out = PathBuf::from(args.iter().find(|a| !a.starts_with("--")).map_or("screenshots", String::as_str));
    std::fs::create_dir_all(&out).unwrap();
    let suffix = if dark { "-dark" } else { "" };

    let dir = tempfile_dir();
    let store = NoteStore::new(&dir).unwrap();
    for mut note in demo_notes() {
        store.save(&mut note, false).unwrap();
    }
    let app = App::new(Some(store), false, Some(realer_xpad::APP_ID));
    let shots = Rc::new(RefCell::new(Vec::new()));

    app.adw().connect_startup(move |_| {
        let scheme = if dark { adw::ColorScheme::ForceDark } else { adw::ColorScheme::ForceLight };
        adw::StyleManager::default().set_color_scheme(scheme);
    });
    let (app2, shots2, out2) = (app.clone(), shots.clone(), out.clone());
    let app3 = app.clone();
    glib::timeout_add_local_once(Duration::from_millis(1200), move || {
        // Show the header controls regardless of which note the compositor
        // focused; styles apply on the next frame, so capture a bit later.
        for win in app3.windows() {
            win.window.add_css_class("hover");
        }
    });
    glib::timeout_add_local_once(Duration::from_millis(1500), move || {
        let wins = app2.windows();
        for (i, win) in wins.iter().enumerate() {
            let path = out2.join(format!("note{}{suffix}.png", i + 1));
            render_widget(&win.window, &path);
            shots2.borrow_mut().push(path);
        }
        app2.open_manager();
        glib::timeout_add_local_once(Duration::from_millis(600), move || {
            let path = out2.join(format!("manager{suffix}.png"));
            render_widget(&app2.manager().unwrap().window, &path);
            shots2.borrow_mut().push(path);
            app2.adw().quit();
        });
    });
    app.run(&["realer-xpad".into()]);

    let shots = shots.borrow();
    collage(&shots, &out.join(format!("collage{suffix}.png")), dark);
    for shot in shots.iter() {
        println!("{}", shot.display());
    }
    let _ = std::fs::remove_dir_all(dir);
}

fn tempfile_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rx-shot-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
