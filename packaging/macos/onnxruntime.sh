#!/usr/bin/env bash
# Usage: packaging/macos/onnxruntime.sh <dir>
#
# Puts the universal (Apple Silicon and Intel) ONNX Runtime library that stem
# separation loads on macOS in <dir>/libonnxruntime.dylib, checked by SHA-256.
# The release copies it into Backspn.app/Contents/Frameworks; for development,
# point it at target/debug or target/release.
set -euo pipefail

[[ $# -eq 1 ]] || { echo "usage: $0 <dir>" >&2; exit 1; }
dir="$1"

# The newest official build with an Intel slice. It runs on macOS 13.4 and
# later (MIN_MACOS in crates/separate/src/runtime.rs).
version=1.23.2
name="onnxruntime-osx-universal2-$version"
sha256=49ae8e3a66ccb18d98ad3fe7f5906b6d7887df8a5edd40f49eb2b14e20885809

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

curl -fsSL -o "$tmp/$name.tgz" \
    "https://github.com/microsoft/onnxruntime/releases/download/v$version/$name.tgz"
echo "$sha256  $tmp/$name.tgz" | shasum -a 256 -c - >/dev/null
tar -xzf "$tmp/$name.tgz" -C "$tmp"
mkdir -p "$dir"
cp "$tmp/$name/lib/libonnxruntime.$version.dylib" "$dir/libonnxruntime.dylib"
