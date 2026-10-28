#!/usr/bin/env bash
# Packs a built Discofast into an AppImage for Ubuntu and other Linux
# desktops: scripts/appimage.sh <discofast binary> <output.AppImage>.
# Wayland, X11 and OpenGL come from the system, as for any desktop app;
# the binary itself only needs ALSA and glibc.
set -euo pipefail
binary=$(realpath "$1")
output=$(realpath -m "$2")
root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
appdir="$work/AppDir"
install -Dm755 "$binary" "$appdir/usr/bin/discofast"
install -Dm644 "$root/contrib/discofast.desktop" "$appdir/usr/share/applications/discofast.desktop"
cp "$root/contrib/discofast.desktop" "$appdir/discofast.desktop"
for size in 32 64 128 256 512; do
    dir="$appdir/usr/share/icons/hicolor/${size}x${size}/apps"
    mkdir -p "$dir"
    "$binary" write-icon "$dir/discofast.png" "$size"
done
cp "$appdir/usr/share/icons/hicolor/256x256/apps/discofast.png" "$appdir/discofast.png"
ln -s usr/bin/discofast "$appdir/AppRun"
tool="$work/appimagetool"
curl -fsSL -o "$tool" https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage
chmod +x "$tool"
# Extracting instead of mounting: CI machines have no FUSE.
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$tool" --no-appstream "$appdir" "$output"
