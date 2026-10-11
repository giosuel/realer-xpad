#!/usr/bin/env bash
# Build Realer_Xpad-<version>-x86_64.AppImage with GTK 4 and libadwaita bundled.
#
#   tools/build-appimage.sh [OUT_DIR]      (default: dist/)
#
# Runs on Debian 13 (what CI uses). Needs: cargo, libgtk-4-dev, libadwaita-1-dev,
# librsvg2-common, adwaita-icon-theme, file, curl, binutils.
#
# The AppImage bundles GTK 4.18 and libadwaita 1.7, so it also runs on distros
# whose own GTK is too old (e.g. Ubuntu 24.04). The oldest glibc it supports is
# the highest version any bundled file needs; the build fails if that rises
# above MAX_GLIBC, so support for older distros can't silently regress.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="$(mkdir -p "${1:-$root/dist}" && cd "${1:-$root/dist}" && pwd)"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)
target="${CARGO_TARGET_DIR:-$root/target}"
work="$(mktemp -d)"
MAX_GLIBC="${MAX_GLIBC:-2.39}" # Ubuntu 24.04 LTS
trap 'rm -rf "$work"' EXIT

# Pinned tools, verified by checksum.
LINUXDEPLOY_URL=https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20251107-1/linuxdeploy-x86_64.AppImage
LINUXDEPLOY_SHA256=c20cd71e3a4e3b80c3483cef793cda3f4e990aca14014d23c544ca3ce1270b4d
GTK_PLUGIN_URL=https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/7a3fbc31a9e5075073ff8790f26effbac5f84453/linuxdeploy-plugin-gtk.sh
GTK_PLUGIN_SHA256=b0f4cbc684a0103a9651f0955b635eaea0096b3a66c0f5a2c2aa337960375171

fetch() { # url sha256 dest
  curl -fsSL "$1" -o "$3"
  echo "$2  $3" | sha256sum --check --quiet
  chmod +x "$3"
}
mkdir -p "$work/tools"
fetch "$LINUXDEPLOY_URL" "$LINUXDEPLOY_SHA256" "$work/tools/linuxdeploy"
fetch "$GTK_PLUGIN_URL" "$GTK_PLUGIN_SHA256" "$work/tools/linuxdeploy-plugin-gtk.sh"
export PATH="$work/tools:$PATH"
export APPIMAGE_EXTRACT_AND_RUN=1 # no FUSE needed (containers, CI)

echo "Building Realer Xpad $version (release)…"
cargo build --release --locked --manifest-path "$root/Cargo.toml"

appdir="$work/AppDir"
install -Dm755 "$target/release/realer-xpad" "$appdir/usr/bin/realer-xpad"
install -Dm644 "$root/extras/ch.giosuel.RealerXpad.desktop" \
  "$appdir/usr/share/applications/ch.giosuel.RealerXpad.desktop"
install -Dm644 "$root/extras/ch.giosuel.RealerXpad.metainfo.xml" \
  "$appdir/usr/share/metainfo/ch.giosuel.RealerXpad.metainfo.xml"
install -Dm644 "$root/data/icons/hicolor/scalable/apps/ch.giosuel.RealerXpad.svg" \
  "$appdir/usr/share/icons/hicolor/scalable/apps/ch.giosuel.RealerXpad.svg"

# Bundle Adwaita's symbolic icons: GTK only ships a few itself, and tiling
# setups like Hyprland often have no Adwaita icon theme installed.
mkdir -p "$appdir/usr/share/icons/Adwaita"
cp /usr/share/icons/Adwaita/index.theme "$appdir/usr/share/icons/Adwaita/"
cp -r /usr/share/icons/Adwaita/symbolic "$appdir/usr/share/icons/Adwaita/"

# 1) Collect the binary's libraries plus GTK 4's modules, loaders and settings.
DEPLOY_GTK_VERSION=4 linuxdeploy --appdir "$appdir" \
  --executable "$appdir/usr/bin/realer-xpad" \
  --desktop-file "$appdir/usr/share/applications/ch.giosuel.RealerXpad.desktop" \
  --icon-file "$appdir/usr/share/icons/hicolor/scalable/apps/ch.giosuel.RealerXpad.svg" \
  --plugin gtk

# 2) The GTK plugin's launch hook forces GDK_BACKEND=x11 (which would run us
#    under XWayland, or not at all without it) and pins GTK_THEME (which stops
#    libadwaita from following the system's light/dark preference). Hooks run in
#    alphabetical order: save the user's values before it, restore them after.
cat > "$appdir/apprun-hooks/00-realer-xpad-save-env.sh" <<'HOOK'
_rx_gdk_backend="${GDK_BACKEND-__unset__}"
_rx_gtk_theme="${GTK_THEME-__unset__}"
HOOK
cat > "$appdir/apprun-hooks/zz-realer-xpad-restore-env.sh" <<'HOOK'
if [ "$_rx_gdk_backend" = __unset__ ]; then unset GDK_BACKEND; else export GDK_BACKEND="$_rx_gdk_backend"; fi
if [ "$_rx_gtk_theme" = __unset__ ]; then unset GTK_THEME; else export GTK_THEME="$_rx_gtk_theme"; fi
unset _rx_gdk_backend _rx_gtk_theme
HOOK

# 2b) Guard the minimum glibc.
needed=$(find "$appdir" -type f -exec sh -c 'file -b "$1" | grep -q ELF && objdump -T "$1"' _ {} \; 2>/dev/null |
  grep -o 'GLIBC_2\.[0-9]*' | sed 's/GLIBC_//' | sort -uV | tail -1)
echo "Bundled files need glibc >= $needed (limit $MAX_GLIBC)"
if [[ "$(printf '%s\n' "$needed" "$MAX_GLIBC" | sort -V | tail -1)" != "$MAX_GLIBC" ]]; then
  echo "error: the AppImage would need glibc $needed, above MAX_GLIBC=$MAX_GLIBC" >&2
  exit 1
fi

# 3) Package. The update information lets AppImageUpdate fetch new releases.
export LDAI_OUTPUT="$out/Realer_Xpad-$version-x86_64.AppImage"
export LDAI_UPDATE_INFORMATION="gh-releases-zsync|giosuel|realer-xpad|latest|Realer_Xpad-*-x86_64.AppImage.zsync"
linuxdeploy --appdir "$appdir" --output appimage
[[ -f "$LDAI_OUTPUT.zsync" ]] || mv "$PWD"/Realer_Xpad-*.zsync "$out/" 2>/dev/null || true

ls -l "$out"/Realer_Xpad-"$version"-x86_64.AppImage*
