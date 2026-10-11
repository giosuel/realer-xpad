"""Drawing layer widget rendered with GSK paths (GPU accelerated, no cairo)."""

from __future__ import annotations

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Gsk", "4.0")
gi.require_version("Graphene", "1.0")
from gi.repository import Gdk, GObject, Graphene, Gsk, Gtk

from .drawing import HIGHLIGHTER_ALPHA, INK_COLORS, WIDTHS, Drawing, Stroke, smooth_points

ERASER_RADIUS = 8.0


def _rgba(color: str, ink: Gdk.RGBA, alpha: float = 1.0) -> Gdk.RGBA:
    c = ink.copy() if color == "ink" else Gdk.RGBA()
    if color != "ink":
        c.parse(color)
    c.alpha *= alpha
    return c


def _path(points) -> Gsk.Path:
    """Smooth path through ``points`` using quadratic curves between midpoints."""
    b = Gsk.PathBuilder.new()
    x0, y0 = points[0][0], points[0][1]
    b.move_to(x0, y0)
    if len(points) == 1:
        b.line_to(x0 + 0.01, y0)  # a dot
    elif len(points) == 2:
        b.line_to(points[1][0], points[1][1])
    else:
        for (ax, ay, _), (bx, by, _) in zip(points[1:], points[2:]):
            b.quad_to(ax, ay, (ax + bx) / 2, (ay + by) / 2)
        b.line_to(points[-1][0], points[-1][1])
    return b.to_path()


def _stroke_style(width: float) -> Gsk.Stroke:
    st = Gsk.Stroke.new(width)
    st.set_line_cap(Gsk.LineCap.ROUND)
    st.set_line_join(Gsk.LineJoin.ROUND)
    return st


def draw_stroke(snapshot: Gtk.Snapshot, stroke: Stroke, ink: Gdk.RGBA) -> None:
    pts = stroke.points
    if not pts:
        return
    if stroke.tool == "highlighter":
        snapshot.append_stroke(_path(pts), _stroke_style(stroke.effective_width),
                               _rgba(stroke.color, ink, HIGHLIGHTER_ALPHA))
        return
    color = _rgba(stroke.color, ink)
    pressures = {round(p[2], 2) for p in pts}
    if len(pressures) <= 1:
        width = stroke.width * _pressure_scale(pts[0][2])
        snapshot.append_stroke(_path(pts), _stroke_style(width), color)
        return
    # Pressure varies: draw short segments with their own widths.
    for a, b in zip(pts, pts[1:]):
        width = stroke.width * _pressure_scale((a[2] + b[2]) / 2)
        snapshot.append_stroke(_path([a, b]), _stroke_style(width), color)


def _pressure_scale(p: float) -> float:
    return 0.35 + 0.9 * max(0.0, min(1.0, p))


class DrawingCanvas(Gtk.Widget):
    __gtype_name__ = "RealerXpadCanvas"
    __gsignals__ = {"changed": (GObject.SignalFlags.RUN_LAST, None, ())}

    def __init__(self, drawing: Drawing | None = None):
        super().__init__(hexpand=True, vexpand=True)
        self.drawing = drawing or Drawing()
        self.tool = "pen"
        self.color = INK_COLORS[0]
        self.width = WIDTHS[1]
        self._current: Stroke | None = None
        self._erasing = False
        self._start = (0.0, 0.0)
        self._hover: tuple[float, float] | None = None
        self._cache: Gsk.RenderNode | None = None

        drag = Gtk.GestureDrag()
        drag.connect("drag-begin", self._on_begin)
        drag.connect("drag-update", self._on_update)
        drag.connect("drag-end", self._on_end)
        self.add_controller(drag)

        motion = Gtk.EventControllerMotion()
        motion.connect("motion", lambda _c, x, y: self._set_hover((x, y)))
        motion.connect("leave", lambda _c: self._set_hover(None))
        self.add_controller(motion)
        self.set_cursor_from_name("crosshair")

    # -- public API -----------------------------------------------------

    def set_drawing(self, drawing: Drawing) -> None:
        self.drawing = drawing
        self.invalidate()

    def set_tool(self, tool: str) -> None:
        self.tool = tool
        self.set_cursor_from_name("none" if tool == "eraser" else "crosshair")
        self.queue_draw()

    def invalidate(self) -> None:
        self._cache = None
        self.queue_draw()

    def undo(self) -> None:
        if self.drawing.undo():
            self._changed()

    def redo(self) -> None:
        if self.drawing.redo():
            self._changed()

    def clear(self) -> None:
        self.drawing.clear()
        self._changed()

    # -- input ----------------------------------------------------------

    def _event_info(self, gesture) -> tuple[float, bool]:
        """Pressure and whether the stylus' eraser end is used."""
        event = gesture.get_last_event(gesture.get_last_updated_sequence())
        pressure, eraser_tip = 1.0, False
        if event is not None:
            ok, value = event.get_axis(Gdk.AxisUse.PRESSURE)
            if ok and value > 0:
                pressure = value
            tool = event.get_device_tool()
            eraser_tip = tool is not None and tool.get_tool_type() == Gdk.DeviceToolType.ERASER
        return pressure, eraser_tip

    def _on_begin(self, gesture, x, y) -> None:
        self._start = (x, y)
        pressure, eraser_tip = self._event_info(gesture)
        if self.tool == "eraser" or eraser_tip:
            self._erasing = True
            self.drawing.begin_erase()
            self._erase(x, y)
            return
        self._current = Stroke(self.tool, self.color, self.width, [(x, y, pressure)])
        self.queue_draw()

    def _on_update(self, gesture, dx, dy) -> None:
        x, y = self._start[0] + dx, self._start[1] + dy
        self._hover = (x, y)
        if self._erasing:
            self._erase(x, y)
            return
        if self._current is None:
            return
        last = self._current.points[-1]
        if abs(last[0] - x) + abs(last[1] - y) < 1.5:
            return  # ignore jitter
        pressure, _ = self._event_info(gesture)
        self._current.points.append((x, y, pressure))
        self.queue_draw()

    def _on_end(self, gesture, dx, dy) -> None:
        if self._erasing:
            self._erasing = False
            self.drawing.end_erase()
            self.emit("changed")
            return
        if self._current is not None:
            self._current.points = smooth_points(self._current.points)
            self.drawing.add(self._current)
            self._current = None
            self._changed()

    def _erase(self, x, y) -> None:
        if self.drawing.erase_at(x, y, ERASER_RADIUS):
            self.invalidate()
        else:
            self.queue_draw()

    def _set_hover(self, pos) -> None:
        self._hover = pos
        if self.tool == "eraser":
            self.queue_draw()

    def _changed(self) -> None:
        self.invalidate()
        self.emit("changed")

    # -- rendering ------------------------------------------------------

    def do_snapshot(self, snapshot: Gtk.Snapshot) -> None:
        ink = self.get_color()
        if self._cache is None and self.drawing.strokes:
            inner = Gtk.Snapshot()
            for stroke in self.drawing.strokes:
                draw_stroke(inner, stroke, ink)
            self._cache = inner.to_node()
        if self._cache is not None:
            snapshot.append_node(self._cache)
        if self._current is not None:
            draw_stroke(snapshot, self._current, ink)
        if self.tool == "eraser" and self._hover and self.get_can_target():
            x, y = self._hover
            b = Gsk.PathBuilder.new()
            b.add_circle(Graphene.Point().init(x, y), ERASER_RADIUS)
            fg = self.get_color()
            fg.alpha = 0.6
            snapshot.append_stroke(b.to_path(), Gsk.Stroke.new(1.2), fg)
