//! Drawing layer widget.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use gtk::prelude::*;
use gtk::{cairo, gdk};

use crate::drawing::{Drawing, HIGHLIGHTER_ALPHA, INK_COLORS, Point, Stroke, WIDTHS, smooth_points};

pub const ERASER_RADIUS: f64 = 8.0;

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

pub fn draw_stroke(cr: &cairo::Context, stroke: &Stroke, ink: &gdk::RGBA) {
    let pts = &stroke.points;
    if pts.is_empty() {
        return;
    }
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_line_join(cairo::LineJoin::Round);
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

    pub fn set_tool(&self, tool: &str) {
        *self.tool.borrow_mut() = tool.into();
        self.area.set_cursor_from_name(Some(if tool == "eraser" { "none" } else { "crosshair" }));
        self.area.queue_draw();
    }

    pub fn undo(&self) {
        if self.drawing.borrow_mut().undo() {
            self.changed();
        }
    }

    pub fn redo(&self) {
        if self.drawing.borrow_mut().redo() {
            self.changed();
        }
    }

    pub fn clear(&self) {
        self.drawing.borrow_mut().clear();
        self.changed();
    }

    /// Pointer/stylus pressed at `(x, y)`.
    pub fn begin(&self, x: f64, y: f64, pressure: f64, eraser_tip: bool) {
        self.start.set((x, y));
        if self.tool.borrow().as_str() == "eraser" || eraser_tip {
            self.erasing.set(true);
            self.drawing.borrow_mut().begin_erase();
            self.erase(x, y);
            return;
        }
        let stroke = Stroke::new(&self.tool.borrow(), &self.color.borrow(), self.width.get(), vec![(x, y, pressure)]);
        *self.current.borrow_mut() = Some(stroke);
        self.area.queue_draw();
    }

    /// Pointer/stylus moved to `(x, y)` while pressed.
    pub fn update(&self, x: f64, y: f64, pressure: f64) {
        self.hover.set(Some((x, y)));
        if self.erasing.get() {
            self.erase(x, y);
            return;
        }
        let mut current = self.current.borrow_mut();
        let Some(stroke) = current.as_mut() else { return };
        let last = stroke.points[stroke.points.len() - 1];
        if (last.0 - x).abs() + (last.1 - y).abs() < 1.5 {
            return; // ignore jitter
        }
        stroke.points.push((x, y, pressure));
        self.area.queue_draw();
    }

    /// Pointer/stylus released.
    pub fn end(&self) {
        if self.erasing.replace(false) {
            self.drawing.borrow_mut().end_erase();
            self.emit_changed();
            return;
        }
        let Some(mut stroke) = self.current.borrow_mut().take() else { return };
        stroke.points = smooth_points(&stroke.points);
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
        for stroke in &self.drawing.borrow().strokes {
            draw_stroke(cr, stroke, &ink);
        }
        if let Some(stroke) = self.current.borrow().as_ref() {
            draw_stroke(cr, stroke, &ink);
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

/// Pressure and whether the stylus' eraser end is used.
fn event_info(gesture: &gtk::GestureDrag) -> (f64, bool) {
    let Some(event) = gesture.last_event(gesture.last_updated_sequence().as_ref()) else { return (1.0, false) };
    let pressure = event.axis(gdk::AxisUse::Pressure).filter(|p| *p > 0.0).unwrap_or(1.0);
    let eraser = event.device_tool().is_some_and(|t| t.tool_type() == gdk::DeviceToolType::Eraser);
    (pressure, eraser)
}
