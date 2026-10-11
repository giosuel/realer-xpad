"""A small Markdown parser tailored for styling a text buffer in place.

``parse`` never changes the text; it returns *spans* (style tags over source
ranges) and *markers* (syntax characters such as ``**`` or ``# ``).  The editor
applies spans as text tags and dims markers.  ``render`` produces the clean
read view by dropping/replacing markers and remapping the spans.

All offsets are Python string indices, which are Unicode code points – the same
unit Gtk.TextBuffer uses for character offsets.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field


@dataclass(frozen=True)
class Span:
    tag: str
    start: int
    end: int
    data: str | None = None  # e.g. link target


@dataclass(frozen=True)
class Marker:
    start: int
    end: int
    replace: str = ""  # what the read view shows instead
    tag: str = "marker"


@dataclass(frozen=True)
class Checkbox:
    start: int  # range of "[ ]" / "[x]"
    end: int
    checked: bool


@dataclass
class Parsed:
    spans: list[Span] = field(default_factory=list)
    markers: list[Marker] = field(default_factory=list)
    checkboxes: list[Checkbox] = field(default_factory=list)


RE_FENCE = re.compile(r"^\s*(```|~~~)")
RE_HEADING = re.compile(r"^(#{1,6})[ \t]+")
RE_HR = re.compile(r"^[ \t]*([-*_])([ \t]*\1){2,}[ \t]*$")
RE_QUOTE = re.compile(r"^[ \t]*>[ \t]?")
RE_CHECKBOX = re.compile(r"^([ \t]*)[-*+][ \t]+(\[([ xX])\])[ \t]+")
RE_BULLET = re.compile(r"^([ \t]*)[-*+][ \t]+")
RE_ORDERED = re.compile(r"^([ \t]*)(\d+[.)])[ \t]+")

RE_CODE = re.compile(r"(`+)(.+?)\1")
RE_LINK = re.compile(r"\[([^\]\n]+)\]\(([^)\s]+)\)")
RE_URL = re.compile(r"\bhttps?://[^\s<>()\[\]]+[^\s<>()\[\].,;:!?'\"]")
RE_BOLD = re.compile(r"(\*\*|__)(?=\S)(.+?)(?<=\S)\1")
RE_STRIKE = re.compile(r"(~~)(?=\S)(.+?)(?<=\S)\1")
RE_ITALIC = re.compile(r"(?<![\w*_])([*_])(?=\S)(.+?)(?<=\S)\1(?![\w*_])")

MASK = "\x00"


def parse(text: str) -> Parsed:
    out = Parsed()
    in_fence = False
    pos = 0
    for line in text.split("\n"):
        start = pos
        pos += len(line) + 1
        if RE_FENCE.match(line):
            in_fence = not in_fence
            out.markers.append(Marker(start, start + len(line), tag="fence"))
            continue
        if in_fence:
            out.spans.append(Span("code_block", start, start + len(line)))
            continue
        _parse_line(line, start, out)
    return out


def _parse_line(line: str, base: int, out: Parsed) -> None:
    end = base + len(line)
    body = 0  # index in `line` where inline content begins

    if RE_HR.match(line):
        out.markers.append(Marker(base, end, replace="─" * 16, tag="hr"))
        return

    if m := RE_HEADING.match(line):
        level = min(len(m.group(1)), 3)
        out.markers.append(Marker(base, base + m.end()))
        out.spans.append(Span(f"h{level}", base + m.end(), end))
        body = m.end()
    elif m := RE_QUOTE.match(line):
        out.markers.append(Marker(base, base + m.end()))
        out.spans.append(Span("quote", base + m.end(), end))
        body = m.end()
    elif m := RE_CHECKBOX.match(line):
        checked = m.group(3) != " "
        indent = m.group(1)
        out.markers.append(
            Marker(base + len(indent), base + m.end(), replace="☑ " if checked else "☐ ", tag="checkbox")
        )
        out.checkboxes.append(Checkbox(base + m.start(2), base + m.end(2), checked))
        if checked:
            out.spans.append(Span("done", base + m.end(), end))
        body = m.end()
    elif m := RE_BULLET.match(line):
        out.markers.append(Marker(base + len(m.group(1)), base + m.end(), replace="• ", tag="bullet"))
        body = m.end()
    elif m := RE_ORDERED.match(line):
        out.spans.append(Span("bullet", base + m.start(2), base + m.end(2)))
        body = m.end()

    _parse_inline(line[body:], base + body, out)


def _parse_inline(text: str, base: int, out: Parsed) -> None:
    masked = list(text)

    def mask(a: int, b: int) -> None:
        masked[a:b] = MASK * (b - a)

    def scan(regex: re.Pattern) -> list[re.Match]:
        return list(regex.finditer("".join(masked)))

    # Inline code first: nothing inside it is parsed.
    for m in scan(RE_CODE):
        tick = len(m.group(1))
        out.markers.append(Marker(base + m.start(), base + m.start() + tick))
        out.markers.append(Marker(base + m.end() - tick, base + m.end()))
        out.spans.append(Span("code", base + m.start(2), base + m.end(2)))
        mask(m.start(), m.end())

    for m in scan(RE_LINK):
        url = m.group(2)
        out.markers.append(Marker(base + m.start(), base + m.start(1)))
        out.markers.append(Marker(base + m.end(1), base + m.end()))
        out.spans.append(Span("link", base + m.start(1), base + m.end(1), url))
        # Mask the whole link so the URL is not picked up again, but keep the
        # label parseable for emphasis by un-masking it afterwards.
        label = masked[m.start(1):m.end(1)]
        mask(m.start(), m.end())
        masked[m.start(1):m.end(1)] = label

    for m in scan(RE_URL):
        out.spans.append(Span("link", base + m.start(), base + m.end(), m.group()))
        mask(m.start(), m.end())

    for regex, tag in ((RE_BOLD, "bold"), (RE_STRIKE, "strike"), (RE_ITALIC, "italic")):
        for m in scan(regex):
            n = len(m.group(1))
            out.markers.append(Marker(base + m.start(), base + m.start() + n))
            out.markers.append(Marker(base + m.end() - n, base + m.end()))
            out.spans.append(Span(tag, base + m.start(2), base + m.end(2)))
            mask(m.start(), m.start() + n)
            mask(m.end() - n, m.end())


@dataclass
class Rendered:
    text: str
    spans: list[Span]
    # (start, end) in rendered text -> index into Parsed.checkboxes
    checkboxes: list[tuple[int, int, int]]
    # kept source runs: (src_start, src_end, rendered_start)
    runs: list[tuple[int, int, int]] = field(default_factory=list)

    def to_source(self, offset: int) -> int:
        """Map a rendered offset back to the closest source offset."""
        for s, e, o in self.runs:
            if o <= offset < o + (e - s):
                return s + offset - o
            if offset < o:  # inside replaced marker text: snap to next run
                return s
        return self.runs[-1][1] if self.runs else 0


def render(source: str, parsed: Parsed | None = None) -> Rendered:
    """Build the read view: markers are removed or replaced, spans remapped."""
    parsed = parsed or parse(source)
    markers = sorted(parsed.markers, key=lambda m: m.start)
    checkbox_index = {cb.start: i for i, cb in enumerate(parsed.checkboxes)}

    pieces: list[str] = []
    # Piecewise offset map: list of (source_start, rendered_start) for kept runs.
    runs: list[tuple[int, int, int]] = []  # (src_start, src_end, out_start)
    rendered_boxes: list[tuple[int, int, int]] = []
    extra_spans: list[Span] = []
    src = out = 0
    for mk in markers:
        if mk.start < src:  # overlapping marker; keep the first one
            continue
        if mk.start > src:
            runs.append((src, mk.start, out))
            pieces.append(source[src:mk.start])
            out += mk.start - src
        if mk.replace:
            if mk.tag == "checkbox":
                idx = next(i for s, i in checkbox_index.items() if mk.start <= s < mk.end)
                rendered_boxes.append((out, out + 1, idx))
            extra_spans.append(Span(mk.tag, out, out + len(mk.replace)))
            pieces.append(mk.replace)
            out += len(mk.replace)
        src = mk.end
    if src < len(source):
        runs.append((src, len(source), out))
        pieces.append(source[src:])

    def remap(offset: int, is_end: bool) -> int:
        for s, e, o in runs:
            if s <= offset < e or (is_end and offset == e):
                return o + offset - s
        # Offset lies inside a removed marker: snap to the next kept run.
        for s, _e, o in runs:
            if s >= offset:
                return o
        return out

    spans = extra_spans[:]
    for sp in parsed.spans:
        a, b = remap(sp.start, False), remap(sp.end, True)
        if b > a:
            spans.append(Span(sp.tag, a, b, sp.data))
    return Rendered("".join(pieces), spans, rendered_boxes, runs)


def toggle_checkbox(source: str, checkbox: Checkbox) -> str:
    mark = " " if checkbox.checked else "x"
    return source[: checkbox.start + 1] + mark + source[checkbox.start + 2:]


def continue_list(line: str) -> str | None:
    """Prefix for the next line when pressing Enter at the end of ``line``.

    Returns ``None`` when ``line`` is not a list item, and ``""`` when the item
    is empty (pressing Enter again should end the list).
    """
    if m := RE_CHECKBOX.match(line):
        prefix = line[: m.start(2)] + "[ ] "
    elif m := RE_BULLET.match(line):
        prefix = line[: m.end()]
    elif m := RE_ORDERED.match(line):
        num = m.group(2)
        prefix = m.group(1) + str(int(num[:-1]) + 1) + num[-1] + " "
    elif m := RE_QUOTE.match(line):
        prefix = line[: m.end()]
    else:
        return None
    return "" if not line[m.end():].strip() else prefix
