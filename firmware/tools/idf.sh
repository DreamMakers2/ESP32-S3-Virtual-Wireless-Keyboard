#!/usr/bin/env bash
# Use only the approved project-local ESP-IDF/tool installation.
set -euo pipefail
project_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
export IDF_TOOLS_PATH="$project_root/.tools/espressif"
export PIP_CACHE_DIR="$project_root/.tools/pip-cache"
export CCACHE_DIR="$project_root/.tools/ccache"
export PATH="$project_root/.tools/python/bin:$PATH"
if [[ ! -f "$project_root/.tools/esp-idf/export.sh" ]]; then
    echo 'Project-local ESP-IDF missing; see firmware/README.md.' >&2
    exit 1
fi
source "$project_root/.tools/esp-idf/export.sh" >/dev/null
exec idf.py "$@"
