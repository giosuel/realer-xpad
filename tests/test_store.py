from realer_xpad.store import Note, NoteStore


def test_save_and_load_roundtrip(tmp_path):
    store = NoteStore(tmp_path)
    note = Note(text="# Hi\nthere", color="mint", x=10, y=20, strokes=[{"tool": "pen", "points": []}])
    store.save(note)
    assert (tmp_path / f"{note.id}.md").read_text() == "# Hi\nthere"
    loaded = store.load(note.id)
    assert loaded == note


def test_load_all_sorted_by_creation(tmp_path):
    store = NoteStore(tmp_path)
    a, b = Note(text="a", created=2), Note(text="b", created=1)
    store.save(a)
    store.save(b)
    assert [n.text for n in store.load_all()] == ["b", "a"]


def test_delete_moves_to_trash_and_restore(tmp_path):
    store = NoteStore(tmp_path)
    note = Note(text="bye")
    store.save(note)
    store.delete(note)
    assert store.load_all() == []
    assert (tmp_path / "trash" / f"{note.id}.md").exists()
    assert store.restore(note.id).text == "bye"


def test_corrupt_and_unknown_fields_are_tolerated(tmp_path):
    store = NoteStore(tmp_path)
    (tmp_path / "bad.json").write_text("{not json")
    (tmp_path / "old.json").write_text('{"color": "neon", "future_field": 1}')
    notes = store.load_all()
    assert len(notes) == 1
    assert notes[0].id == "old" and notes[0].color == "yellow" and notes[0].text == ""


def test_title():
    assert Note(text="\n## Shopping list\n- milk").title == "Shopping list"
    assert Note(text="- [ ] call mom").title == "call mom"
    assert Note(text="").title == "Empty note"
    assert Note(strokes=[{}]).title == "Drawing"


def test_env_data_dir(tmp_path, monkeypatch):
    monkeypatch.setenv("REALER_XPAD_DATA", str(tmp_path / "x"))
    assert NoteStore().dir == tmp_path / "x"
