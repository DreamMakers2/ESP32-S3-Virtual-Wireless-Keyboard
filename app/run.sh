#!/usr/bin/env bash
set -euo pipefail
app_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
binary="$app_dir/bin/keyboard-bridge"
if [[ ! -x "$binary" ]]; then
  echo "keyboard-bridge is not packaged. Run: ./build.sh" >&2
  exit 1
fi
exec "$binary" "$@"
