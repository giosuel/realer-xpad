---
title: Drawing
---

# Drawing

Every note has a drawing layer on top of its text. Press ++ctrl+d++ or click
:material-draw: in the note's header to start drawing, and press it again (or
++esc++) to go back to text.

## Tools

The toolbar floats at the bottom of the note while you draw.

| Tool | Key | |
|---|---|---|
| :material-pen: Pen | ++p++ | Pressure-sensitive with a stylus |
| :material-marker: Highlighter | ++h++ | Wide and translucent, good for marking up text |
| :material-eraser: Eraser | ++e++ | Removes whole strokes it touches |

The **colour dot** opens the ink picker: six inks and three sizes (fine,
medium and bold), plus *Clear drawing*.

!!! tip "Auto ink"
    The first ink isn't a fixed colour. It follows the note's text colour, so a
    sketch drawn in light mode stays readable after your system switches to
    dark mode.

## Undo & redo

++ctrl+z++ undoes and ++ctrl+shift+z++ (or ++ctrl+y++) redoes. One drag of
the eraser counts as a single step, and so does *Clear drawing*, so both can
be undone.

## Pen tablets

Realer Xpad reads **pressure** from styluses, so strokes get thicker the harder
you press. Flipping the pen to its **eraser end** erases, whichever tool is
selected.

## How drawings are stored

Strokes are saved as smoothed vector points in the note's `.json` file, not as
an image. They stay sharp at any size and add very little to the file.
