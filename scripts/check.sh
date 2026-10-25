#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --no-fail-fast

if [[ -n "${BACKSPN_NML:-}" || -n "${BACKSPN_TRANSIENTS:-}" ]]; then
    cargo test --workspace --release -- --ignored --nocapture
fi
