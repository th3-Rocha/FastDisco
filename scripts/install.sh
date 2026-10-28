#!/usr/bin/env bash
# Builds Discofast and installs it for the current user: the binary in
# ~/.local/bin, the launcher entry and the icon, so the desktop's app
# launcher, notifications and autostart find it.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release
prefix="${PREFIX:-$HOME/.local}"
install -Dm755 target/release/discofast "$prefix/bin/discofast"
install -Dm644 contrib/discofast.desktop "$prefix/share/applications/discofast.desktop"
for size in 32 64 128 256 512; do
    dir="$prefix/share/icons/hicolor/${size}x${size}/apps"
    mkdir -p "$dir"
    "$prefix/bin/discofast" write-icon "$dir/discofast.png" "$size"
done
command -v update-desktop-database >/dev/null && update-desktop-database "$prefix/share/applications" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q "$prefix/share/icons/hicolor" || true
echo "Discofast installed in $prefix/bin/discofast"
