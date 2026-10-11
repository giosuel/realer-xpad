---
title: Contributing
---

# Contributing

Bug reports, ideas and pull requests are welcome on
[GitHub](https://github.com/giosuel/realer-xpad).

## Development setup

You need Rust 1.92+ and the GTK development files (see
[Install → build from source](install.md#option-b-build-from-source)).

```sh
git clone https://github.com/giosuel/realer-xpad && cd realer-xpad
cargo run                     # run from the checkout
cargo run -- new              # any CLI command works after --
```

!!! tip
    While hacking, point the app at a scratch folder so your real notes stay
    safe: `REALER_XPAD_DATA=/tmp/notes cargo run`.

## Tests

```sh
cargo test                                  # unit tests; UI tests skip without a display
tools/headless.sh cargo test --test ui      # UI tests, inside a throwaway headless sway
cargo clippy --all-targets -- -D warnings   # lint
cargo fmt
```

`tools/headless.sh` starts a pure-Wayland sway with no Xwayland, so the UI
tests also check that nothing depends on X11. It needs `sway` installed. GTK
must stay on a single thread, so `tests/ui.rs` uses its own small test harness
instead of libtest. CI runs everything on Debian and Fedora.

To regenerate the screenshots used in the docs:

```sh
tools/headless.sh cargo run --example screenshot -- docs
tools/headless.sh cargo run --example screenshot -- docs --dark
```

## How it's put together

| Module | Responsibility |
|---|---|
| `src/markdown.rs` | Parses Markdown into style spans and markers; renders the read view. No GTK. |
| `src/editor.rs` | The live-styled edit view and the read view. |
| `src/drawing.rs` | Stroke model with erase and undo/redo. No GTK. |
| `src/canvas.rs` | The drawing widget. |
| `src/note_window.rs` | A sticky note: header, editor, canvas, toolbar. |
| `src/manager.rs` | The notes manager window. |
| `src/app.rs` | Single-instance app, CLI commands, note lifecycle. |
| `src/hypr.rs` | Hyprland IPC: find, place, pin windows. |
| `src/store.rs` | `.md` + `.json` persistence and the trash. |
| `src/theme.rs`, `data/` | Note palette, stylesheet and icons (compiled in as GResources). |

The Markdown parser, drawing model, storage and Hyprland IPC don't depend on
GTK, so most logic is tested with plain `cargo test`.

## Docs

This site is built with [MkDocs Material](https://squidfunk.github.io/mkdocs-material/):

```sh
python3 -m venv .venv && .venv/bin/pip install -r requirements-docs.txt
.venv/bin/mkdocs serve
```

## Releasing

Bump `version` in `Cargo.toml`, then push a matching tag:

```sh
git tag v0.2.0 && git push origin v0.2.0
```

CI checks that the tag matches the version, builds a release binary and
publishes a GitHub release with a ready-to-install tarball.
