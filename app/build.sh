#!/usr/bin/env bash
set -euo pipefail
app_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
export CARGO_HOME="$app_dir/build/cargo-home"
export CARGO_TARGET_DIR="$app_dir/build/target"
cargo build --locked --manifest-path "$app_dir/Cargo.toml" --release
install -Dm755 "$CARGO_TARGET_DIR/release/keyboard-bridge" "$app_dir/bin/keyboard-bridge"
