#!/usr/bin/env bash
# Usage: packaging/manifest.sh <version> <packages dir>
#
# Writes backspn-manifest.json into the packages dir: the version and, for each
# package the app can update itself from, its platform, arch, kind, size and
# SHA-256. Package names follow Backspn_<version>_<platform>_<arch>.<ext>. The
# release signs the manifest with minisign; the app checks that signature.
set -euo pipefail

[[ $# -eq 2 ]] || { echo "usage: $0 <version> <dir>" >&2; exit 1; }
version="$1"
dir="$2"

entries=()
for file in "$dir"/Backspn_"$version"_*; do
    name="$(basename "$file")"
    case "$name" in
        *.app.tar.gz) kind=app; ext=.app.tar.gz ;;
        *.exe) kind=nsis; ext=.exe ;;
        *.AppImage) kind=appimage; ext=.AppImage ;;
        *.deb) kind=deb; ext=.deb ;;
        *) continue ;;
    esac
    rest="${name#Backspn_"$version"_}"
    rest="${rest%"$ext"}"
    platform="${rest%%_*}"
    arch="${rest#*_}"
    size=$(wc -c < "$file" | tr -d ' ')
    sha=$(sha256sum "$file" | cut -d' ' -f1)
    entries+=("    { \"platform\": \"$platform\", \"arch\": \"$arch\", \"kind\": \"$kind\", \"name\": \"$name\", \"size\": $size, \"sha256\": \"$sha\" }")
done
[[ ${#entries[@]} -gt 0 ]] || { echo "no packages for $version in $dir" >&2; exit 1; }

{
    printf '{\n  "version": "%s",\n  "assets": [\n' "$version"
    for i in "${!entries[@]}"; do
        sep=","
        [[ $i -eq $((${#entries[@]} - 1)) ]] && sep=""
        printf '%s%s\n' "${entries[$i]}" "$sep"
    done
    printf '  ]\n}\n'
} > "$dir/backspn-manifest.json"
