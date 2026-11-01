#!/usr/bin/env bash
# Usage: packaging/macos/universal.sh <Backspn.app>
#
# Turns the Apple Silicon app built by cargo-packager into a universal one: adds
# the Intel build of the binary and the stem separation runtime. Run it before
# signing. Needs `rustup target add x86_64-apple-darwin`.
set -euo pipefail

[[ $# -eq 1 ]] || { echo "usage: $0 <app>" >&2; exit 1; }
app="$1"
here="$(cd "$(dirname "$0")" && pwd)"
repo="$here/../.."

cargo build --release -p backspn-app --target x86_64-apple-darwin \
    --manifest-path "$repo/Cargo.toml"

bin="$app/Contents/MacOS/Backspn"
lipo -create "$bin" "$repo/target/x86_64-apple-darwin/release/Backspn" -output "$bin.universal"
mv "$bin.universal" "$bin"

"$here/onnxruntime.sh" "$app/Contents/Frameworks"
