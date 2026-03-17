#!/usr/bin/env bash
# shellcheck disable=SC1091,SC2034

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/test-prelude.bash"

TEST_NAME="screenshot-mode"

HAVI_DEVTOOLS_ENABLED=0
start_server
setup_acl u test

HPPR_SIGNER='ring1:ring0#init' "$HPPR" add //u/test/screenshot-mode.html \
    -H "Seal-By: oldest" -H "Content-Type: text/html" \
    <<< '<!doctype html><style>html,body{margin:0;background:#ffffff}body{font:32px monospace}.box{width:160px;height:120px;background:#3366cc;margin:40px}</style><div class="box"></div>'

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
if width <= 100 or height <= 100:
    raise SystemExit(f"image too small: {img.size}")

# Screenshot mode must capture the webview only, not shell chrome.
chrome = (240, 245, 245)
for pixel in img.getdata():
    if pixel[:3] == chrome:
        raise SystemExit("shell chrome color leaked into screenshot")

# The HPPR fixture page is mostly white and should render real page content in
# the webview capture. Verify the page-shaped result directly.
white = (255, 255, 255)
white_count = 0
for pixel in img.getdata():
    if pixel[:3] == white:
        white_count += 1

total = width * height
if white_count < total * 0.90:
    raise SystemExit(f"expected mostly white page background, got {white_count}/{total} white pixels")

# The rendered document should not be empty: find the tight bounds of all
# non-white pixels and require a meaningful content region near the page origin.
xs = []
ys = []
for y in range(height):
    for x in range(width):
        if img.getpixel((x, y))[:3] != white:
            xs.append(x)
            ys.append(y)
if not xs:
    raise SystemExit("expected visible rendered content, got all-white screenshot")
min_x, max_x = min(xs), max(xs)
min_y, max_y = min(ys), max(ys)
content_width = max_x - min_x + 1
content_height = max_y - min_y + 1
if content_width < 120 or content_height < 90:
    raise SystemExit(f"content bounds too small: {content_width}x{content_height}")
if min_x > 80 or min_y > 80:
    raise SystemExit(f"content not near expected top-left area: ({min_x}, {min_y})")
PY
rm -f "$SCREENSHOT" "$run_log"
log "PASS"
