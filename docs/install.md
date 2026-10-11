---
title: Install
---

# Install

Realer Xpad is a single, small **Rust** binary. The only things it needs at
runtime are **GTK ≥ 4.16** and **libadwaita ≥ 1.5**, which every current
desktop distro already ships.

## Option A: prebuilt binary

1. Download `realer-xpad-<version>-x86_64-linux.tar.gz` from the
   [latest release](https://github.com/giosuel/realer-xpad/releases).
2. Unpack it and run the installer:

    ```sh
    tar xzf realer-xpad-*-x86_64-linux.tar.gz
    cd realer-xpad-*-x86_64-linux
    ./install.sh
    ```

!!! note
    Release binaries are built on Debian 13. They run on distros from the same
    generation or newer: Debian 13, Ubuntu 25.04+, Fedora 41+, Arch. On anything
    older, build from source.

## Option B: build from source

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
