#!/usr/bin/env bash
# Renders background.html to background.png and background@2x.png with headless
# Chrome. Needs the site's node_modules for the fonts (cd site && pnpm install).
set -euo pipefail
cd "$(dirname "$0")"

chrome="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
[[ -f ../../../site/node_modules/@fontsource-variable/inter/files/inter-latin-wght-normal.woff2 ]] \
    || { echo "error: fonts missing, run pnpm install in site/" >&2; exit 1; }

for scale in 1 2; do
    out=background.png
    [[ $scale == 2 ]] && out=background@2x.png
    "$chrome" --headless --disable-gpu --hide-scrollbars --allow-file-access-from-files \
        --force-device-scale-factor=$scale --window-size=660,440 \
        --virtual-time-budget=2000 --screenshot="$PWD/$out" "file://$PWD/background.html" 2>/dev/null
    echo "wrote $out"
done
