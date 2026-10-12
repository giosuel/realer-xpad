# Flatpak

`ch.giosuel.RealerXpad.yml` builds the app on the GNOME 51 runtime with the Rust SDK
extension. Builds are offline, so every crate in `Cargo.lock` is listed in
`cargo-sources.json`.

| File | What it is |
| --- | --- |
| `ch.giosuel.RealerXpad.yml` | Manifest. Builds the local checkout (`type: dir`). |
| `cargo-sources.json` | Crate sources. Regenerate with `update-cargo-sources.sh` whenever `Cargo.lock` changes (the CI build fails otherwise). |
| `flathub-manifest.sh` | Prints the manifest for Flathub, building a release tag instead of the checkout. |
| `test.sh` | Builds with `org.flatpak.Builder` (what Flathub uses) in a container, runs Flathub's linter, and smoke-tests the app on headless sway against a fake Hyprland socket. |

CI builds the Flatpak on every PR, and each release ships `Realer_Xpad-<version>-x86_64.flatpak`.

## Permissions

| Permission | Why |
| --- | --- |
| `--socket=wayland`, `--socket=fallback-x11`, `--share=ipc`, `--device=dri` | A GTK 4 window, with GPU rendering |
| `--filesystem=xdg-run/hypr:ro` | Hyprland's IPC socket, to float, size, place and pin note windows. Without it, notes still work but open as tiled windows |
| `--filesystem=xdg-config/xpad:ro` | Reading xpad's notes (`~/.config/xpad`) for the one-time import. Read-only |
| `--talk-name=org.kde.StatusNotifierWatcher` | The tray icon: registering it with the tray (Waybar, KDE, …). The item is served under the app's unique bus name, so it needs no `--own-name` |

The app needs no network and no portals beyond the defaults, and can't read
your home folder beyond `~/.config/xpad`.

> [!WARNING]
> **The Hyprland socket.** Hyprland's IPC socket accepts any command, including starting programs (`dispatch exec`), and `:ro` doesn't limit that. So on Hyprland the sandbox doesn't protect you from the app: it can do anything you can do with `hyprctl`. That's the price of floating and pinning notes there. To keep the sandbox tight instead, run `flatpak override --user --nofilesystem=xdg-run/hypr ch.giosuel.RealerXpad`; notes then open as tiled windows on Hyprland.

## Submitting to Flathub

1. Fork [flathub/flathub](https://github.com/flathub/flathub) and branch from `new-pr`.
2. Add the Flathub manifest and the crate sources:

    ```sh
    packaging/flatpak/flathub-manifest.sh v0.1.1 > ../flathub/ch.giosuel.RealerXpad.yml
    cp packaging/flatpak/cargo-sources.json ../flathub/
    ```

3. Open a PR against `new-pr`, titled `Add ch.giosuel.RealerXpad`.
4. After it's accepted, Flathub creates `flathub/ch.giosuel.RealerXpad`. Update that
   repository for each release, with the same two commands.

Flathub checks that the app ID's domain belongs to you: the linter currently fails
with `appid-url-not-reachable`, because `https://giosuel.ch` doesn't answer.
