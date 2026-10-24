#!/usr/bin/env bash
# Builds FastDisco and installs it for the current user: the binary in
# ~/.local/bin, the launcher entry and the icon, so the desktop's app
# launcher, notifications and autostart find it.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release
prefix="${PREFIX:-$HOME/.local}"
install -Dm755 target/release/fastdisco "$prefix/bin/fastdisco"
install -Dm644 contrib/fastdisco.desktop "$prefix/share/applications/fastdisco.desktop"
for size in 32 64 128 256 512; do
    dir="$prefix/share/icons/hicolor/${size}x${size}/apps"
    mkdir -p "$dir"
    "$prefix/bin/fastdisco" write-icon "$dir/fastdisco.png" "$size"
done
command -v update-desktop-database >/dev/null && update-desktop-database "$prefix/share/applications" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q "$prefix/share/icons/hicolor" || true
echo "FastDisco installed in $prefix/bin/fastdisco"
