# Nix

There are two ways to package the app for Nix:

- **`flake.nix`** (repository root) builds the current checkout. It provides
  `packages.{x86_64,aarch64}-linux.default`, an overlay and a dev shell. CI builds it on every PR.
- **`packaging/nix/package.nix`** is the package for
  [nixpkgs](https://github.com/NixOS/nixpkgs), building a release tag. The flake reuses it,
  swapping in the local source.

## Use the flake

```sh
nix run github:giosuel/realer-xpad                 # try it
nix profile install github:giosuel/realer-xpad     # install it
nix develop                                         # cargo, clippy, GTK headers
```

On NixOS or with home-manager, add the flake as an input and use
`inputs.realer-xpad.packages.${pkgs.system}.default`, or apply `overlays.default`.

## Submitting to nixpkgs

Not yet. nixpkgs only takes packages with a real user base ("if it's a niche of 5
people it's probably too small") that aren't "too immature", and its
[contributing guide](https://github.com/NixOS/nixpkgs/blob/master/CONTRIBUTING.md)
asks for AI-assisted work to be reviewed by a responsible person and disclosed with
an `Assisted-by:` commit trailer (a `Co-authored-by:` trailer doesn't count). Until
then, the flake above is the way to install it with Nix. When it's time:

1. Add yourself to `maintainers/maintainer-list.nix` in a separate commit, and list
   yourself in `meta.maintainers` (nixpkgs requires a maintainer for new packages).
2. Copy the package:

    ```sh
    mkdir -p pkgs/by-name/re/realer-xpad
    cp packaging/nix/package.nix pkgs/by-name/re/realer-xpad/package.nix
    ```

3. Check it builds: `nix-build -A realer-xpad`. To review it: `nixpkgs-review wip`.
4. Commit as `realer-xpad: init at <version>`, with an `Assisted-by:` trailer naming the AI tool and model, and open a PR against `master`. In the description, say what the app is and why it belongs in nixpkgs.

After that, nixpkgs' update bot (r-ryantm) opens version bumps on its own
(`passthru.updateScript`). To update this copy by hand, run
`nix-update --file packaging/nix/package.nix realer-xpad`, or change `version` and
fix the two hashes.
