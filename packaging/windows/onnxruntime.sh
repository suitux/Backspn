#!/usr/bin/env bash
# Usage: packaging/windows/onnxruntime.sh <dir>
#
# Puts the ONNX Runtime library that stem separation loads on Windows in
# <dir>/onnxruntime.dll, checked by SHA-256. The installer ships it next to
# Backspn.exe (packaging/windows/runtime.sh); for development, point it at
# target/debug or target/release.
set -euo pipefail

[[ $# -eq 1 ]] || { echo "usage: $0 <dir>" >&2; exit 1; }
dir="$1"

# Microsoft's official build: it picks the fastest code for the processor
# while it runs, so it also works on processors without AVX2.
version=1.23.2
name="onnxruntime-win-x64-$version"
sha256=0b38df9af21834e41e73d602d90db5cb06dbd1ca618948b8f1d66d607ac9f3cd

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

curl -fsSL -o "$tmp/$name.zip" \
    "https://github.com/microsoft/onnxruntime/releases/download/v$version/$name.zip"
echo "$sha256  $tmp/$name.zip" | sha256sum -c - >/dev/null
unzip -q "$tmp/$name.zip" -d "$tmp"
mkdir -p "$dir"
cp "$tmp/$name/lib/onnxruntime.dll" "$dir/onnxruntime.dll"
