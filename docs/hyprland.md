---
title: Hyprland
---

# Hyprland

<figure class="rx-figure" markdown>
  <img src="../assets/hyprland-pin.svg" alt="A pinned note staying in place while workspaces switch" width="600">
</figure>

Realer Xpad runs on any Wayland compositor, but it has extra integration for
Hyprland: it talks to Hyprland's IPC socket to **float**, **place**, **resize**
and **pin** your notes.

## Why the window rules?

Wayland doesn't let an app decide where its windows go, because that's the
compositor's job. Without rules, Hyprland treats notes like any other window
and **tiles** them. One rule fixes that.

## Setup

Since 0.55, Hyprland is configured in Lua (`~/.config/hypr/hyprland.lua`), and
the older `hyprland.conf` syntax is on its way out. Pick the tab for your setup.
The ready-made files are
[`extras/hyprland.lua`](https://github.com/giosuel/realer-xpad/blob/main/extras/hyprland.lua)
and [`extras/hyprland.conf`](https://github.com/giosuel/realer-xpad/blob/main/extras/hyprland.conf).

=== "Lua (0.55+)"

    ```lua
    local notes = [[^(ch\.giosuel\.RealerXpad)$]]

    hl.window_rule({
        match = { class = notes },
        float = true,
        border_size = 0,
        rounding = 14,
        no_blur = true,
    })

    hl.on("hyprland.start", function()
        hl.exec_cmd("realer-xpad")
    end)

    hl.bind("SUPER + N", hl.dsp.exec_cmd("realer-xpad new"))              -- new note
    hl.bind("SUPER + SHIFT + N", hl.dsp.exec_cmd("realer-xpad toggle"))   -- hide / show all notes
    hl.bind("SUPER + ALT + N", hl.dsp.exec_cmd("realer-xpad manager"))    -- notes manager
    ```

    That's everything: rules, autostart and keybinds.

=== "hyprland.conf 0.53+"

    ```ini
    windowrule = match:class ^(ch\.giosuel\.RealerXpad)$, float on
    windowrule = match:class ^(ch\.giosuel\.RealerXpad)$, border_size 0
    windowrule = match:class ^(ch\.giosuel\.RealerXpad)$, rounding 14
    windowrule = match:class ^(ch\.giosuel\.RealerXpad)$, no_blur on
    ```

=== "0.48 – 0.52"

    ```ini
    windowrule = float, class:^(ch\.giosuel\.RealerXpad)$
    windowrule = noborder, class:^(ch\.giosuel\.RealerXpad)$
    windowrule = rounding 14, class:^(ch\.giosuel\.RealerXpad)$
    windowrule = noblur, class:^(ch\.giosuel\.RealerXpad)$
    ```

=== "Older"

    ```ini
    windowrulev2 = float, class:^(ch\.giosuel\.RealerXpad)$
    windowrulev2 = noborder, class:^(ch\.giosuel\.RealerXpad)$
    windowrulev2 = rounding 14, class:^(ch\.giosuel\.RealerXpad)$
    windowrulev2 = noblur, class:^(ch\.giosuel\.RealerXpad)$
    ```

With `hyprland.conf`, then start it with Hyprland and bind it to some keys:

```ini
exec-once = realer-xpad

bind = SUPER, N, exec, realer-xpad new            # new note
bind = SUPER SHIFT, N, exec, realer-xpad toggle   # hide / show all notes
bind = SUPER ALT, N, exec, realer-xpad manager    # notes manager
```

!!! note "Installed as a Flatpak?"
    Use `flatpak run ch.giosuel.RealerXpad` wherever these lines say
    `realer-xpad`, e.g. `hl.bind("SUPER + N", hl.dsp.exec_cmd("flatpak run ch.giosuel.RealerXpad new"))`
    or `bind = SUPER, N, exec, flatpak run ch.giosuel.RealerXpad new`.
    The Flatpak may read Hyprland's socket, so pinning and placement work the same.

Reload with `hyprctl reload`, and you're set.

## What the integration does

Remembers where notes are
:   While notes are open, Realer Xpad reads each note's position and size from
    Hyprland every few seconds and saves them. Next time, every note opens
    exactly where you left it.

Pins notes to every workspace
:   Hover a note and click :material-pin-outline:. Hyprland pins it: it floats
    above your windows and stays visible on every workspace. Pinning is
    remembered too.

Hides gracefully
:   Closing a note with `killactive` (e.g. ++super+q++) hides it instead of
    deleting it. It's still in the notes manager.

!!! info "How it finds its windows"
    Each note window starts with a unique initial title
    (`realer-xpad-<note id>`), which Hyprland reports as `initialTitle`. That
    lets the app match a note to its window even after the title changes to
    your note's heading. It needs nothing beyond the
    `HYPRLAND_INSTANCE_SIGNATURE` socket Hyprland already provides, and not
    even the `hyprctl` binary. It speaks both dispatcher syntaxes: Lua
    (`hl.dsp.window.pin(...)`) for Hyprland 0.55+ with a Lua config, and the
    classic one for older versions and `hyprland.conf` setups.

## Turning it off

Set `REALER_XPAD_NO_HYPR=1` to make Realer Xpad behave like it would on any
other compositor.
