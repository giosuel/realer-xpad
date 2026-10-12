---
title: Shortcuts & CLI
---

# Shortcuts & CLI

## In a note

| Keys | Action |
|---|---|
| ++ctrl+n++ | New note |
| ++ctrl+e++ | Switch between edit and read view |
| ++ctrl+d++ | Toggle draw mode |
| ++ctrl+b++ / ++ctrl+i++ | Bold / italic (wraps the selection) |
| ++tab++ / ++shift+tab++ | Indent / outdent list item |
| ++enter++ | Continue list; on an empty item, end it |
| ++ctrl+w++ | Close (hide) the note |
| ++esc++ | Read view, or leave draw mode |
| ++ctrl++ + click | Open a link while editing |
| ++ctrl+q++ | Save everything and quit |

## In draw mode

| Keys | Action |
|---|---|
| ++p++ / ++h++ / ++e++ | Pen / highlighter / eraser |
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
