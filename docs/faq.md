---
title: FAQ
---

# FAQ

??? question "Can I bring my notes over from xpad?"
    Yes. When Realer Xpad starts for the first time and finds notes in
    `~/.config/xpad`, it offers to import them. You can also import any time
    with **Import from xpad…** in the notes manager's menu, or
    `realer-xpad import-xpad`.

    Each note keeps its text, size, and whether it's sticky (which becomes
    pinned) or hidden. Its colour becomes the closest of the seven note colours.
    Window positions aren't kept, because xpad's are X11 screen coordinates.
    Empty notes are skipped, and importing again only brings in notes that are
    new since last time. xpad's own files are never changed.

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
    Realer Xpad shows its logo in the system tray of Waybar, KDE Plasma, and
    other bars that support StatusNotifierItem. Click it to show or hide your
    notes, middle-click for a new note, and right-click for the menu (New Note,
    Show / Hide All Notes, All Notes…, Quit). GNOME needs the
    [AppIndicator extension](https://extensions.gnome.org/extension/615/appindicator-support/)
    to show tray icons. On Waybar, add `"tray"` to your modules.

    Set `REALER_XPAD_NO_TRAY=1` to run without the icon.

??? question "How do I quit? Closing every note doesn't."
    Like xpad, Realer Xpad keeps running in the background when all notes are
    closed, so a keybind or a click on the tray icon brings them back right away.
    To really quit, choose **Quit** in the tray menu or the notes manager, press
    ++ctrl+q++, or run `realer-xpad quit`.
