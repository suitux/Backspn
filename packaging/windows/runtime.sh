#!/usr/bin/env bash
# Usage: packaging/windows/runtime.sh <dir>
#
# Puts the DLLs that Backspn.exe loads next to itself on Windows in <dir>, so
# the installer ships them beside the executable (crates/app/Cargo.toml,
# `resources`):
#
# - DirectML.dll, the one stem separation is linked against. The copy inside
#   Windows 10 is older and keeps the app from starting.
# - The Visual C++ runtime, so the app does not depend on the version the user
#   has installed.
#
# Run it on the Windows runner after `cargo build --release -p backspn-app`.
set -euo pipefail

[[ $# -eq 1 ]] || { echo "usage: $0 <dir>" >&2; exit 1; }
dir="$1"
here="$(cd "$(dirname "$0")" && pwd)"
repo="$here/../.."

directml="$repo/target/release/DirectML.dll"
[[ -f "$directml" ]] || { echo "missing $directml" >&2; exit 1; }

vswhere="/c/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe"
vs="$("$vswhere" -latest -products '*' -property installationPath | tr -d '\r')"
crt="$(ls -d "$(cygpath -u "$vs")"/VC/Redist/MSVC/*/x64/Microsoft.VC14*.CRT | sort -V | tail -n 1)"
[[ -d "$crt" ]] || { echo "missing the Visual C++ runtime in $vs" >&2; exit 1; }

mkdir -p "$dir"
cp "$directml" "$dir/"
cp "$crt"/*.dll "$dir/"
ls -l "$dir"
