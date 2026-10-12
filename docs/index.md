---
title: Sticky notes that speak Markdown
hide:
  - navigation
  - toc
---

<div class="rx-hero" markdown>

<img class="rx-hero-art" src="assets/hero.svg" alt="Animated sticky notes: a checklist ticking itself off, Markdown appearing, and a sketch drawing itself">

<span class="rx-pill">:simple-rust: Powered by Rust</span>

<p class="rx-title" role="heading" aria-level="1">Realer Xpad</p>

<p class="rx-tagline">Sticky notes for Linux that speak <strong>Markdown</strong> and let you <strong>doodle</strong>. Wayland-native, Hyprland-ready, and powered by Rust.</p>

<div class="rx-badges">
  <a href="https://github.com/giosuel/realer-xpad/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/giosuel/realer-xpad/ci.yml?branch=main&style=flat-square&label=CI&logo=githubactions&logoColor=white"></a>
  <a href="https://github.com/giosuel/realer-xpad/releases"><img alt="Release" src="https://img.shields.io/github/v/release/giosuel/realer-xpad?style=flat-square&color=8e4ec6&include_prereleases"></a>
  <a href="https://github.com/giosuel/realer-xpad/blob/main/LICENSE"><img alt="License" src="https://img.shields.io/github/license/giosuel/realer-xpad?style=flat-square&color=3e63dd"></a>
  <a href="https://github.com/giosuel/realer-xpad/stargazers"><img alt="Stars" src="https://img.shields.io/github/stars/giosuel/realer-xpad?style=flat-square&color=f0c43c&logo=github"></a>
  <a href="https://www.rust-lang.org"><img alt="Powered by Rust" src="https://img.shields.io/badge/powered_by-Rust-dea584?style=flat-square&logo=rust&logoColor=white"></a>
  <img alt="Wayland native" src="https://img.shields.io/badge/Wayland-native-e5484d?style=flat-square&logo=wayland&logoColor=white">
  <img alt="Hyprland ready" src="https://img.shields.io/badge/Hyprland-ready-58e1ff?style=flat-square&logo=hyprland&logoColor=white">
</div>

[Get started :material-arrow-right:](install.md){ .md-button .md-button--primary }
[:fontawesome-brands-github: View on GitHub](https://github.com/giosuel/realer-xpad){ .md-button }

<div class="rx-install"><span>$</span> git clone https://github.com/giosuel/realer-xpad && ./realer-xpad/install.sh</div>

</div>

## Small notes, fewer compromises

xpad got one thing right: notes that just *sit there* on your desktop, ready for a thought. Realer Xpad keeps that and adds what was missing.

<div class="grid cards" markdown>

-   :material-language-markdown:{ .lg } **Live Markdown**

    ---

    Headings, **bold**, *italic*, `code`, quotes and links, styled as you type. Click away and the note switches to a clean read view.

-   :material-checkbox-marked-outline:{ .lg } **Checklists that click**

    ---

    `- [ ]` becomes a real checkbox. <kbd>Enter</kbd> continues a list and <kbd>Tab</kbd> nests it.

-   :material-draw:{ .lg } **Pen, highlighter, shapes, eraser**

    ---

    Sketch over your text with stylus pressure, lines, arrows, boxes and circles, six inks and undo. The tools are always in reach. The default ink adapts to light and dark mode.

-   :material-palette-swatch-variant:{ .lg } **Seven moods**

    ---

    Lemon, peach, rose, lavender, sky, mint and graphite, each with a hand-tuned dark variant that follows your system theme.

-   :material-pin-outline:{ .lg } **Hyprland-native**

    ---

    Notes float, return to where you left them, and pin to every workspace through Hyprland's IPC.

-   :simple-rust:{ .lg } **Powered by Rust**

    ---

    A single ~3 MB binary that starts fast and uses about a third less memory than the Python version did. Notes are still plain `.md` files you can grep.

</div>

<figure class="rx-figure" markdown>
  <img class="rx-shot" src="collage-dark.png" alt="Four sticky notes and the notes manager in dark mode">
  <figcaption>Notes in dark mode next to the notes manager, rendered in a pure Wayland session.</figcaption>
</figure>

## Write it plain, read it pretty

<div class="grid" markdown>

<div markdown>

You type ordinary Markdown. Realer Xpad dims the syntax while you edit, so the structure stays visible without getting in the way. When you click away, the note switches to a clean rendered view where checkboxes and links are clickable.

Lists continue themselves, <kbd>Ctrl</kbd>+<kbd>B</kbd> and <kbd>Ctrl</kbd>+<kbd>I</kbd> wrap your selection, and code fences switch off formatting inside them.

[Writing notes :material-arrow-right:](guide/notes.md)

</div>

<img src="assets/markdown-live.svg" alt="A note switching between its Markdown source and the rendered view">

</div>

## Stays with you on every workspace

<div class="grid" markdown>

<img src="assets/hyprland-pin.svg" alt="A pinned note staying in place while workspaces switch">

<div markdown>

Pin a note and it follows you across workspaces, for that one thing you can't forget today. Notes also remember their exact position and size between sessions, because Realer Xpad asks Hyprland where you put them.

Bind `realer-xpad new` and `realer-xpad toggle` to keys and notes are always one shortcut away.

[Set up Hyprland :material-arrow-right:](hyprland.md)

</div>

</div>

<div class="rx-cta" markdown>

## Stick a note on it

Free and open source under AGPL-3.0. Built with :simple-rust: Rust, GTK 4 and libadwaita.

[Install Realer Xpad :material-arrow-right:](install.md){ .md-button .md-button--primary }
[:fontawesome-brands-github: Star on GitHub](https://github.com/giosuel/realer-xpad){ .md-button }

</div>
