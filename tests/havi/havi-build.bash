#!/usr/bin/env bash
# havi-build.bash - Shared HAVI test build helper
# shellcheck disable=SC2034

set -euo pipefail

if [[ -z "${SCRIPT_DIR:-}" ]]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fi

if [[ -n "${FORGE_ROOT:-}" ]]; then
    HAVI_ROOT="${HAVI_ROOT:-$FORGE_ROOT/havi}"
else
    HAVI_ROOT="${HAVI_ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
fi

HAVI_BIN="${HAVI_BIN:-$HAVI_ROOT/target/debug/havi}"
HAVI_BUILD_READY="${HAVI_BUILD_READY:-}"

ensure_havi_built() {
    [[ -n "$HAVI_BUILD_READY" ]] && return 0
    echo "[${TEST_NAME:-test}] Building HAVI via ./mach-havi build..." >&2
    (
        cd "$HAVI_ROOT"
        ./mach-havi build >/dev/null
    )
    HAVI_BUILD_READY=1
}
