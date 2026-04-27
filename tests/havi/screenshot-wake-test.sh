#!/usr/bin/env bash
# shellcheck disable=SC1091,SC2034

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/test-prelude.bash"

TEST_NAME="screenshot-wake"

HAVI_DEVTOOLS_ENABLED=0
start_server
setup_acl u test

HPPR_SIGNER='ring1:ring0|init' "$HPPR" add //u/test/screenshot-wake.html \
    -H "Seal-By: ring0" -H "Content-Type: text/html" \
    <<< '<!doctype html><style>html,body{margin:0;background:#ffffff}.box{position:absolute;left:0;top:0;width:120px;height:120px;background:#008000}</style><div class="box"></div><script>setInterval(()=>{window.__tick=(window.__tick||0)+1},50)</script>'

havi_bin="$HAVI_BIN"

SCREENSHOT="/tmp/havi-screenshot-wake-$$.png"
rm -f "$SCREENSHOT"
run_log=$(mktemp)
start_time=$(python3 - <<'PY'
import time
print(time.monotonic())
PY
)
set +e
(
    cd "$HAVI_ROOT"
    HAVI_HOME="tcp+127.0.0.1:$HPPR_PORT" \
    HAVI_URL="hppr://u/test/screenshot-wake.html" \
    "$havi_bin" --screenshot "$SCREENSHOT"
) >"$run_log" 2>&1
status=$?
set -e
end_time=$(python3 - <<'PY'
import time
print(time.monotonic())
PY
)

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
python3 - "$SCREENSHOT" "$start_time" "$end_time" <<'PY'
import sys
from PIL import Image

path = sys.argv[1]
start = float(sys.argv[2])
end = float(sys.argv[3])
elapsed = end - start
if elapsed >= 4.0:
    raise SystemExit(f"screenshot took too long: {elapsed:.2f}s")

img = Image.open(path).convert('RGBA')
if img.size[0] <= 140 or img.size[1] <= 140:
    raise SystemExit(f"image too small: {img.size}")

checks = {
    (25, 25): (0, 128, 0),
    (130, 130): (255, 255, 255),
}
for point, expected in checks.items():
    actual = img.getpixel(point)[:3]
    if actual != expected:
        raise SystemExit(f"pixel {point} = {actual}, expected {expected}")
PY
rm -f "$SCREENSHOT" "$run_log"
log "PASS"
