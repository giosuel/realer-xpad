from realer_xpad.drawing import Drawing, Stroke, smooth_points


def line(x0, y0, x1, y1, tool="pen"):
    return Stroke(tool, "#000000", 2.0, [(x0, y0, 1.0), (x1, y1, 1.0)])


def test_add_undo_redo():
    d = Drawing()
    a, b = line(0, 0, 10, 0), line(0, 10, 10, 10)
    d.add(a)
    d.add(b)
    assert d.undo() and d.strokes == [a]
    assert d.redo() and d.strokes == [a, b]
    assert not d.redo()


def test_new_stroke_clears_redo():
    d = Drawing()
    d.add(line(0, 0, 1, 1))
    d.undo()
    d.add(line(2, 2, 3, 3))
    assert not d.can_redo


def test_erase_hits_only_nearby_strokes():
    d = Drawing()
    a, b = line(0, 0, 100, 0), line(0, 50, 100, 50)
    d.add(a)
    d.add(b)
    assert d.erase_at(50, 3)
    assert d.strokes == [b]
    assert not d.erase_at(50, 25)


def test_grouped_erase_undo_restores_order():
    d = Drawing()
    strokes = [line(0, y, 100, y) for y in (0, 20, 40, 60)]
    for s in strokes:
        d.add(s)
    d.begin_erase()
    d.erase_at(50, 20)   # removes strokes[1]
    d.erase_at(50, 60)   # removes strokes[3]
    d.erase_at(50, 0)    # removes strokes[0]
    d.end_erase()
    assert d.strokes == [strokes[2]]
    d.undo()
    assert d.strokes == strokes
    d.redo()
    assert d.strokes == [strokes[2]]


def test_erase_multiple_in_one_spot_undo():
    d = Drawing()
    strokes = [line(0, 0, 100, 0), line(50, -50, 50, 50), line(0, 90, 10, 90), line(0, 1, 100, 1)]
    for s in strokes:
        d.add(s)
    d.erase_at(50, 0)
    assert d.strokes == [strokes[2]]
    d.undo()
    assert d.strokes == strokes


def test_clear_is_undoable():
    d = Drawing()
    d.add(line(0, 0, 1, 1))
    d.clear()
    assert d.strokes == []
    d.undo()
    assert len(d.strokes) == 1


def test_highlighter_hit_uses_wider_width():
    s = line(0, 0, 100, 0, tool="highlighter")  # 2 * 4 = 8px wide
    assert s.hit(50, 6, 2)
    assert not line(0, 0, 100, 0).hit(50, 6, 2)


def test_json_roundtrip():
    d = Drawing()
    d.add(Stroke("highlighter", "#ff0000", 4.0, [(1.234, 2.0, 0.5), (3.0, 4.0, 0.75)]))
    again = Drawing.from_json(d.to_json())
    s = again.strokes[0]
    assert (s.tool, s.color, s.width) == ("highlighter", "#ff0000", 4.0)
    assert s.points[0] == (1.2, 2.0, 0.5)


def test_from_json_accepts_points_without_pressure():
    s = Stroke.from_dict({"points": [[1, 2], [3, 4]]})
    assert s.points == [(1, 2, 1.0), (3, 4, 1.0)]


def test_smooth_keeps_endpoints():
    pts = [(0, 0, 1), (10, 10, 1), (20, 0, 1)]
    out = smooth_points(pts)
    assert out[0] == pts[0] and out[-1] == pts[-1]
    assert out[1][1] == 5
