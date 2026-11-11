#!/usr/bin/env bash
# Usage: packaging/windows/runtime.sh <dir>
#
# Puts the DLLs the installer ships next to Backspn.exe in <dir>
# (crates/app/Cargo.toml, `resources`):
#
# - onnxruntime.dll, which stem separation loads (packaging/windows/onnxruntime.sh).
# - The Visual C++ runtime, so the app does not depend on the version the user
#   has installed.
#
# Run it on the Windows runner.
set -euo pipefail

[[ $# -eq 1 ]] || { echo "usage: $0 <dir>" >&2; exit 1; }
dir="$1"
here="$(cd "$(dirname "$0")" && pwd)"

"$here/onnxruntime.sh" "$dir"

vswhere="/c/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe"
vs="$("$vswhere" -latest -products '*' -property installationPath | tr -d '\r')"
crt="$(ls -d "$(cygpath -u "$vs")"/VC/Redist/MSVC/*/x64/Microsoft.VC14*.CRT | sort -V | tail -n 1)"
[[ -d "$crt" ]] || { echo "missing the Visual C++ runtime in $vs" >&2; exit 1; }

mkdir -p "$dir"
cp "$crt"/*.dll "$dir/"
ls -l "$dir"
