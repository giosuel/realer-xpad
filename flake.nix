{
  description = "Realer Xpad: sticky notes with Markdown and drawing for Wayland and Hyprland";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

      # The nixpkgs package (packaging/nix/package.nix), built from this checkout
      # instead of a release tag.
      realer-xpad =
        pkgs:
        (pkgs.callPackage ./packaging/nix/package.nix { }).overrideAttrs {
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
          src = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              ./build.rs
              ./src
              ./data
              ./extras
              ./tests
              ./examples
            ];
          };
          cargoDeps = pkgs.rustPlatform.importCargoLock { lockFile = ./Cargo.lock; };
        };
    in
    {
      packages = forAllSystems (pkgs: {
        realer-xpad = realer-xpad pkgs;
        default = self.packages.${pkgs.stdenv.hostPlatform.system}.realer-xpad;
      });

      overlays.default = final: _prev: { realer-xpad = realer-xpad final; };

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${pkgs.stdenv.hostPlatform.system}.realer-xpad ];
          packages = with pkgs; [
            clippy
            rustfmt
            rust-analyzer
          ];
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
