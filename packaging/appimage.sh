#!/usr/bin/env bash
# Builds flupmix-<version>-x86_64.AppImage from target/release/flupmix.
# Usage: packaging/appimage.sh <version> [output-dir]
# The AppImage bundles the binary only; PipeWire, Wayland/X11, Vulkan/GL and
# font libraries come from the host system, as for any desktop audio app.
set -euo pipefail

version="${1:?usage: appimage.sh <version> [output-dir]}"
out="${2:-dist}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
appdir="$work/flupmix.AppDir"

install -Dm755 "$root/target/release/flupmix" "$appdir/usr/bin/flupmix"
install -Dm644 "$root/assets/flupmix.svg" "$appdir/flupmix.svg"
install -Dm644 "$root/assets/flupmix.svg" "$appdir/usr/share/icons/hicolor/scalable/apps/flupmix.svg"
install -Dm644 "$root/assets/flupmix.desktop" "$appdir/flupmix.desktop"
install -Dm644 "$root/assets/flupmix.desktop" "$appdir/usr/share/applications/flupmix.desktop"
ln -s flupmix.svg "$appdir/.DirIcon"
cat > "$appdir/AppRun" <<'RUN'
#!/bin/sh
here="$(dirname "$(readlink -f "$0")")"
exec "$here/usr/bin/flupmix" "$@"
RUN
chmod +x "$appdir/AppRun"

tool="${APPIMAGETOOL:-}"
if [ -z "$tool" ]; then
  tool="$work/appimagetool"
  curl -fsSL -o "$tool" \
    https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
  chmod +x "$tool"
fi

mkdir -p "$out"
ARCH=x86_64 "$tool" --appimage-extract-and-run --no-appstream "$appdir" \
  "$out/flupmix-$version-x86_64.AppImage"
