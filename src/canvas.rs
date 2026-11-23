//! Drawing layer widget.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};

use gtk::prelude::*;
use gtk::{cairo, gdk};

use crate::drawing::{
    Drawing, Grab, HIGHLIGHTER_ALPHA, INK_COLORS, Picture, Point, ROTATE_ARM, Stroke, WIDTHS, arrow_head, fit,
    is_shape, smooth_points,
};

pub const ERASER_RADIUS: f64 = 8.0;
/// Selection outline and handles.
const SELECTION: (f64, f64, f64) = (0.208, 0.518, 0.894);
const HANDLE_SIZE: f64 = 10.0;
/// A new picture takes up at most this much of the note, unless it's smaller.
const PICTURE_SHARE: f64 = 0.8;

fn color(name: &str, ink: &gdk::RGBA, alpha: f64) -> (f64, f64, f64, f64) {
    let c = if name == "ink" { *ink } else { gdk::RGBA::parse(name).unwrap_or(*ink) };
    (c.red() as f64, c.green() as f64, c.blue() as f64, c.alpha() as f64 * alpha)
}

/// Smooth path through `points`: quadratic curves between midpoints (as cubics).
fn path(cr: &cairo::Context, points: &[Point]) {
    let (x0, y0, _) = points[0];
    cr.move_to(x0, y0);
    match points.len() {
        1 => cr.line_to(x0 + 0.01, y0), // a dot
        2 => cr.line_to(points[1].0, points[1].1),
        _ => {
            let (mut cx, mut cy) = (x0, y0);
            for w in points[1..].windows(2) {
                let (qx, qy) = (w[0].0, w[0].1);
                let (ex, ey) = ((w[0].0 + w[1].0) / 2.0, (w[0].1 + w[1].1) / 2.0);
                cr.curve_to(
                    cx + 2.0 / 3.0 * (qx - cx),
                    cy + 2.0 / 3.0 * (qy - cy),
                    ex + 2.0 / 3.0 * (qx - ex),
                    ey + 2.0 / 3.0 * (qy - ey),
                    ex,
                    ey,
                );
                (cx, cy) = (ex, ey);
            }
            let last = points[points.len() - 1];
            cr.line_to(last.0, last.1);
        }
    }
}

fn pressure_scale(p: f64) -> f64 {
    0.35 + 0.9 * p.clamp(0.0, 1.0)
}

/// A line, arrow, rectangle or ellipse between the stroke's first and last point.
fn draw_shape(cr: &cairo::Context, stroke: &Stroke) {
    let (a, b) = (stroke.points[0], stroke.points[stroke.points.len() - 1]);
    let ((x0, y0), (x1, y1)) = ((a.0, a.1), (b.0, b.1));
    match stroke.tool.as_str() {
        "rect" => cr.rectangle(x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs()),
        "ellipse" => {
            let (rx, ry) = ((x1 - x0).abs() / 2.0, (y1 - y0).abs() / 2.0);
            if rx > 0.0 && ry > 0.0 {
                // Scale a unit circle; restoring the matrix before stroking keeps the line even.
                cr.save().ok();
                cr.translate((x0 + x1) / 2.0, (y0 + y1) / 2.0);
                cr.scale(rx, ry);
                cr.arc(0.0, 0.0, 1.0, 0.0, std::f64::consts::TAU);
                cr.restore().ok();
            }
        }
        _ => {
            cr.move_to(x0, y0);
            cr.line_to(x1, y1);
            if stroke.tool == "arrow" {
                let (l, r) = arrow_head((x0, y0), (x1, y1), stroke.width);
                cr.move_to(l.0, l.1);
                cr.line_to(x1, y1);
                cr.line_to(r.0, r.1);
            }
        }
    }
    cr.set_line_width(stroke.width);
    let _ = cr.stroke();
}

pub fn draw_stroke(cr: &cairo::Context, stroke: &Stroke, ink: &gdk::RGBA) {
    let pts = &stroke.points;
    if pts.is_empty() {
        return;
    }
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_line_join(cairo::LineJoin::Round);
    if stroke.is_shape() {
        let (r, g, b, a) = color(&stroke.color, ink, 1.0);
        cr.set_source_rgba(r, g, b, a);
        draw_shape(cr, stroke);
        return;
    }
    let stroke_path = |width: f64, points: &[Point]| {
        path(cr, points);
        cr.set_line_width(width);
        let _ = cr.stroke();
    };
    if stroke.tool == "highlighter" {
        let (r, g, b, a) = color(&stroke.color, ink, HIGHLIGHTER_ALPHA);
        cr.set_source_rgba(r, g, b, a);
        stroke_path(stroke.effective_width(), pts);
        return;
    }
    let (r, g, b, a) = color(&stroke.color, ink, 1.0);
    cr.set_source_rgba(r, g, b, a);
    let first = pts[0].2;
    if pts.iter().all(|p| (p.2 - first).abs() < 0.005) {
        stroke_path(stroke.width * pressure_scale(first), pts);
    } else {
        // Pressure varies: draw short segments with their own widths.
        for w in pts.windows(2) {
            stroke_path(stroke.width * pressure_scale((w[0].2 + w[1].2) / 2.0), w);
        }
    }
}

pub struct Inner {
    pub area: gtk::DrawingArea,
    pub drawing: RefCell<Drawing>,
    pub tool: RefCell<String>,
    pub color: RefCell<String>,
    pub width: Cell<f64>,
    current: RefCell<Option<Stroke>>,
    erasing: Cell<bool>,
    start: Cell<(f64, f64)>,
    hover: Cell<Option<(f64, f64)>>,
    on_changed: RefCell<Vec<Box<dyn Fn()>>>,
    /// The notes folder, which pictures' file names are relative to.
    dir: RefCell<PathBuf>,
    /// Loaded pictures by file name (`None` if unreadable), and copies scaled
    /// down to the size they're shown at.
    surfaces: RefCell<HashMap<String, Option<cairo::ImageSurface>>>,
    scaled: RefCell<HashMap<String, cairo::ImageSurface>>,
    selected: Cell<Option<u64>>,
    /// The picture being dragged, as it was before, and by what.
    grab: RefCell<Option<(Grab, Picture)>>,
    /// Shift is held: rotation snaps.
    pub snap: Cell<bool>,
}

#[derive(Clone)]
pub struct Canvas(Rc<Inner>);

impl std::ops::Deref for Canvas {
    type Target = Inner;
    fn deref(&self) -> &Inner {
        &self.0
    }
}

impl Canvas {
    pub fn new(drawing: Drawing) -> Self {
        let area = gtk::DrawingArea::builder().hexpand(true).vexpand(true).build();
        area.set_cursor_from_name(Some("crosshair"));
        let canvas = Canvas(Rc::new(Inner {
            area,
            drawing: RefCell::new(drawing),
            tool: RefCell::new("pen".into()),
            color: RefCell::new(INK_COLORS[0].into()),
            width: Cell::new(WIDTHS[1]),
            current: RefCell::default(),
            erasing: Cell::new(false),
            start: Cell::new((0.0, 0.0)),
            hover: Cell::new(None),
            on_changed: RefCell::default(),
            dir: RefCell::default(),
            surfaces: RefCell::default(),
            scaled: RefCell::default(),
            selected: Cell::new(None),
            grab: RefCell::default(),
            snap: Cell::new(false),
        }));
        canvas.connect_signals();
        canvas
    }

    fn weak(&self) -> Weak<Inner> {
        Rc::downgrade(&self.0)
    }

    fn connect_signals(&self) {
        let weak = self.weak();
        self.area.set_draw_func(move |area, cr, _, _| {
            if let Some(inner) = weak.upgrade() {
                Canvas(inner).draw(area, cr);
            }
        });

        let drag = gtk::GestureDrag::new();
        let weak = self.weak();
        drag.connect_drag_begin(move |g, x, y| {
            if let Some(inner) = weak.upgrade() {
                let (pressure, eraser_tip) = event_info(g);
                Canvas(inner).begin(x, y, pressure, eraser_tip);
            }
        });
        let weak = self.weak();
        drag.connect_drag_update(move |g, dx, dy| {
            if let Some(inner) = weak.upgrade() {
                let c = Canvas(inner);
                c.snap.set(g.current_event_state().contains(gdk::ModifierType::SHIFT_MASK));
                let (sx, sy) = c.start.get();
                c.update(sx + dx, sy + dy, event_info(g).0);
            }
        });
        let weak = self.weak();
        drag.connect_drag_end(move |_, _, _| {
            if let Some(inner) = weak.upgrade() {
                Canvas(inner).end();
            }
        });
        self.area.add_controller(drag);

        let motion = gtk::EventControllerMotion::new();
        let weak = self.weak();
        motion.connect_motion(move |_, x, y| {
            if let Some(inner) = weak.upgrade() {
                Canvas(inner).set_hover(Some((x, y)));
            }
        });
        let weak = self.weak();
        motion.connect_leave(move |_| {
            if let Some(inner) = weak.upgrade() {
                Canvas(inner).set_hover(None);
            }
        });
        self.area.add_controller(motion);
    }

    // -- public API -----------------------------------------------------

    pub fn connect_changed(&self, f: impl Fn() + 'static) {
        self.on_changed.borrow_mut().push(Box::new(f));
    }

    /// A drawing tool, or `"text"`, where pictures can be selected and changed.
    pub fn set_tool(&self, tool: &str) {
        *self.tool.borrow_mut() = tool.into();
        let cursor = match tool {
            "text" => None,
            "eraser" => Some("none"),
            _ => Some("crosshair"),
        };
        self.area.set_cursor_from_name(cursor);
        if tool != "text" {
            self.select(None);
        }
        self.area.queue_draw();
    }

    pub fn undo(&self) {
        if self.drawing.borrow_mut().undo() {
            self.forget_lost_selection();
            self.changed();
        }
    }

    pub fn redo(&self) {
        if self.drawing.borrow_mut().redo() {
            self.forget_lost_selection();
            self.changed();
        }
    }

    // -- pictures ---------------------------------------------------------

    /// The notes folder, which pictures' file names are relative to.
    pub fn set_dir(&self, dir: &Path) {
        *self.dir.borrow_mut() = dir.into();
    }

    pub fn picture_path(&self, picture: &Picture) -> PathBuf {
        self.dir.borrow().join(&picture.file)
    }

    pub fn selected(&self) -> Option<Picture> {
        self.drawing.borrow().picture(self.selected.get()?).cloned()
    }

    pub fn select(&self, id: Option<u64>) {
        if self.selected.replace(id) != id {
            self.area.queue_draw();
            self.emit_changed();
        }
    }

    fn forget_lost_selection(&self) {
        if self.selected.get().is_some() && self.selected().is_none() {
            self.selected.set(None);
        }
    }

    /// What a press at `(x, y)` would take hold of with the text tool.
    pub fn grab_at(&self, x: f64, y: f64) -> Option<Grab> {
        if let Some(p) = self.selected()
            && let Some(grab) = p.grab_at(x, y, true)
        {
            return Some(grab);
        }
        self.drawing.borrow().picture_at(x, y).map(|_| Grab::Move)
    }

    pub fn grabbing(&self) -> bool {
        self.grab.borrow().is_some()
    }

    /// The cursor for a grab, with corner arrows turned along with the picture.
    pub fn cursor_for(&self, grab: Grab) -> &'static str {
        match grab {
            Grab::Move => "move",
            Grab::Rotate => "grab",
            Grab::Corner(i) => {
                let angle = self.selected().map_or(0.0, |p| p.angle);
                let turns = (angle / 90.0).round().rem_euclid(2.0) as usize;
                if (i + turns).is_multiple_of(2) { "nwse-resize" } else { "nesw-resize" }
            }
        }
    }

    /// The size a picture is shown at by default: its own, or less to fit the note.
    fn default_size(&self, file: &str) -> Option<(f64, f64)> {
        let s = self.surface(file)?;
        let (w, h) = (self.area.width().max(1) as f64, self.area.height().max(1) as f64);
        Some(fit((s.width() as f64, s.height() as f64), (w * PICTURE_SHARE, h * PICTURE_SHARE)))
    }

    /// Add the picture saved as `file` (relative to the notes folder), centred
    /// on `at` or the note, and select it. False if it can't be read.
    pub fn insert_picture(&self, file: &str, at: Option<(f64, f64)>) -> bool {
        let Some((w, h)) = self.default_size(file) else { return false };
        let (aw, ah) = (self.area.width() as f64, self.area.height() as f64);
        let (x, y) = at.unwrap_or((aw / 2.0, ah / 2.0));
        // Wholly inside the note where it fits.
        let inside =
            |v: f64, size: f64, max: f64| if size < max { v.clamp(size / 2.0, max - size / 2.0) } else { max / 2.0 };
        let picture = Picture::new(file, (inside(x, w, aw), inside(y, h, ah)), (w, h));
        let id = picture.id;
        self.drawing.borrow_mut().add_picture(picture);
        self.selected.set(Some(id));
        self.changed();
        true
    }

    pub fn delete_selected(&self) {
        if let Some(id) = self.selected.take() {
            self.drawing.borrow_mut().remove_picture(id);
            self.changed();
        }
    }

    /// Back to the size it was pasted at, unrotated.
    pub fn reset_selected(&self) {
        let Some(before) = self.selected() else { return };
        let Some((width, height)) = self.default_size(&before.file) else { return };
        let mut d = self.drawing.borrow_mut();
        d.set_picture(Picture { width, height, angle: 0.0, ..before.clone() });
        d.commit_picture(before);
        drop(d);
        self.changed();
    }

    /// Start dragging the picture (or handle) at `(x, y)`; elsewhere, unselect.
    fn begin_grab(&self, x: f64, y: f64) {
        let Some(grab) = self.grab_at(x, y) else {
            self.select(None);
            return;
        };
        let picture = match grab {
            Grab::Move if self.selected().is_none_or(|p| !p.contains(x, y)) => {
                self.drawing.borrow().picture_at(x, y).cloned()
            }
            _ => self.selected(),
        };
        let Some(picture) = picture else { return };
        self.select(Some(picture.id));
        *self.grab.borrow_mut() = Some((grab, picture));
    }

    fn update_grab(&self, x: f64, y: f64) {
        let Some((grab, before)) = self.grab.borrow().clone() else { return };
        let (sx, sy) = self.start.get();
        let (aw, ah) = (self.area.width() as f64, self.area.height() as f64);
        let picture = match grab {
            // The centre stays on the note, so a picture can't get lost.
            Grab::Move => {
                before.moved((x - sx).clamp(-before.x, aw - before.x), (y - sy).clamp(-before.y, ah - before.y))
            }
            Grab::Corner(i) => {
                // Follow the corner, not the pointer, so it doesn't jump.
                let c = before.corners()[i];
                before.resized(i, x + c.0 - sx, y + c.1 - sy)
            }
            Grab::Rotate => before.rotated((sx, sy), (x, y), self.snap.get()),
        };
        self.drawing.borrow_mut().set_picture(picture);
        self.area.queue_draw();
    }

    fn end_grab(&self) {
        let Some((_, before)) = self.grab.take() else { return };
        self.drawing.borrow_mut().commit_picture(before);
        self.changed();
    }

    fn surface(&self, file: &str) -> Option<cairo::ImageSurface> {
        self.surfaces
            .borrow_mut()
            .entry(file.into())
            .or_insert_with(|| {
                let texture = gdk::Texture::from_filename(self.dir.borrow().join(file)).ok()?;
                texture_surface(&texture)
            })
            .clone()
    }

    /// `file` scaled down to `w`×`h` pixels, kept until it's shown at another size.
    /// Scaling a large picture down on every frame would be slow.
    fn scaled(&self, file: &str, original: &cairo::ImageSurface, w: i32, h: i32) -> Option<cairo::ImageSurface> {
        if let Some(s) = self.scaled.borrow().get(file).filter(|s| (s.width(), s.height()) == (w, h)) {
            return Some(s.clone());
        }
        let s = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h).ok()?;
        let cr = cairo::Context::new(&s).ok()?;
        cr.scale(w as f64 / original.width() as f64, h as f64 / original.height() as f64);
        cr.set_source_surface(original, 0.0, 0.0).ok()?;
        cr.source().set_filter(cairo::Filter::Good);
        cr.paint().ok()?;
        drop(cr);
        self.scaled.borrow_mut().insert(file.into(), s.clone());
        Some(s)
    }

    fn draw_picture(&self, cr: &cairo::Context, p: &Picture, live: bool, ink: &gdk::RGBA) {
        let _ = cr.save();
        cr.translate(p.x, p.y);
        cr.rotate(p.angle.to_radians());
        let (w, h) = (p.width, p.height);
        match self.surface(&p.file) {
            Some(original) => {
                let sf = self.area.scale_factor() as f64;
                let (pw, ph) = (((w * sf).round() as i32).max(1), ((h * sf).round() as i32).max(1));
                // While it's dragged, scale the original; when it rests, a copy made
                // with a better filter, if it's shown smaller than it is.
                let image = if live || pw >= original.width() {
                    original
                } else {
                    self.scaled(&p.file, &original, pw, ph).unwrap_or(original)
                };
                cr.scale(w / image.width() as f64, h / image.height() as f64);
                if cr.set_source_surface(&image, -image.width() as f64 / 2.0, -image.height() as f64 / 2.0).is_ok() {
                    cr.source().set_filter(if live { cairo::Filter::Bilinear } else { cairo::Filter::Good });
                    let _ = cr.paint();
                }
            }
            None => {
                // The file is missing: show where the picture was.
                cr.rectangle(-w / 2.0, -h / 2.0, w, h);
                cr.set_source_rgba(ink.red() as f64, ink.green() as f64, ink.blue() as f64, 0.4);
                cr.set_line_width(1.0);
                cr.set_dash(&[4.0, 4.0], 0.0);
                let _ = cr.stroke();
            }
        }
        let _ = cr.restore();
    }

    fn draw_selection(&self, cr: &cairo::Context, p: &Picture, rotating: bool) {
        let (r, g, b) = SELECTION;
        let (w, h) = (p.width / 2.0, p.height / 2.0);
        let _ = cr.save();
        cr.translate(p.x, p.y);
        cr.rotate(p.angle.to_radians());
        cr.set_line_width(1.5);
        cr.set_source_rgb(r, g, b);
        cr.rectangle(-w, -h, 2.0 * w, 2.0 * h);
        cr.move_to(0.0, -h);
        cr.line_to(0.0, -h - ROTATE_ARM);
        let _ = cr.stroke();
        let handle = HANDLE_SIZE / 2.0;
        for (cx, cy) in [(-w, -h), (w, -h), (w, h), (-w, h)] {
            cr.rectangle(cx - handle, cy - handle, HANDLE_SIZE, HANDLE_SIZE);
        }
        cr.new_sub_path();
        cr.arc(0.0, -h - ROTATE_ARM, handle + 0.5, 0.0, std::f64::consts::TAU);
        cr.set_source_rgb(1.0, 1.0, 1.0);
        let _ = cr.fill_preserve();
        cr.set_source_rgb(r, g, b);
        let _ = cr.stroke();
        if rotating {
            cr.arc(0.0, -h - ROTATE_ARM, handle + 0.5, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
        }
        let _ = cr.restore();
        if rotating {
            // The angle, in a label beside the handle.
            let (hx, hy) = p.rotate_handle();
            let text = format!("{}°", p.angle.round() as i64).replace('-', "\u{2212}");
            cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
            cr.set_font_size(12.0);
            let advance = cr.text_extents(&text).map_or(24.0, |e| e.x_advance());
            let (x, y, bw, bh) = (hx + 14.0, hy - 9.0, advance + 14.0, 18.0);
            cr.new_sub_path();
            cr.arc(
                x + bh / 2.0,
                y + bh / 2.0,
                bh / 2.0,
                std::f64::consts::FRAC_PI_2,
                3.0 * std::f64::consts::FRAC_PI_2,
            );
            cr.arc(
                x + bw - bh / 2.0,
                y + bh / 2.0,
                bh / 2.0,
                -std::f64::consts::FRAC_PI_2,
                std::f64::consts::FRAC_PI_2,
            );
            cr.close_path();
            cr.set_source_rgb(r, g, b);
            let _ = cr.fill();
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.move_to(x + 7.0, y + 13.0);
            let _ = cr.show_text(&text);
        }
    }

    pub fn clear(&self) {
        self.drawing.borrow_mut().clear();
        self.changed();
    }

    /// Pointer/stylus pressed at `(x, y)`.
    pub fn begin(&self, x: f64, y: f64, pressure: f64, eraser_tip: bool) {
        self.start.set((x, y));
        if self.tool.borrow().as_str() == "text" {
            self.begin_grab(x, y);
            return;
        }
        if self.tool.borrow().as_str() == "eraser" || eraser_tip {
            self.erasing.set(true);
            self.drawing.borrow_mut().begin_erase();
            self.erase(x, y);
            return;
        }
        let tool = self.tool.borrow().clone();
        // A shape starts with both of its points where the drag starts.
        let points = if is_shape(&tool) { vec![(x, y, 1.0), (x, y, 1.0)] } else { vec![(x, y, pressure)] };
        let stroke = Stroke::new(&tool, &self.color.borrow(), self.width.get(), points);
        *self.current.borrow_mut() = Some(stroke);
        self.area.queue_draw();
    }

    /// Pointer/stylus moved to `(x, y)` while pressed.
    pub fn update(&self, x: f64, y: f64, pressure: f64) {
        self.hover.set(Some((x, y)));
        if self.grabbing() {
            self.update_grab(x, y);
            return;
        }
        if self.erasing.get() {
            self.erase(x, y);
            return;
        }
        let mut current = self.current.borrow_mut();
        let Some(stroke) = current.as_mut() else { return };
        if stroke.is_shape() {
            stroke.points[1] = (x, y, 1.0);
            self.area.queue_draw();
            return;
        }
        let last = stroke.points[stroke.points.len() - 1];
        if (last.0 - x).abs() + (last.1 - y).abs() < 1.5 {
            return; // ignore jitter
        }
        stroke.points.push((x, y, pressure));
        self.area.queue_draw();
    }

    /// Pointer/stylus released.
    pub fn end(&self) {
        if self.grabbing() {
            self.end_grab();
            return;
        }
        if self.erasing.replace(false) {
            self.drawing.borrow_mut().end_erase();
            self.emit_changed();
            return;
        }
        let Some(mut stroke) = self.current.borrow_mut().take() else { return };
        if stroke.is_shape() {
            let (a, b) = (stroke.points[0], stroke.points[1]);
            let (dx, dy) = ((a.0 - b.0).abs(), (a.1 - b.1).abs());
            // A click, not a drag, or a box or ellipse too flat to see: no shape.
            let flat = matches!(stroke.tool.as_str(), "rect" | "ellipse") && dx.min(dy) < 3.0;
            if dx.hypot(dy) < 3.0 || flat {
                self.area.queue_draw();
                return;
            }
        } else {
            stroke.points = smooth_points(&stroke.points);
        }
        self.drawing.borrow_mut().add(stroke);
        self.changed();
    }

    // -- internals ------------------------------------------------------

    fn erase(&self, x: f64, y: f64) {
        self.drawing.borrow_mut().erase_at(x, y, ERASER_RADIUS);
        self.area.queue_draw();
    }

    fn set_hover(&self, pos: Option<(f64, f64)>) {
        self.hover.set(pos);
        if self.tool.borrow().as_str() == "eraser" {
            self.area.queue_draw();
        }
    }

    fn changed(&self) {
        self.area.queue_draw();
        self.emit_changed();
    }

    fn emit_changed(&self) {
        for f in self.on_changed.borrow().iter() {
            f();
        }
    }

    fn draw(&self, area: &gtk::DrawingArea, cr: &cairo::Context) {
        let ink = area.color();
        let grab = self.grab.borrow().as_ref().map(|(g, p)| (*g, p.id));
        let drawing = self.drawing.borrow();
        for picture in &drawing.pictures {
            self.draw_picture(cr, picture, grab.is_some_and(|(_, id)| id == picture.id), &ink);
        }
        for stroke in &drawing.strokes {
            draw_stroke(cr, stroke, &ink);
        }
        if let Some(stroke) = self.current.borrow().as_ref() {
            draw_stroke(cr, stroke, &ink);
        }
        if let Some(p) = self.selected.get().and_then(|id| drawing.picture(id)) {
            self.draw_selection(cr, p, grab.is_some_and(|(g, _)| g == Grab::Rotate));
        }
        if self.tool.borrow().as_str() == "eraser"
            && area.can_target()
            && let Some((x, y)) = self.hover.get()
        {
            cr.set_source_rgba(ink.red() as f64, ink.green() as f64, ink.blue() as f64, 0.6);
            cr.set_line_width(1.2);
            cr.arc(x, y, ERASER_RADIUS, 0.0, std::f64::consts::TAU);
            let _ = cr.stroke();
        }
    }
}

/// A texture's pixels as a cairo surface.
fn texture_surface(texture: &gdk::Texture) -> Option<cairo::ImageSurface> {
    let mut downloader = gdk::TextureDownloader::new(texture);
    // Cairo's ARGB32 is a native-endian 32-bit value.
    downloader.set_format(if cfg!(target_endian = "little") {
        gdk::MemoryFormat::B8g8r8a8Premultiplied
    } else {
        gdk::MemoryFormat::A8r8g8b8Premultiplied
    });
    let (bytes, stride) = downloader.download_bytes();
    let (w, h) = (texture.width(), texture.height());
    cairo::ImageSurface::create_for_data(bytes.to_vec(), cairo::Format::ARgb32, w, h, stride as i32).ok()
}

/// Pressure and whether the stylus' eraser end is used.
fn event_info(gesture: &gtk::GestureDrag) -> (f64, bool) {
    let Some(event) = gesture.last_event(gesture.last_updated_sequence().as_ref()) else { return (1.0, false) };
    let pressure = event.axis(gdk::AxisUse::Pressure).filter(|p| *p > 0.0).unwrap_or(1.0);
    let eraser = event.device_tool().is_some_and(|t| t.tool_type() == gdk::DeviceToolType::Eraser);
    (pressure, eraser)
}
