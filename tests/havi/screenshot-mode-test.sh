#!/usr/bin/env bash
# shellcheck disable=SC1091,SC2034

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/test-prelude.bash"

TEST_NAME="screenshot-mode"

HAVI_DEVTOOLS_ENABLED=0
start_server
setup_acl u test

HPPR_SIGNER='ring1:ring0|init' "$HPPR" add //u/test/screenshot-mode.html \
    -H "Seal-By: oldest" -H "Content-Type: text/html" \
    <<< '<!doctype html><style>html,body{margin:0;background:#ffffff}.box{position:absolute;width:50px;height:50px}.red{left:0;top:0;background:#ff0000}.blue{left:100px;top:0;background:#0000ff}.green{left:0;top:100px;background:#008000}</style><div class="box red"></div><div class="box blue"></div><div class="box green"></div>'

havi_bin="$HAVI_ROOT/target/debug/havi"
if [[ ! -x "$havi_bin" ]]; then
    cargo build -q --manifest-path "$HAVI_ROOT/ports/havishell/Cargo.toml"
fi

SCREENSHOT="/tmp/havi-screenshot-mode-$$.png"
rm -f "$SCREENSHOT"
run_log=$(mktemp)
set +e
(
    cd "$HAVI_ROOT"
    HAVI_HOME="tcp+127.0.0.1:$HPPR_PORT" \
    HAVI_URL="hppr://u/test/screenshot-mode.html" \
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
import sys
from PIL import Image

path = sys.argv[1]
img = Image.open(path).convert('RGBA')
width, height = img.size
if width <= 150 or height <= 150:
    raise SystemExit(f"image too small: {img.size}")

# Screenshot mode must capture the webview only, not shell chrome.
chrome = (240, 245, 245)
for pixel in img.getdata():
    if pixel[:3] == chrome:
        raise SystemExit("shell chrome color leaked into screenshot")

checks = {
    (25, 25): (255, 0, 0),
    (125, 25): (0, 0, 255),
    (25, 125): (0, 128, 0),
    (80, 80): (255, 255, 255),
}
for point, expected in checks.items():
    actual = img.getpixel(point)[:3]
    if actual != expected:
        raise SystemExit(f"pixel {point} = {actual}, expected {expected}")
PY
rm -f "$SCREENSHOT" "$run_log"
log "PASS"
