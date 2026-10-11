"""Toolkit-independent drawing model: strokes, erasing and undo/redo."""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from itertools import pairwise

TOOLS = ("pen", "highlighter", "eraser")
# "ink" follows the note's text colour, so it stays readable in light and dark mode.
INK_COLORS = ("ink", "#e5484d", "#3e63dd", "#30a46c", "#f76b15", "#8e4ec6")
WIDTHS = (2.0, 4.0, 8.0)
HIGHLIGHTER_SCALE = 4.0
HIGHLIGHTER_ALPHA = 0.35


@dataclass
class Stroke:
    tool: str
    color: str
    width: float
    points: list[tuple[float, float, float]] = field(default_factory=list)  # x, y, pressure

    def to_dict(self) -> dict:
        return {"tool": self.tool, "color": self.color, "width": self.width,
                "points": [[round(x, 1), round(y, 1), round(p, 2)] for x, y, p in self.points]}

    @classmethod
    def from_dict(cls, data: dict) -> Stroke:
        return cls(data.get("tool", "pen"), data.get("color", INK_COLORS[0]),
                   float(data.get("width", WIDTHS[1])),
                   [tuple(p) if len(p) == 3 else (p[0], p[1], 1.0) for p in data.get("points", [])])

    @property
    def effective_width(self) -> float:
        return self.width * (HIGHLIGHTER_SCALE if self.tool == "highlighter" else 1.0)

    def hit(self, x: float, y: float, radius: float) -> bool:
        reach = radius + self.effective_width / 2
        pts = self.points
        if len(pts) == 1:
            return math.hypot(pts[0][0] - x, pts[0][1] - y) <= reach
        return any(_seg_dist(x, y, a, b) <= reach for a, b in pairwise(pts))


def _seg_dist(px: float, py: float, a, b) -> float:
    ax, ay, bx, by = a[0], a[1], b[0], b[1]
    dx, dy = bx - ax, by - ay
    length2 = dx * dx + dy * dy
    t = 0.0 if length2 == 0 else max(0.0, min(1.0, ((px - ax) * dx + (py - ay) * dy) / length2))
    return math.hypot(px - (ax + t * dx), py - (ay + t * dy))


class Drawing:
    """A list of strokes with undo/redo.

    History entries are ``("add", [strokes])`` or ``("remove", [batch, ...])``
    where each batch is a list of ``(index, stroke)`` removed in one go, with
    indices relative to the stroke list just before that batch was removed.
    """

    def __init__(self, strokes: list[Stroke] | None = None):
        self.strokes: list[Stroke] = strokes or []
        self._undo: list[tuple] = []
        self._redo: list[tuple] = []
        self._erasing: list[list[tuple[int, Stroke]]] | None = None

    @classmethod
    def from_json(cls, data: list[dict]) -> Drawing:
        return cls([Stroke.from_dict(d) for d in data])

    def to_json(self) -> list[dict]:
        return [s.to_dict() for s in self.strokes]

    @property
    def can_undo(self) -> bool:
        return bool(self._undo)

    @property
    def can_redo(self) -> bool:
        return bool(self._redo)

    def add(self, stroke: Stroke) -> None:
        if not stroke.points:
            return
        self.strokes.append(stroke)
        self._push(("add", [stroke]))

    # Erasing is grouped so one drag of the eraser is a single undo step.
    def begin_erase(self) -> None:
        self._erasing = []

    def erase_at(self, x: float, y: float, radius: float = 6.0) -> bool:
        hits = [(i, s) for i, s in enumerate(self.strokes) if s.hit(x, y, radius)]
        if not hits:
            return False
        for i, _ in reversed(hits):
            del self.strokes[i]
        if self._erasing is not None:
            self._erasing.append(hits)
        else:
            self._push(("remove", [hits]))
        return True

    def end_erase(self) -> None:
        if self._erasing:
            self._push(("remove", self._erasing))
        self._erasing = None

    def clear(self) -> None:
        if self.strokes:
            self._push(("remove", [list(enumerate(self.strokes))]))
            self.strokes = []

    def undo(self) -> bool:
        if not self._undo:
            return False
        entry = self._undo.pop()
        self._apply(entry, reverse=True)
        self._redo.append(entry)
        return True

    def redo(self) -> bool:
        if not self._redo:
            return False
        entry = self._redo.pop()
        self._apply(entry, reverse=False)
        self._undo.append(entry)
        return True

    def _push(self, entry: tuple) -> None:
        self._undo.append(entry)
        self._redo.clear()

    def _apply(self, entry: tuple, reverse: bool) -> None:
        kind, items = entry
        if kind == "add":
            for s in items:
                if reverse:
                    self.strokes.remove(s)
                else:
                    self.strokes.append(s)
        else:
            if reverse:
                for batch in reversed(items):
                    for i, s in batch:  # ascending indices restore positions
                        self.strokes.insert(i, s)
            else:
                for batch in items:
                    for _, s in batch:
                        self.strokes.remove(s)


def smooth_points(points: list[tuple[float, float, float]]) -> list[tuple[float, float, float]]:
    """Light moving-average smoothing that keeps the end points fixed."""
    if len(points) < 3:
        return list(points)
    out = [points[0]]
    for a, b, c in zip(points, points[1:], points[2:], strict=False):
        out.append(((a[0] + 2 * b[0] + c[0]) / 4, (a[1] + 2 * b[1] + c[1]) / 4, b[2]))
    out.append(points[-1])
    return out
