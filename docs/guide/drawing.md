---
title: Drawing
---

# Drawing

Every note has a drawing layer on top of its text. The tool row at the
bottom of the note is always there: pick a drawing tool to sketch, and
:material-format-text: **Text** to type again.

++ctrl+d++ switches between typing and the drawing tool you used last. While a
drawing tool is selected, ++esc++ goes back to Text.

## Tools

| Tool | Key | |
|---|---|---|
| :material-format-text: Text | ++t++ | Type and edit as usual (the default) |
| :material-pen: Pen | ++p++ | Pressure-sensitive with a stylus |
| :material-marker: Highlighter | ++h++ | Wide and translucent, good for marking up text |
| :material-shape-outline: Shapes | ++s++ | Line, arrow, rectangle or ellipse |
| :material-eraser: Eraser | ++e++ | Removes whole strokes it touches |

The tool keys only work while a drawing tool is selected. In Text they just
type the letter.

The **ink dot** next to the tools opens the ink picker: six inks and three
sizes (fine, medium and bold), plus *Clear Drawing*. It's greyed out while you
type.

## Shapes

Drag to draw a shape from one corner (or end) to the other. The **▾** next to
the shape button picks which one: line, arrow, rectangle or ellipse. The
eraser removes a shape only when it touches the outline, so erasing a
scribble inside a box leaves the box alone.

!!! tip "Auto ink"
    The first ink isn't a fixed colour. It follows the note's text colour, so a
    sketch drawn in light mode stays readable after your system switches to
    dark mode.

## Images

Paste an image with ++ctrl+v++ (a screenshot, an image copied in a browser,
or image files copied in a file manager), or drop image files onto a note.
It lands in the middle of the note, shrunk to fit if it's bigger, and is
selected so you can place it straight away:

- drag it to **move** it;
- drag a **corner** to resize it, keeping its proportions;
- drag the **round handle** above it to rotate it; hold ++shift++ to turn in
  steps of 15°.

With Text selected, a click on an image selects it and a click anywhere else
goes back to the text. Right-click an image for *Copy Image*, *Reset Size &
Rotation* and *Delete Image*. ++delete++ removes the selected image, ++ctrl+c++
copies it and ++esc++ unselects it.

Images lie on top of the text and under the drawing, so you can circle things
on a screenshot with the pen: with a drawing tool selected, the tool draws on
the image instead of picking it up. The eraser only removes strokes, never
images.


++ctrl+z++ undoes and ++ctrl+shift+z++ (or ++ctrl+y++) redoes, as do the
arrows at the end of the tool row. With Text selected they undo typing, or
changes to images while one is selected or was the last thing you changed;
with a drawing tool they undo strokes and images. One drag of the eraser counts as a single
step, and so does *Clear Drawing*, so both can be undone.

## Pen tablets

Realer Xpad reads **pressure** from styluses, so strokes get thicker the harder
you press. Flipping the pen to its **eraser end** erases, whichever tool is
selected.

## How drawings are stored

Strokes are saved as smoothed vector points in the note's `.json` file, not as
an image. They stay sharp at any size and add very little to the file.

Pasted images are saved as PNG files in the `images` folder next to the notes,
and the note's `.json` file keeps where each one is, its size and its rotation.
