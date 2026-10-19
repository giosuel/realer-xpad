//! Toolkit-independent drawing model: strokes, pictures, erasing and undo/redo.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

/// Freehand tools. Shapes are tools too (see [`SHAPES`]); the eraser removes strokes.
pub const TOOLS: [&str; 3] = ["pen", "highlighter", "eraser"];
/// Shapes are strokes with two points, where they start and end.
pub const SHAPES: [&str; 4] = ["line", "arrow", "rect", "ellipse"];
/// `"ink"` follows the note's text colour, so it stays readable in light and dark mode.
pub const INK_COLORS: [&str; 6] = ["ink", "#e5484d", "#3e63dd", "#30a46c", "#f76b15", "#8e4ec6"];
pub const WIDTHS: [f64; 3] = [2.0, 4.0, 8.0];
pub const HIGHLIGHTER_SCALE: f64 = 4.0;
pub const HIGHLIGHTER_ALPHA: f64 = 0.35;
/// How far above a selected picture its rotation handle sits.
pub const ROTATE_ARM: f64 = 27.0;
/// How close to a handle a press must be to grab it.
pub const HANDLE_REACH: f64 = 9.0;
/// Pictures can't be resized smaller than this, on their shorter side.
pub const MIN_PICTURE: f64 = 16.0;
/// Shift snaps rotation to steps of this many degrees.
pub const ROTATE_SNAP: f64 = 15.0;

/// A point: x, y and pen pressure (1.0 for mice).
pub type Point = (f64, f64, f64);

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(from = "RawStroke", into = "RawStroke")]
pub struct Stroke {
    /// Process-local identity, used by undo/redo.
    pub id: u64,
    pub tool: String,
    pub color: String,
    pub width: f64,
    pub points: Vec<Point>,
}

impl PartialEq for Stroke {
    fn eq(&self, other: &Self) -> bool {
        (&self.tool, &self.color, self.width, &self.points) == (&other.tool, &other.color, other.width, &other.points)
    }
}

/// On-disk format, compatible with the original Python version:
/// `{"tool", "color", "width", "points": [[x, y, pressure], ...]}`.
#[derive(Serialize, Deserialize)]
struct RawStroke {
    #[serde(default = "default_tool")]
    tool: String,
    #[serde(default = "default_color")]
    color: String,
    #[serde(default = "default_width")]
    width: f64,
    #[serde(default)]
    points: Vec<Vec<f64>>,
}

fn default_tool() -> String {
    "pen".into()
}
fn default_color() -> String {
    INK_COLORS[0].into()
}
fn default_width() -> f64 {
    WIDTHS[1]
}

fn round(v: f64, digits: i32) -> f64 {
    let f = 10f64.powi(digits);
    (v * f).round() / f
}

impl From<RawStroke> for Stroke {
    fn from(raw: RawStroke) -> Self {
        let points = raw
            .points
            .iter()
            .filter(|p| p.len() >= 2)
            .map(|p| (p[0], p[1], p.get(2).copied().unwrap_or(1.0)))
            .collect();
        Stroke::new(&raw.tool, &raw.color, raw.width, points)
    }
}

impl From<Stroke> for RawStroke {
    fn from(s: Stroke) -> Self {
        RawStroke {
            tool: s.tool,
            color: s.color,
            width: s.width,
            points: s.points.iter().map(|&(x, y, p)| vec![round(x, 1), round(y, 1), round(p, 2)]).collect(),
        }
    }
}

impl Stroke {
    pub fn new(tool: &str, color: &str, width: f64, points: Vec<Point>) -> Self {
        Stroke { id: NEXT_ID.fetch_add(1, Ordering::Relaxed), tool: tool.into(), color: color.into(), width, points }
    }

    pub fn is_shape(&self) -> bool {
        is_shape(&self.tool)
    }

    pub fn effective_width(&self) -> f64 {
        self.width * if self.tool == "highlighter" { HIGHLIGHTER_SCALE } else { 1.0 }
    }

    /// The lines a stroke is drawn with, as polylines: the points of a freehand
    /// stroke, or a shape's outline (an ellipse as a fine polygon).
    pub fn outline(&self) -> Vec<Vec<(f64, f64)>> {
        let pts = &self.points;
        let (Some(a), Some(b)) = (pts.first(), pts.last()) else { return Vec::new() };
        let ((x0, y0), (x1, y1)) = ((a.0, a.1), (b.0, b.1));
        match self.tool.as_str() {
            "line" => vec![vec![(x0, y0), (x1, y1)]],
            "arrow" => {
                let (l, r) = arrow_head((x0, y0), (x1, y1), self.width);
                vec![vec![(x0, y0), (x1, y1)], vec![l, (x1, y1), r]]
            }
            "rect" => vec![vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)]],
            "ellipse" => {
                let (cx, cy, rx, ry) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0, (x1 - x0).abs() / 2.0, (y1 - y0).abs() / 2.0);
                let n = 48;
                vec![
                    (0..=n)
                        .map(|i| {
                            let t = i as f64 / n as f64 * std::f64::consts::TAU;
                            (cx + rx * t.cos(), cy + ry * t.sin())
                        })
                        .collect(),
                ]
            }
            _ => vec![pts.iter().map(|p| (p.0, p.1)).collect()],
        }
    }

    pub fn hit(&self, x: f64, y: f64, radius: f64) -> bool {
        let reach = radius + self.effective_width() / 2.0;
        self.outline().iter().any(|line| match line.as_slice() {
            [] => false,
            [p] => (p.0 - x).hypot(p.1 - y) <= reach,
            _ => {
                // Cheap bounding-box rejection before the per-segment distance check.
                let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
                for p in line {
                    x0 = x0.min(p.0);
                    y0 = y0.min(p.1);
                    x1 = x1.max(p.0);
                    y1 = y1.max(p.1);
                }
                if x < x0 - reach || x > x1 + reach || y < y0 - reach || y > y1 + reach {
                    return false;
                }
                line.windows(2).any(|w| seg_dist(x, y, w[0], w[1]) <= reach)
            }
        })
    }
}

pub fn is_shape(tool: &str) -> bool {
    SHAPES.contains(&tool)
}

/// The two ends of an arrow head at `tip`, for a shaft coming from `from`.
pub fn arrow_head(from: (f64, f64), tip: (f64, f64), width: f64) -> ((f64, f64), (f64, f64)) {
    let len = (width * 3.5).max(10.0);
    let angle = (tip.1 - from.1).atan2(tip.0 - from.0);
    let spread = 28f64.to_radians();
    let end = |a: f64| (tip.0 - len * a.cos(), tip.1 - len * a.sin());
    (end(angle - spread), end(angle + spread))
}

fn seg_dist(px: f64, py: f64, a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 { 0.0 } else { (((px - a.0) * dx + (py - a.1) * dy) / len2).clamp(0.0, 1.0) };
    (px - (a.0 + t * dx)).hypot(py - (a.1 + t * dy))
}

fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// A pasted image: a file in the notes folder, placed by its centre, size and
/// rotation (degrees, clockwise). Pictures lie under the strokes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Picture {
    /// Process-local identity, used by undo/redo and the selection.
    #[serde(skip, default = "next_id")]
    pub id: u64,
    /// Path relative to the notes folder, e.g. `images/<id>.png`.
    pub file: String,
    #[serde(serialize_with = "round1")]
    pub x: f64,
    #[serde(serialize_with = "round1")]
    pub y: f64,
    #[serde(serialize_with = "round1")]
    pub width: f64,
    #[serde(serialize_with = "round1")]
    pub height: f64,
    #[serde(default, serialize_with = "round1")]
    pub angle: f64,
}

fn round1<S: serde::Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_f64(round(*v, 1))
}

impl PartialEq for Picture {
    fn eq(&self, other: &Self) -> bool {
        (&self.file, self.x, self.y, self.width, self.height, self.angle)
            == (&other.file, other.x, other.y, other.width, other.height, other.angle)
    }
}

/// What a press on a picture takes hold of.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Grab {
    Move,
    /// A corner, clockwise from the top left.
    Corner(usize),
    Rotate,
}

/// The largest size up to `natural` that fits in `max`, keeping proportions.
pub fn fit(natural: (f64, f64), max: (f64, f64)) -> (f64, f64) {
    let (w, h) = (natural.0.max(1.0), natural.1.max(1.0));
    let s = 1f64.min(max.0 / w).min(max.1 / h).max(MIN_PICTURE / w.min(h));
    (w * s, h * s)
}

impl Picture {
    pub fn new(file: &str, (x, y): (f64, f64), (width, height): (f64, f64)) -> Self {
        Picture { id: next_id(), file: file.into(), x, y, width, height, angle: 0.0 }
    }

    fn sin_cos(&self) -> (f64, f64) {
        self.angle.to_radians().sin_cos()
    }

    /// A point in the picture's own frame (centre at 0, 0, unrotated) on the note.
    pub fn to_note(&self, (lx, ly): (f64, f64)) -> (f64, f64) {
        let (sin, cos) = self.sin_cos();
        (self.x + lx * cos - ly * sin, self.y + lx * sin + ly * cos)
    }

    /// A point on the note in the picture's own frame.
    pub fn to_local(&self, (px, py): (f64, f64)) -> (f64, f64) {
        let (sin, cos) = self.sin_cos();
        let (dx, dy) = (px - self.x, py - self.y);
        (dx * cos + dy * sin, -dx * sin + dy * cos)
    }

    /// Corner `i`, clockwise from the top left, in the picture's frame.
    fn corner(&self, i: usize) -> (f64, f64) {
        let (w, h) = (self.width / 2.0, self.height / 2.0);
        [(-w, -h), (w, -h), (w, h), (-w, h)][i % 4]
    }

    pub fn corners(&self) -> [(f64, f64); 4] {
        [0, 1, 2, 3].map(|i| self.to_note(self.corner(i)))
    }

    pub fn rotate_handle(&self) -> (f64, f64) {
        self.to_note((0.0, -self.height / 2.0 - ROTATE_ARM))
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        let (lx, ly) = self.to_local((x, y));
        lx.abs() <= self.width / 2.0 && ly.abs() <= self.height / 2.0
    }

    /// What a press at `(x, y)` grabs; only a selected picture shows handles.
    pub fn grab_at(&self, x: f64, y: f64, selected: bool) -> Option<Grab> {
        let near = |(hx, hy): (f64, f64)| (hx - x).hypot(hy - y) <= HANDLE_REACH;
        if selected {
            if near(self.rotate_handle()) {
                return Some(Grab::Rotate);
            }
            if let Some(i) = self.corners().into_iter().position(near) {
                return Some(Grab::Corner(i));
            }
        }
        self.contains(x, y).then_some(Grab::Move)
    }

    pub fn moved(&self, dx: f64, dy: f64) -> Picture {
        Picture { x: self.x + dx, y: self.y + dy, ..self.clone() }
    }

    /// Dragging corner `i` to `(px, py)`: the opposite corner stays put and the
    /// picture keeps its proportions.
    pub fn resized(&self, i: usize, px: f64, py: f64) -> Picture {
        let anchor = self.corner(i + 2);
        let c = self.corner(i);
        let d = (c.0 - anchor.0, c.1 - anchor.1);
        let p = self.to_local((px, py));
        let p = (p.0 - anchor.0, p.1 - anchor.1);
        let s = ((p.0 * d.0 + p.1 * d.1) / (d.0 * d.0 + d.1 * d.1)).max(MIN_PICTURE / self.width.min(self.height));
        let (x, y) = self.to_note((anchor.0 + d.0 * s / 2.0, anchor.1 + d.1 * s / 2.0));
        Picture { x, y, width: self.width * s, height: self.height * s, ..self.clone() }
    }

    /// Turning by the angle the pointer swept around the centre since `from`;
    /// `snap` rounds to [`ROTATE_SNAP`] degrees.
    pub fn rotated(&self, from: (f64, f64), to: (f64, f64), snap: bool) -> Picture {
        let swept = (to.1 - self.y).atan2(to.0 - self.x) - (from.1 - self.y).atan2(from.0 - self.x);
        let mut angle = self.angle + swept.to_degrees();
        if snap {
            angle = (angle / ROTATE_SNAP).round() * ROTATE_SNAP;
        }
        // Keep it in (-180, 180], so it reads well and saves the same.
        angle = (angle + 180.0).rem_euclid(360.0) - 180.0;
        if angle == -180.0 {
            angle = 180.0;
        }
        Picture { angle, ..self.clone() }
    }
}

/// One removal: `(index, stroke)` pairs with indices relative to the stroke
/// list just before the batch was removed.
type Batch = Vec<(usize, Stroke)>;

#[derive(Clone, Debug)]
enum Entry {
    Add(Stroke),
    Remove(Vec<Batch>),
    /// A picture added (`before` is `None`), removed (`after` is `None`) or
    /// changed, at `index` in the pictures.
    Picture {
        index: usize,
        before: Option<Picture>,
        after: Option<Picture>,
    },
}

/// A list of strokes, and the pictures under them, with undo/redo.
#[derive(Clone, Debug, Default)]
pub struct Drawing {
    pub strokes: Vec<Stroke>,
    pub pictures: Vec<Picture>,
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    erasing: Option<Vec<Batch>>,
}

impl Drawing {
    pub fn new(strokes: Vec<Stroke>, pictures: Vec<Picture>) -> Self {
        Drawing { strokes, pictures, ..Default::default() }
    }

    pub fn picture(&self, id: u64) -> Option<&Picture> {
        self.pictures.iter().find(|p| p.id == id)
    }

    /// The topmost picture at `(x, y)`.
    pub fn picture_at(&self, x: f64, y: f64) -> Option<&Picture> {
        self.pictures.iter().rev().find(|p| p.contains(x, y))
    }

    pub fn add_picture(&mut self, picture: Picture) {
        self.pictures.push(picture.clone());
        self.push(Entry::Picture { index: self.pictures.len() - 1, before: None, after: Some(picture) });
    }

    pub fn remove_picture(&mut self, id: u64) -> bool {
        let Some(index) = self.pictures.iter().position(|p| p.id == id) else { return false };
        let before = self.pictures.remove(index);
        self.push(Entry::Picture { index, before: Some(before), after: None });
        true
    }

    /// Show a picture moved, resized or turned, without an undo step yet (while
    /// it's being dragged): see [`Drawing::commit_picture`].
    pub fn set_picture(&mut self, picture: Picture) {
        if let Some(p) = self.pictures.iter_mut().find(|p| p.id == picture.id) {
            *p = picture;
        }
    }

    /// One undo step from `before` to how the picture is now.
    pub fn commit_picture(&mut self, before: Picture) {
        let Some(index) = self.pictures.iter().position(|p| p.id == before.id) else { return };
        if self.pictures[index] != before {
            let after = Some(self.pictures[index].clone());
            self.push(Entry::Picture { index, before: Some(before), after });
        }
    }

    fn put_picture(&mut self, index: usize, id: u64, picture: Option<&Picture>) {
        match (self.pictures.iter().position(|p| p.id == id), picture) {
            (Some(i), Some(p)) => self.pictures[i] = p.clone(),
            (Some(i), None) => drop(self.pictures.remove(i)),
            (None, Some(p)) => self.pictures.insert(index.min(self.pictures.len()), p.clone()),
            (None, None) => {}
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn add(&mut self, stroke: Stroke) {
        if stroke.points.is_empty() {
            return;
        }
        self.strokes.push(stroke.clone());
        self.push(Entry::Add(stroke));
    }

    /// Group erasing so one drag of the eraser is a single undo step.
    pub fn begin_erase(&mut self) {
        self.erasing = Some(Vec::new());
    }

    pub fn erase_at(&mut self, x: f64, y: f64, radius: f64) -> bool {
        let hits: Batch =
            self.strokes.iter().enumerate().filter(|(_, s)| s.hit(x, y, radius)).map(|(i, s)| (i, s.clone())).collect();
        if hits.is_empty() {
            return false;
        }
        for &(i, _) in hits.iter().rev() {
            self.strokes.remove(i);
        }
        match &mut self.erasing {
            Some(batches) => batches.push(hits),
            None => self.push(Entry::Remove(vec![hits])),
        }
        true
    }

    pub fn end_erase(&mut self) {
        if let Some(batches) = self.erasing.take()
            && !batches.is_empty()
        {
            self.push(Entry::Remove(batches));
        }
    }

    pub fn clear(&mut self) {
        if !self.strokes.is_empty() {
            let batch = std::mem::take(&mut self.strokes).into_iter().enumerate().collect();
            self.push(Entry::Remove(vec![batch]));
        }
    }

    /// Does nothing in the middle of an eraser drag, whose strokes aren't in
    /// the history yet.
    pub fn undo(&mut self) -> bool {
        if self.erasing.is_some() {
            return false;
        }
        let Some(entry) = self.undo.pop() else { return false };
        self.apply(&entry, true);
        self.redo.push(entry);
        true
    }

    pub fn redo(&mut self) -> bool {
        if self.erasing.is_some() {
            return false;
        }
        let Some(entry) = self.redo.pop() else { return false };
        self.apply(&entry, false);
        self.undo.push(entry);
        true
    }

    fn push(&mut self, entry: Entry) {
        self.undo.push(entry);
        self.redo.clear();
    }

    fn remove_id(&mut self, id: u64) {
        self.strokes.retain(|s| s.id != id);
    }

    fn apply(&mut self, entry: &Entry, reverse: bool) {
        match (entry, reverse) {
            (Entry::Add(s), true) => self.remove_id(s.id),
            (Entry::Add(s), false) => self.strokes.push(s.clone()),
            (Entry::Remove(batches), true) => {
                for batch in batches.iter().rev() {
                    for (i, s) in batch {
                        // ascending indices restore the original positions
                        self.strokes.insert((*i).min(self.strokes.len()), s.clone());
                    }
                }
            }
            (Entry::Remove(batches), false) => {
                for (_, s) in batches.iter().flatten() {
                    self.remove_id(s.id);
                }
            }
            (Entry::Picture { index, before, after }, reverse) => {
                let (from, to) = if reverse { (after, before) } else { (before, after) };
                let Some(id) = from.as_ref().or(to.as_ref()).map(|p| p.id) else { return };
                self.put_picture(*index, id, to.as_ref());
            }
        }
    }
}

/// Light moving-average smoothing that keeps the end points fixed.
pub fn smooth_points(points: &[Point]) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut out = vec![points[0]];
    for w in points.windows(3) {
        let (a, b, c) = (w[0], w[1], w[2]);
        out.push(((a.0 + 2.0 * b.0 + c.0) / 4.0, (a.1 + 2.0 * b.1 + c.1) / 4.0, b.2));
    }
    out.push(*points.last().unwrap());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Stroke {
        Stroke::new("pen", "#000000", 2.0, vec![(x0, y0, 1.0), (x1, y1, 1.0)])
    }

    fn ids(d: &Drawing) -> Vec<u64> {
        d.strokes.iter().map(|s| s.id).collect()
    }

    #[test]
    fn undo_waits_for_the_eraser_drag_to_finish() {
        let mut d = Drawing::default();
        d.add(Stroke::new("pen", "ink", 2.0, vec![(0.0, 0.0, 1.0), (10.0, 0.0, 1.0)]));
        d.begin_erase();
        assert!(d.erase_at(5.0, 0.0, 3.0));
        assert!(!d.undo(), "not mid-drag");
        d.end_erase();
        assert!(d.undo(), "the erase");
        assert_eq!(d.strokes.len(), 1);
        assert!(d.undo(), "the stroke");
        assert!(d.strokes.is_empty());
        assert!(!d.undo());
    }

    #[test]
    fn add_undo_redo() {
        let mut d = Drawing::default();
        let (a, b) = (line(0., 0., 10., 0.), line(0., 10., 10., 10.));
        d.add(a.clone());
        d.add(b.clone());
        assert!(d.undo());
        assert_eq!(ids(&d), [a.id]);
        assert!(d.redo());
        assert_eq!(ids(&d), [a.id, b.id]);
        assert!(!d.redo());
    }

    #[test]
    fn new_stroke_clears_redo() {
        let mut d = Drawing::default();
        d.add(line(0., 0., 1., 1.));
        d.undo();
        d.add(line(2., 2., 3., 3.));
        assert!(!d.can_redo());
    }

    #[test]
    fn erase_hits_only_nearby_strokes() {
        let mut d = Drawing::default();
        let (a, b) = (line(0., 0., 100., 0.), line(0., 50., 100., 50.));
        d.add(a);
        d.add(b.clone());
        assert!(d.erase_at(50., 3., 6.));
        assert_eq!(ids(&d), [b.id]);
        assert!(!d.erase_at(50., 25., 6.));
    }

    #[test]
    fn grouped_erase_undo_restores_order() {
        let mut d = Drawing::default();
        let strokes: Vec<_> = [0., 20., 40., 60.].iter().map(|&y| line(0., y, 100., y)).collect();
        for s in &strokes {
            d.add(s.clone());
        }
        let all: Vec<_> = strokes.iter().map(|s| s.id).collect();
        d.begin_erase();
        d.erase_at(50., 20., 6.);
        d.erase_at(50., 60., 6.);
        d.erase_at(50., 0., 6.);
        d.end_erase();
        assert_eq!(ids(&d), [strokes[2].id]);
        d.undo();
        assert_eq!(ids(&d), all);
        d.redo();
        assert_eq!(ids(&d), [strokes[2].id]);
    }

    #[test]
    fn erase_multiple_in_one_spot_undo() {
        let mut d = Drawing::default();
        let strokes =
            [line(0., 0., 100., 0.), line(50., -50., 50., 50.), line(0., 90., 10., 90.), line(0., 1., 100., 1.)];
        for s in &strokes {
            d.add(s.clone());
        }
        d.erase_at(50., 0., 6.);
        assert_eq!(ids(&d), [strokes[2].id]);
        d.undo();
        assert_eq!(ids(&d), strokes.iter().map(|s| s.id).collect::<Vec<_>>());
    }

    #[test]
    fn clear_is_undoable() {
        let mut d = Drawing::default();
        d.add(line(0., 0., 1., 1.));
        d.clear();
        assert!(d.strokes.is_empty());
        d.undo();
        assert_eq!(d.strokes.len(), 1);
    }

    #[test]
    fn highlighter_hit_uses_wider_width() {
        let mut s = line(0., 0., 100., 0.);
        assert!(!s.hit(50., 6., 2.));
        s.tool = "highlighter".into(); // 2 * 4 = 8px wide
        assert!(s.hit(50., 6., 2.));
    }

    #[test]
    fn json_roundtrip_matches_python_format() {
        let s = Stroke::new("highlighter", "#ff0000", 4.0, vec![(1.234, 2.0, 0.5), (3.0, 4.0, 0.75)]);
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(
            json,
            r##"{"tool":"highlighter","color":"#ff0000","width":4.0,"points":[[1.2,2.0,0.5],[3.0,4.0,0.75]]}"##
        );
        let back: Stroke = serde_json::from_str(&json).unwrap();
        assert_eq!(back.points[0], (1.2, 2.0, 0.5));
    }

    #[test]
    fn points_without_pressure_and_missing_fields() {
        let s: Stroke = serde_json::from_str(r#"{"points": [[1, 2], [3, 4]]}"#).unwrap();
        assert_eq!(s.points, [(1.0, 2.0, 1.0), (3.0, 4.0, 1.0)]);
        assert_eq!((s.tool.as_str(), s.color.as_str(), s.width), ("pen", "ink", 4.0));
    }

    fn shape(tool: &str, x0: f64, y0: f64, x1: f64, y1: f64) -> Stroke {
        Stroke::new(tool, "ink", 2.0, vec![(x0, y0, 1.0), (x1, y1, 1.0)])
    }

    #[test]
    fn shapes_are_hit_on_their_outline() {
        let rect = shape("rect", 0.0, 0.0, 100.0, 50.0);
        assert!(rect.hit(100.0, 25.0, 4.0), "right edge");
        assert!(rect.hit(50.0, 50.0, 4.0), "bottom edge");
        assert!(!rect.hit(50.0, 25.0, 4.0), "not the inside");
        assert!(!rect.hit(50.0, 25.5, 4.0), "nor the diagonal between its two points");

        let ellipse = shape("ellipse", 0.0, 0.0, 100.0, 50.0);
        assert!(ellipse.hit(50.0, 0.0, 4.0) && ellipse.hit(100.0, 25.0, 4.0));
        assert!(!ellipse.hit(50.0, 25.0, 4.0), "not the centre");
        assert!(!ellipse.hit(2.0, 2.0, 2.0), "nor the bounding box corner");

        let arrow = shape("arrow", 0.0, 0.0, 100.0, 0.0);
        assert!(arrow.hit(50.0, 0.0, 2.0));
        let (l, r) = arrow_head((0.0, 0.0), (100.0, 0.0), 2.0);
        assert!(l.0 < 100.0 && r.0 < 100.0, "the head points back from the tip");
        assert!((l.1 < 0.0) != (r.1 < 0.0), "one end on each side of the shaft");
        assert!(arrow.hit(l.0, l.1, 1.0), "and is part of the arrow");

        // Drawn from bottom right to top left, it's the same rectangle.
        assert!(shape("rect", 100.0, 50.0, 0.0, 0.0).hit(100.0, 25.0, 4.0));
    }

    #[test]
    fn shapes_erase_and_save_like_strokes() {
        let mut d = Drawing::new(vec![shape("ellipse", 0.0, 0.0, 40.0, 40.0)], vec![]);
        assert!(!d.erase_at(20.0, 20.0, 5.0), "erasing inside an ellipse leaves it");
        assert!(d.erase_at(40.0, 20.0, 5.0));
        assert!(d.strokes.is_empty());
        let json = serde_json::to_string(&shape("arrow", 1.0, 2.0, 3.0, 4.0)).unwrap();
        assert_eq!(json, r#"{"tool":"arrow","color":"ink","width":2.0,"points":[[1.0,2.0,1.0],[3.0,4.0,1.0]]}"#);
        assert!(serde_json::from_str::<Stroke>(&json).unwrap().is_shape());
    }

    #[test]
    fn smooth_keeps_endpoints() {
        let pts = [(0., 0., 1.), (10., 10., 1.), (20., 0., 1.)];
        let out = smooth_points(&pts);
        assert_eq!((out[0], out[2]), (pts[0], pts[2]));
        assert_eq!(out[1].1, 5.0);
    }

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn pictures_are_hit_where_they_are_drawn() {
        let mut p = Picture::new("images/a.png", (100.0, 100.0), (80.0, 40.0));
        assert!(p.contains(130.0, 110.0) && !p.contains(100.0, 130.0));
        assert!(close(p.corners()[2], (140.0, 120.0)));
        p.angle = 90.0; // now tall
        assert!(p.contains(100.0, 130.0) && !p.contains(130.0, 100.0));
        assert!(close(p.corners()[0], (120.0, 60.0)));
        assert!(close(p.to_local(p.to_note((3.0, -7.0))), (3.0, -7.0)));
    }

    #[test]
    fn only_a_selected_picture_has_handles() {
        let p = Picture::new("images/a.png", (100.0, 100.0), (80.0, 40.0));
        let (hx, hy) = p.rotate_handle();
        assert!(close((hx, hy), (100.0, 80.0 - ROTATE_ARM)));
        assert_eq!(p.grab_at(hx, hy + 2.0, true), Some(Grab::Rotate));
        assert_eq!(p.grab_at(hx, hy + 2.0, false), None);
        assert_eq!(p.grab_at(143.0, 122.0, true), Some(Grab::Corner(2)), "a little outside counts");
        assert_eq!(p.grab_at(138.0, 118.0, false), Some(Grab::Move));
        assert_eq!(p.grab_at(100.0, 100.0, true), Some(Grab::Move));
    }

    #[test]
    fn corners_resize_keeping_proportions_and_the_opposite_corner() {
        let p = Picture::new("images/a.png", (100.0, 100.0), (80.0, 40.0));
        let r = p.resized(2, 180.0, 140.0);
        assert!(close((r.width, r.height), (120.0, 60.0)), "{r:?}");
        assert!(close(r.corners()[0], p.corners()[0]));
        let r = p.resized(2, 0.0, 0.0);
        assert!(close((r.width, r.height), (2.0 * MIN_PICTURE, MIN_PICTURE)), "not smaller than the minimum");

        let p = Picture { angle: 30.0, ..p };
        let (x, y) = p.corners()[1];
        let r = p.resized(1, x + 15.0, y - 10.0);
        assert!(r.width > p.width && (r.width / r.height - 2.0).abs() < 1e-9);
        assert!(close(r.corners()[3], p.corners()[3]), "turned pictures too");
        assert_eq!(r.angle, 30.0);
    }

    #[test]
    fn rotating_follows_the_pointer_and_snaps() {
        let p = Picture::new("images/a.png", (100.0, 100.0), (80.0, 40.0));
        let r = p.rotated((100.0, 50.0), (150.0, 100.0), false);
        assert!((r.angle - 90.0).abs() < 1e-9);
        let r = p.rotated((100.0, 50.0), (112.0, 50.0), false);
        assert!(r.angle > 10.0 && r.angle < 15.0);
        assert_eq!(p.rotated((100.0, 50.0), (112.0, 50.0), true).angle, 15.0);
        let r = Picture { angle: 170.0, ..p.clone() }.rotated((100.0, 50.0), (150.0, 100.0), false);
        assert!((r.angle + 100.0).abs() < 1e-9, "kept within ±180: {}", r.angle);
        assert_eq!(p.moved(5.0, -5.0).to_note((0.0, 0.0)), (105.0, 95.0));
    }

    #[test]
    fn pictures_fit_without_growing() {
        assert_eq!(fit((1000.0, 500.0), (200.0, 200.0)), (200.0, 100.0));
        assert_eq!(fit((50.0, 40.0), (200.0, 200.0)), (50.0, 40.0));
    }

    #[test]
    fn picture_changes_undo_in_order() {
        let mut d = Drawing::default();
        let a = Picture::new("images/a.png", (50.0, 50.0), (20.0, 20.0));
        let b = Picture::new("images/b.png", (80.0, 50.0), (20.0, 20.0));
        d.add_picture(a.clone());
        d.add_picture(b.clone());
        d.set_picture(a.moved(10.0, 0.0)); // dragging
        d.set_picture(a.moved(30.0, 0.0));
        d.commit_picture(a.clone());
        d.commit_picture(b.clone()); // unchanged: no step
        assert!(d.remove_picture(a.id));
        assert!(!d.remove_picture(a.id));
        assert!(d.pictures.len() == 1 && d.pictures[0] == b);
        assert!(d.undo());
        assert_eq!(d.pictures, [a.moved(30.0, 0.0), b.clone()], "back in its place, under b");
        assert!(d.undo());
        assert_eq!(d.pictures, [a.clone(), b.clone()], "the whole drag is one step");
        assert!(d.undo() && d.undo());
        assert!(d.pictures.is_empty() && !d.can_undo());
        assert!(d.redo() && d.redo() && d.redo());
        assert_eq!(d.pictures[0].x, 80.0);
        assert_eq!(d.picture_at(85.0, 50.0).map(|p| &p.file), Some(&b.file), "the top one");
    }

    #[test]
    fn pictures_save_rounded_and_load_with_new_ids() {
        let mut p = Picture::new("images/a.png", (10.04, 20.0), (30.0, 40.0));
        p.angle = -12.345;
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(json, r#"{"file":"images/a.png","x":10.0,"y":20.0,"width":30.0,"height":40.0,"angle":-12.3}"#);
        let back: Picture =
            serde_json::from_str(r#"{"file":"images/a.png","x":1,"y":2,"width":3,"height":4}"#).unwrap();
        assert_eq!((back.angle, back.width), (0.0, 3.0));
        assert_ne!(back.id, p.id);
    }
}
