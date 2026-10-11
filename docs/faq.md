---
title: FAQ
---

# FAQ

??? question "Why Rust?"
    Because it's *realer*. More seriously: a single ~3 MB binary with no Python
    or PyGObject to install, about 35 MB less memory with a few notes open, and
    a compiler that catches whole classes of bugs before you ever see them.

??? question "The AppImage doesn't start"
    AppImages mount themselves with FUSE. If you see a FUSE error, install your
    distro's `fuse` / `fuse3` package, or run it without mounting:
    `./Realer_Xpad-*.AppImage --appimage-extract-and-run`.

??? question "Does it work on X11?"
    Realer Xpad is developed and tested on Wayland. GTK 4 also runs on X11, so
    it should work there too, but X11 isn't a target.

??? question "Does it work on GNOME, KDE or Sway?"
    Yes, on any Wayland compositor. What you lose outside Hyprland is position
    restore and pinning, because Wayland doesn't let apps place their own
    windows. On Sway, add `for_window [app_id="ch.giosuel.RealerXpad"] floating enable`
    so notes float.

??? question "My notes are tiled on Hyprland"
    Add the [window rules](hyprland.md#setup). Without them, Hyprland treats
    notes like any other window.

??? question "The app takes ~25 seconds to start"
    GTK and libadwaita ask the XDG desktop portal for your settings at startup,
    and wait for it if the portal service is broken. Check
    `systemctl --user status xdg-desktop-portal`. On Hyprland you want
    `xdg-desktop-portal-hyprland` installed. To confirm it's the portal, start
    with `ADW_DISABLE_PORTAL=1 GDK_DEBUG=no-portals realer-xpad`.

??? question "It fails to start with a missing `libgtk-4` or `libadwaita` library"
    Install GTK 4 and libadwaita from your distro (`gtk4` and `libadwaita` on
    most). Realer Xpad needs GTK ≥ 4.16, because the theme uses CSS variables.
    Debian 13, Ubuntu 25.04, Fedora 41 and Arch all ship a new enough GTK.

??? question "Can I edit notes with another editor?"
    Yes. Notes are plain `.md` files in `~/.local/share/realer-xpad/`. Edit them
    while Realer Xpad isn't running, or it may overwrite your changes with its
    own copy.

??? question "Can I sync notes between machines?"
    Point `REALER_XPAD_DATA` at a synced folder (Syncthing, Nextcloud, a git
    repo…). Avoid running Realer Xpad on two machines at the same time against
    the same folder.

??? question "How do I recover a deleted note?"
    Deleted notes are moved to `~/.local/share/realer-xpad/trash/`. Move both
    files (`<id>.md` and `<id>.json`) back to the parent folder and restart
    Realer Xpad.

??? question "Where's the tray icon?"
    There isn't one yet. Use the notes manager (`realer-xpad manager`) or a
    keybind for `realer-xpad toggle`.
