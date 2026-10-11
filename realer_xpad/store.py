"""Note persistence.

Each note is stored as two files in the data directory:

* ``<id>.md``   – the note's Markdown text, readable with any editor
* ``<id>.json`` – metadata (colour, size, position, drawing strokes, ...)

Deleted notes are moved to ``trash/`` instead of being removed outright.
"""

from __future__ import annotations

import json
import os
import shutil
import time
import uuid
from dataclasses import asdict, dataclass, field
from pathlib import Path

COLORS = ("yellow", "peach", "pink", "lavender", "sky", "mint", "graphite")
DEFAULT_COLOR = "yellow"
DEFAULT_SIZE = (300, 300)


def default_data_dir() -> Path:
    env = os.environ.get("REALER_XPAD_DATA")
    if env:
        return Path(env)
    base = os.environ.get("XDG_DATA_HOME") or Path.home() / ".local" / "share"
    return Path(base) / "realer-xpad"


@dataclass
class Note:
    id: str = field(default_factory=lambda: uuid.uuid4().hex)
    text: str = ""
    color: str = DEFAULT_COLOR
    width: int = DEFAULT_SIZE[0]
    height: int = DEFAULT_SIZE[1]
    # Position is only known/restorable on compositors that expose it (Hyprland).
    x: int | None = None
    y: int | None = None
    pinned: bool = False
    visible: bool = True
    mode: str = "text"  # "text" or "draw"
    strokes: list[dict] = field(default_factory=list)
    created: float = field(default_factory=time.time)
    updated: float = field(default_factory=time.time)

    @property
    def title(self) -> str:
        for line in self.text.splitlines():
            line = line.strip().lstrip("#>-*+ ").replace("[ ] ", "").replace("[x] ", "").strip()
            if line:
                return line[:60]
        return "Drawing" if self.strokes else "Empty note"

    def meta(self) -> dict:
        data = asdict(self)
        del data["text"]
        return data


class NoteStore:
    def __init__(self, directory: Path | str | None = None):
        self.dir = Path(directory) if directory else default_data_dir()
        self.dir.mkdir(parents=True, exist_ok=True)

    def _md(self, note_id: str) -> Path:
        return self.dir / f"{note_id}.md"

    def _json(self, note_id: str) -> Path:
        return self.dir / f"{note_id}.json"

    def load_all(self) -> list[Note]:
        notes = []
        for meta_path in self.dir.glob("*.json"):
            note = self.load(meta_path.stem)
            if note is not None:
                notes.append(note)
        notes.sort(key=lambda n: n.created)
        return notes

    def load(self, note_id: str) -> Note | None:
        try:
            meta = json.loads(self._json(note_id).read_text())
        except (OSError, ValueError):
            return None
        known = Note.__dataclass_fields__
        note = Note(**{k: v for k, v in meta.items() if k in known and k != "text"})
        note.id = note_id
        md = self._md(note_id)
        note.text = md.read_text() if md.exists() else ""
        if note.color not in COLORS:
            note.color = DEFAULT_COLOR
        return note

    def save(self, note: Note, touch: bool = True) -> None:
        if touch:
            note.updated = time.time()
        _atomic_write(self._md(note.id), note.text)
        _atomic_write(self._json(note.id), json.dumps(note.meta(), indent=1))

    def delete(self, note: Note) -> None:
        trash = self.dir / "trash"
        trash.mkdir(exist_ok=True)
        for path in (self._md(note.id), self._json(note.id)):
            if path.exists():
                shutil.move(str(path), trash / path.name)

    def restore(self, note_id: str) -> Note | None:
        trash = self.dir / "trash"
        for name in (f"{note_id}.md", f"{note_id}.json"):
            if (trash / name).exists():
                shutil.move(str(trash / name), self.dir / name)
        return self.load(note_id)


def _atomic_write(path: Path, content: str) -> None:
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(content)
    os.replace(tmp, path)
