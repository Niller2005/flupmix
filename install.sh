#!/usr/bin/env bash
# Installs flupmix, its desktop entry and icon.
#   ./install.sh             install under ~/.local (or $PREFIX)
#   ./install.sh uninstall   remove it again
# Uses a prebuilt ./flupmix next to this script (release tarball) when present,
# otherwise builds from source with cargo.
set -euo pipefail

PREFIX="${PREFIX:-$HOME/.local}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bin="$PREFIX/bin/flupmix"
desktop="$PREFIX/share/applications/flupmix.desktop"
icon="$PREFIX/share/icons/hicolor/scalable/apps/flupmix.svg"

refresh_caches() {
  command -v update-desktop-database >/dev/null && update-desktop-database "$PREFIX/share/applications" 2>/dev/null || true
  command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$PREFIX/share/icons/hicolor" 2>/dev/null || true
}

if [[ "${1:-}" == "uninstall" ]]; then
  rm -f "$bin" "$desktop" "$icon"
  refresh_caches
  echo "flupmix removed from $PREFIX"
  exit 0
fi

if [[ -x "$here/flupmix" ]]; then
  src="$here/flupmix"
else
  (cd "$here" && cargo build --release -p flupmix)
  src="$here/target/release/flupmix"
fi

install -Dm755 "$src" "$bin"
install -Dm644 "$here/assets/flupmix.svg" "$icon"
# Absolute Exec so the launcher works even when $PREFIX/bin is not on the menu's PATH.
install -d "$(dirname "$desktop")"
sed "s|^Exec=.*|Exec=$bin|" "$here/assets/flupmix.desktop" > "$desktop"
chmod 644 "$desktop"
refresh_caches
echo "flupmix installed to $PREFIX (launch it from your app menu or run: $bin)"
