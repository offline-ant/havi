#!/usr/bin/env bash
# shellcheck disable=SC1091

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/test-prelude.bash"

TEST_NAME="screenshot-surface-cache"

havi_bin="$HAVI_BIN"

html=$(mktemp /tmp/havi-surface-cache-XXXXXX.html)
cat >"$html" <<'HTML'
<!doctype html>
<html>
<head>
<style>
html, body { margin: 0; background: white; }
.box { position: absolute; top: 50px; left: 50px; width: 100px; height: 100px; background: green; }
.marker-h { position: absolute; top: 0; left: 0; width: 200px; height: 1px; background: red; }
.marker-v { position: absolute; top: 0; left: 0; width: 1px; height: 200px; background: red; }
.marker-50h { position: absolute; top: 50px; left: 0; width: 200px; height: 1px; background: blue; }
.marker-50v { position: absolute; top: 0; left: 50px; width: 1px; height: 200px; background: blue; }
</style>
</head>
<body>
<div class="marker-h"></div>
<div class="marker-v"></div>
<div class="marker-50h"></div>
<div class="marker-50v"></div>
<div class="box"></div>
</body>
</html>
HTML

SCREENSHOT="/tmp/havi-surface-cache-$$.png"
run_log=$(mktemp)
rm -f "$SCREENSHOT"

set +e
(
    cd "$HAVI_ROOT"
    HAVI_URL="file://$html" "$havi_bin" --no-pylon --screenshot "$SCREENSHOT"
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
import sys
from PIL import Image

img = Image.open(sys.argv[1]).convert('RGBA')
checks = {
    (0, 0): (255, 0, 0),
    (1, 50): (0, 0, 255),
    (50, 1): (0, 0, 255),
    (75, 75): (0, 128, 0),
    (250, 250): (255, 255, 255),
}
for point, expected in checks.items():
    actual = img.getpixel(point)[:3]
    if actual != expected:
        raise SystemExit(f"pixel {point} = {actual}, expected {expected}")
PY

rm -f "$SCREENSHOT" "$run_log" "$html"
log "PASS"
