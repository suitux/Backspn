#!/usr/bin/env bash
# Usage: packaging/linux/onnxruntime.sh <dir>
#
# Puts the ONNX Runtime library that stem separation loads on Linux in
# <dir>/libonnxruntime.so, checked by SHA-256. The packages install it in
# /usr/lib/Backspn (crates/app/Cargo.toml, `resources`); for development,
# point it at target/debug or target/release.
set -euo pipefail

[[ $# -eq 1 ]] || { echo "usage: $0 <dir>" >&2; exit 1; }
dir="$1"

# Microsoft's official build: it picks the fastest code for the processor
# while it runs, so it also works on processors without AVX2.
version=1.23.2
name="onnxruntime-linux-x64-$version"
sha256=1fa4dcaef22f6f7d5cd81b28c2800414350c10116f5fdd46a2160082551c5f9b

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

curl -fsSL -o "$tmp/$name.tgz" \
    "https://github.com/microsoft/onnxruntime/releases/download/v$version/$name.tgz"
echo "$sha256  $tmp/$name.tgz" | sha256sum -c - >/dev/null
tar -xzf "$tmp/$name.tgz" -C "$tmp"
mkdir -p "$dir"
cp "$tmp/$name/lib/libonnxruntime.so.$version" "$dir/libonnxruntime.so"
