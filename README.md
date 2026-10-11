<div align="center">

<img src="docs/assets/hero.svg" alt="Animated sticky notes: a checklist ticking itself off, Markdown appearing, and a sketch drawing itself" width="760">

# Realer Xpad

**Sticky notes for Linux that speak Markdown and let you doodle.**

[![CI](https://img.shields.io/github/actions/workflow/status/giosuel/realer-xpad/ci.yml?branch=main&style=flat-square&label=CI&logo=githubactions&logoColor=white)](https://github.com/giosuel/realer-xpad/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/giosuel/realer-xpad?style=flat-square&color=8e4ec6&include_prereleases)](https://github.com/giosuel/realer-xpad/releases)
[![License: AGPL-3.0](https://img.shields.io/github/license/giosuel/realer-xpad?style=flat-square&color=3e63dd)](LICENSE)
[![Python 3.11+](https://img.shields.io/badge/python-3.11%2B-30a46c?style=flat-square&logo=python&logoColor=white)](pyproject.toml)
[![GTK 4 + libadwaita](https://img.shields.io/badge/GTK_4-libadwaita-f76b15?style=flat-square&logo=gnome&logoColor=white)](https://gnome.pages.gitlab.gnome.org/libadwaita/)
[![Wayland native](https://img.shields.io/badge/Wayland-native-e5484d?style=flat-square&logo=wayland&logoColor=white)](#)
[![Hyprland ready](https://img.shields.io/badge/Hyprland-ready-58e1ff?style=flat-square&logo=hyprland&logoColor=white)](#hyprland)

[**Website**](https://giosuel.github.io/realer-xpad/) · [**Install**](#install) · [**Hyprland setup**](#hyprland) · [**Shortcuts**](#shortcuts)

</div>

---

Remember [xpad](https://launchpad.net/xpad)? Quick little notes stuck on your desktop. Realer Xpad keeps that idea and adds what was missing: **real formatting**, **a pen**, and a look that fits a 2026 desktop.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/collage-dark.png">
  <img src="docs/collage.png" alt="Realer Xpad notes: a checklist, a Markdown note, a drawing and the notes manager">
</picture>

## ✨ What you get

<table>
<tr>
<td width="33%" valign="top">

**📝 Live Markdown**<br>
Headings, **bold**, *italic*, `code`, quotes, links and lists. Styling appears as you type, and a clean read view shows when you click away.

</td>
<td width="33%" valign="top">

**☑️ Real checklists**<br>
Click a `- [ ]` box to tick it. <kbd>Enter</kbd> continues the list and <kbd>Tab</kbd> nests it.

</td>
<td width="33%" valign="top">

**✏️ Draw on anything**<br>
Pen, highlighter and eraser, with stylus pressure. The default ink adapts to light and dark mode.

</td>
</tr>
<tr>
<td valign="top">

**🎨 Seven colours**<br>
Soft pastels by day and deep tones by night. Notes follow your system theme.

</td>
<td valign="top">

**🪟 Wayland-native**<br>
GTK 4 + libadwaita, no Xwayland. On **Hyprland**, notes float, remember where you put them, and can be pinned to every workspace.

</td>
<td valign="top">

**📂 Plain files**<br>
Every note is a `.md` file in `~/.local/share/realer-xpad`, so you can grep, sync or back up your notes with ordinary tools.

</td>
</tr>
</table>

## 🚀 Install

```sh
# 1. dependencies (GTK ≥ 4.16, libadwaita ≥ 1.5)
sudo apt install python3-gi gir1.2-gtk-4.0 gir1.2-adw-1   # Debian / Ubuntu
sudo pacman -S python-gobject gtk4 libadwaita              # Arch
sudo dnf install python3-gobject gtk4 libadwaita           # Fedora

# 2. Realer Xpad
git clone https://github.com/giosuel/realer-xpad && cd realer-xpad
./install.sh
```

Then launch **Realer Xpad** from your app launcher, or run `realer-xpad`.

## 🌊 Hyprland

Add the rules from [`extras/hyprland.conf`](extras/hyprland.conf) so notes float instead of tiling:

```ini
windowrule = match:class ^(ch\.giosuel\.RealerXpad)$, float on
windowrule = match:class ^(ch\.giosuel\.RealerXpad)$, border_size 0

exec-once = realer-xpad
bind = SUPER, N, exec, realer-xpad new           # new note
bind = SUPER SHIFT, N, exec, realer-xpad toggle  # hide / show all
```

> [!TIP]
> On Hyprland older than 0.53, use the legacy rule syntax included at the bottom of that file.

## ⌨️ Shortcuts

<details>
<summary><b>Keyboard shortcuts and CLI commands</b></summary>
<br>

| Key | Action |
|---|---|
| <kbd>Ctrl</kbd> <kbd>N</kbd> | New note |
| <kbd>Ctrl</kbd> <kbd>D</kbd> | Toggle draw mode |
| <kbd>Ctrl</kbd> <kbd>E</kbd> | Switch edit / read view |
| <kbd>Ctrl</kbd> <kbd>B</kbd> · <kbd>Ctrl</kbd> <kbd>I</kbd> | Bold · italic |
| <kbd>Tab</kbd> · <kbd>Shift</kbd> <kbd>Tab</kbd> | Indent · outdent list item |
| <kbd>P</kbd> · <kbd>H</kbd> · <kbd>E</kbd> | Pen · highlighter · eraser (draw mode) |
| <kbd>Ctrl</kbd> <kbd>Z</kbd> · <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>Z</kbd> | Undo · redo |
| <kbd>Ctrl</kbd> <kbd>W</kbd> | Close (hide) note |
| <kbd>Esc</kbd> | Read view / leave draw mode |

| Command | |
|---|---|
| `realer-xpad` | Start, or show all open notes |
| `realer-xpad new` | New note |
| `realer-xpad toggle` | Hide / show all notes |
| `realer-xpad manager` | Notes manager |
| `realer-xpad quit` | Save and quit |

</details>

## 🛠️ Contributing

Bug reports and PRs are welcome. The test suite includes UI tests that run in a throwaway headless sway session:

```sh
python3 -m venv --system-site-packages .venv && .venv/bin/pip install pytest
tools/headless.sh .venv/bin/pytest
```

<div align="center">
<sub>Made with ☕ by <a href="https://github.com/giosuel">giosuel</a> · Licensed under <a href="LICENSE">AGPL-3.0</a></sub>
</div>
