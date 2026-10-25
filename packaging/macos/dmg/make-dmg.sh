#!/usr/bin/env bash
# Usage: packaging/macos/dmg/make-dmg.sh <Backspn.app> <out.dmg>
#
# Builds the drag-to-Applications disk image with create-dmg
# (brew install create-dmg). It lays out the window through Finder, so it needs
# a logged-in session: macOS 26 Finder ignores backgrounds in a .DS_Store
# written by other tools.
set -euo pipefail

[[ $# -eq 2 ]] || { echo "usage: $0 <app> <out.dmg>" >&2; exit 1; }
app="$1"
out="$2"
here="$(cd "$(dirname "$0")" && pwd)"
repo="$here/../../.."

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# One TIFF holding both resolutions, so Retina screens get the @2x image.
tiffutil -cathidpicheck "$here/background.png" "$here/background@2x.png" -out "$tmp/background.tiff" >/dev/null
mkdir "$tmp/src"
cp -R "$app" "$tmp/src/"
name="$(basename "$app")"
rm -f "$out"

# The window is 68 points taller than the image: 400 points of it show under
# macOS 26's title and tab bars, 440 under the plain title bar of older versions.
"${CREATE_DMG:-create-dmg}" \
    --volname Backspn \
    --volicon "$repo/crates/app/icon/backspn.icns" \
    --background "$tmp/background.tiff" \
    --window-pos 200 120 \
    --window-size 660 468 \
    --icon-size 112 \
    --text-size 13 \
    --icon "$name" 170 196 \
    --hide-extension "$name" \
    --app-drop-link 490 196 \
    --no-internet-enable \
    "$out" "$tmp/src"
