---
title: Shortcuts & CLI
---

# Shortcuts & CLI

## In a note

| Keys | Action |
|---|---|
| ++ctrl+n++ | New note |
| ++ctrl+e++ | Switch between edit and read view |
| ++ctrl+d++ | Switch between typing and the last drawing tool |
| ++ctrl+b++ / ++ctrl+i++ | Bold / italic (wraps the selection) |
| ++tab++ / ++shift+tab++ | Indent / outdent list item |
| ++enter++ | Continue list; on an empty item, end it |
| ++ctrl+w++ | Close (hide) the note |
| ++esc++ | Read view, or back to Text from a drawing tool |
| ++ctrl++ + click | Open a link while editing |
| ++ctrl+q++ | Save everything and quit |

## While drawing

These work while a drawing tool is selected. With Text selected the letters
just type.

| Keys | Action |
|---|---|
| ++t++ | Text |
| ++p++ / ++h++ / ++s++ / ++e++ | Pen / highlighter / shapes / eraser |
| ++ctrl+z++ | Undo |
| ++ctrl+shift+z++ or ++ctrl+y++ | Redo |

## Command line

Realer Xpad runs as a single instance. Running the command again sends the
action to the instance that's already running, which makes it easy to bind to
keys in any compositor.

| Command | |
|---|---|
| `realer-xpad` | Start, or show all open notes |
| `realer-xpad new` | Create a new note |
| `realer-xpad toggle` | Hide all notes, or show them if hidden |
| `realer-xpad hide` | Hide all notes |
| `realer-xpad manager` | Open the notes manager |
| `realer-xpad import-xpad` | Import notes from xpad |
| `realer-xpad quit` | Save everything and quit |
| `realer-xpad --version` | Print the version |

## Environment variables

| Variable | |
|---|---|
| `REALER_XPAD_DATA` | Where notes are stored (default `~/.local/share/realer-xpad`) |
| `REALER_XPAD_NO_HYPR` | Set to turn off the Hyprland integration |
| `REALER_XPAD_NO_TRAY` | Set to run without the tray icon |
