---
title: Install
---

# Install

Realer Xpad is a single, small **Rust** binary. The only things it needs at
runtime are **GTK ≥ 4.16** and **libadwaita ≥ 1.5**, which every current
desktop distro already ships.

## Arch Linux (AUR)

```sh
yay -S realer-xpad-bin   # prebuilt binary
yay -S realer-xpad       # or build from source (x86_64 and aarch64)
```

Any AUR helper works (`paru`, `yay`, …). Updates come with your normal system
upgrades.

## Option A: AppImage (recommended)

One file with GTK 4 and libadwaita bundled. Nothing to install, and it runs even
where your distro's own GTK is too old.

1. Download `Realer_Xpad-<version>-x86_64.AppImage` from the
   [latest release](https://github.com/giosuel/realer-xpad/releases).
2. Make it executable and run it:

    ```sh
    chmod +x Realer_Xpad-*-x86_64.AppImage
    ./Realer_Xpad-*-x86_64.AppImage
    ```

Works on x86_64 distros with glibc 2.39 or newer: Ubuntu 24.04+, Debian 13,
Fedora 40+, Arch and others. It runs natively on Wayland.

!!! tip "Using it with Hyprland and keybinds"
    Move it somewhere on your `PATH` under a short name, so the
    [Hyprland config](hyprland.md) works as written:

    ```sh
    install -m 755 Realer_Xpad-*-x86_64.AppImage ~/.local/bin/realer-xpad
    ```

    To update later, download the new version and replace the file, or use
    [AppImageUpdate](https://github.com/AppImageCommunity/AppImageUpdate), which
    only downloads what changed.

## Flatpak

[Releases](https://github.com/giosuel/realer-xpad/releases) from 0.1.2 on include
a Flatpak bundle. It runs sandboxed on the GNOME 51 runtime, which Flatpak fetches
from Flathub:

```sh
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user Realer_Xpad-*-x86_64.flatpak
flatpak run ch.giosuel.RealerXpad
```

The sandbox keeps its notes in `~/.var/app/ch.giosuel.RealerXpad/data/realer-xpad`,
separate from a native install.

## Option B: prebuilt binary

If your distro already has **GTK ≥ 4.16** and **libadwaita ≥ 1.5**, the plain
binary is much smaller (~1 MB download):

```sh
tar xzf realer-xpad-*-x86_64-linux.tar.gz
cd realer-xpad-*-x86_64-linux
./install.sh
```

## Option C: build from source

You need [Rust](https://rustup.rs) (1.92 or newer) and the GTK development files:

=== ":material-arch: Arch"

    ```sh
    sudo pacman -S gtk4 libadwaita base-devel
    ```

=== ":material-debian: Debian / Ubuntu"

    ```sh
    sudo apt install libgtk-4-dev libadwaita-1-dev build-essential
    ```

=== ":material-fedora: Fedora"

    ```sh
    sudo dnf install gtk4-devel libadwaita-devel gcc
    ```

Then:

```sh
git clone https://github.com/giosuel/realer-xpad
cd realer-xpad
./install.sh        # runs cargo build --release, then installs
```

Either way, the installer puts a `realer-xpad` command in `~/.local/bin` and a
**Realer Xpad** entry in your app launcher. It doesn't need root.

!!! tip "Not on your PATH?"
    If `realer-xpad` isn't found, add `~/.local/bin` to your `PATH`.

## 3. First launch

```sh
realer-xpad
```

Your first note opens. Start typing, and that's it. If you use **Hyprland**,
take two minutes to [set it up](hyprland.md) so notes float instead of
tiling.

## Where are my notes?

Each note is stored in `~/.local/share/realer-xpad/` as a `.md` file holding
its text, plus a small `.json` sidecar for its colour, size, position and
drawing. Deleted notes are moved to `trash/` in the same folder.

Set `REALER_XPAD_DATA=/some/dir` to keep notes elsewhere, for example in a
synced folder.

## Updating & uninstalling

Install a newer release the same way, or `git pull && ./install.sh` in your
checkout. To remove Realer Xpad, run `./install.sh --uninstall`. Uninstalling
never touches your notes.

!!! info "Coming from the Python version?"
    Nothing to migrate. The Rust version reads the same note files.
