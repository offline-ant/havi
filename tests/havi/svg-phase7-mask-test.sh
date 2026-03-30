#!/usr/bin/env bash
# svg-phase7-mask-test.sh - Phase 7 mask screenshot coverage
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="svg-phase7-mask"

HAVI_DEVTOOLS_ENABLED=0
start_server

FILE_PAGE="file://$(realpath "$SCRIPT_DIR/content/svg-phase7-mask.html")"
havi_bin="$HAVI_ROOT/target/debug/havi"

SCREENSHOT="/tmp/svg-phase7-mask-$$.png"
rm -f "$SCREENSHOT"
run_log=$(mktemp)
set +e
(
    cd "$HAVI_ROOT"
    HAVI_HOME="tcp+127.0.0.1:$HPPR_PORT" \
    HAVI_URL="$FILE_PAGE" \
    "$havi_bin" --screenshot "$SCREENSHOT"
) >"$run_log" 2>&1
status=$?
set -e

for _ in $(seq 1 50); do
    [[ -f "$SCREENSHOT" ]] && break
    sleep 0.1
done
[[ -f "$SCREENSHOT" ]] || {
    cat "$run_log" >&2
    fail "Screenshot file not created"
}
if [[ "$status" -ne 0 ]]; then
    cat "$run_log" >&2
    fail "Unexpected screenshot exit status: $status"
fi

python3 - "$SCREENSHOT" <<'PY'
from PIL import Image
import sys

img = Image.open(sys.argv[1]).convert('RGBA')
samples = {
    'bboxMaskVisible': ((50, 70), (0, 102, 255)),
    'bboxMaskHidden': ((130, 70), (255, 255, 255)),
}
for name, (point, expected) in samples.items():
    actual = img.getpixel(point)[:3]
    if any(abs(a - e) > 12 for a, e in zip(actual, expected)):
        raise SystemExit(f"{name}: pixel {point} = {actual}, expected {expected}")
PY

rm -f "$SCREENSHOT" "$run_log"
log "PASS"
