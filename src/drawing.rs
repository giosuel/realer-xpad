//! Toolkit-independent drawing model: strokes, erasing and undo/redo.

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

/// One removal: `(index, stroke)` pairs with indices relative to the stroke
/// list just before the batch was removed.
type Batch = Vec<(usize, Stroke)>;

#[derive(Clone, Debug)]
enum Entry {
    Add(Stroke),
    Remove(Vec<Batch>),
}

/// A list of strokes with undo/redo.
#[derive(Clone, Debug, Default)]
pub struct Drawing {
    pub strokes: Vec<Stroke>,
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    erasing: Option<Vec<Batch>>,
}

impl Drawing {
    pub fn new(strokes: Vec<Stroke>) -> Self {
        Drawing { strokes, ..Default::default() }
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
        let mut d = Drawing::new(vec![shape("ellipse", 0.0, 0.0, 40.0, 40.0)]);
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
}
