#!/usr/bin/env bash
# Packs a built Discofast into Discofast.app and a disk image for macOS:
# scripts/macos-app.sh <discofast binary> <version> <output.dmg>.
set -euo pipefail
binary=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
version=$2
output=$3
root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
app="$work/Discofast.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/discofast"
sed "s/VERSION/$version/g" "$root/packaging/macos/Info.plist" > "$app/Contents/Info.plist"
iconset="$work/discofast.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    "$binary" write-icon "$iconset/icon_${size}x${size}.png" "$size"
    "$binary" write-icon "$iconset/icon_${size}x${size}@2x.png" $((size * 2))
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/discofast.icns"
# Signed ad hoc, so Apple Silicon runs it; Gatekeeper still asks once.
codesign --force --deep --sign - "$app"
ln -s /Applications "$work/Applications"
rm -rf "$iconset"
hdiutil create -volname Discofast -srcfolder "$work" -ov -format UDZO "$output"
