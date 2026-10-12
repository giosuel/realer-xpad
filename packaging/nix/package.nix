{
  lib,
  rustPlatform,
  fetchFromGitHub,
  pkg-config,
  wrapGAppsHook4,
  glib,
  gtk4,
  libadwaita,
  nix-update-script,
}:

rustPlatform.buildRustPackage (finalAttrs: {
  pname = "realer-xpad";
  version = "0.1.1";

  src = fetchFromGitHub {
    owner = "giosuel";
    repo = "realer-xpad";
    tag = "v${finalAttrs.version}";
    hash = "sha256-O/SJ8BaAeqg/mUad5tUxQMdZIpZsNlpeOUMXbkzKAg0=";
  };

  cargoHash = "sha256-T3XSuqd0H0ftKIcvcwWRLcG5MN8BclNsUFMdqVZ+cEM=";

  nativeBuildInputs = [
    pkg-config
    wrapGAppsHook4
    glib # glib-compile-resources, for the bundled stylesheet and icons
  ];

  buildInputs = [
    gtk4
    libadwaita
  ];

  # The UI tests need a Wayland compositor; run the unit tests only.
  cargoTestFlags = [ "--lib" ];

  postInstall = ''
    install -Dm644 extras/ch.giosuel.RealerXpad.desktop -t $out/share/applications
    install -Dm644 extras/ch.giosuel.RealerXpad.metainfo.xml -t $out/share/metainfo
    install -Dm644 data/icons/hicolor/scalable/apps/ch.giosuel.RealerXpad.svg \
      -t $out/share/icons/hicolor/scalable/apps
  '';

  passthru.updateScript = nix-update-script { };

  meta = {
    description = "Sticky notes with Markdown and drawing, native on Wayland and Hyprland";
    homepage = "https://giosuel.github.io/realer-xpad/";
    changelog = "https://github.com/giosuel/realer-xpad/releases/tag/v${finalAttrs.version}";
    license = lib.licenses.agpl3Only;
    maintainers = [ ];
    mainProgram = "realer-xpad";
    platforms = lib.platforms.linux;
  };
})
