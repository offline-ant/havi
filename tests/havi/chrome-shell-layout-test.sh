#!/usr/bin/env bash
# chrome-shell-layout-test.sh - Verify the shell top chrome starts at y=0 and tabs fill the tab strip.

set -euo pipefail

TEST_NAME="chrome-shell-layout"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HAVI_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

log() { echo "[$TEST_NAME] $*" >&2; }
fail() { echo "FAIL: $*" >&2; exit 1; }

RUN_PID=""
RUN_LOG=""
HTML_FILE=""
DUMP_FILE=""
SHOT_FILE=""
HAVI_CONFIG_DIR=""

stop_pid() {
    local pid="$1"
    [[ -z "$pid" ]] && return
    if kill -0 "$pid" 2>/dev/null; then
        kill -- "-$pid" 2>/dev/null || true
        kill "$pid" 2>/dev/null || true
        for _ in {1..50}; do
            kill -0 "$pid" 2>/dev/null || break
            sleep 0.1
        done
        if kill -0 "$pid" 2>/dev/null; then
            kill -9 -- "-$pid" 2>/dev/null || true
            kill -9 "$pid" 2>/dev/null || true
        fi
    fi
}

cleanup() {
    local exit_code=$?
    stop_pid "$RUN_PID"
    [[ -n "$HTML_FILE" && -f "$HTML_FILE" ]] && rm -f "$HTML_FILE"
    [[ -n "$DUMP_FILE" && -f "$DUMP_FILE" ]] && rm -f "$DUMP_FILE"
    [[ -n "$SHOT_FILE" && -f "$SHOT_FILE" ]] && rm -f "$SHOT_FILE"
    [[ -n "$RUN_LOG" && -f "$RUN_LOG" ]] && rm -f "$RUN_LOG"
    [[ -n "$HAVI_CONFIG_DIR" && -d "$HAVI_CONFIG_DIR" ]] && rm -rf "$HAVI_CONFIG_DIR"
    exit $exit_code
}
trap cleanup EXIT INT TERM

log "Building HAVI via ./mach-havi build..."
(
    cd "$HAVI_ROOT"
    ./mach-havi build >/dev/null
)

HTML_FILE="$(mktemp /tmp/havi-shell-layout-XXXXXX.html)"
cat >"$HTML_FILE" <<'HTML'
<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <title>Shell Layout Test</title>
  <style>
    html, body {
      margin: 0;
      width: 100%;
      height: 100%;
      background: #224488;
    }
  </style>
</head>
<body></body>
</html>
HTML

RUN_LOG="$(mktemp /tmp/havi-shell-layout-run-XXXXXX.log)"
DUMP_FILE="$(mktemp /tmp/havi-shell-layout-dump-XXXXXX.txt)"
SHOT_FILE="$(mktemp /tmp/havi-shell-layout-shot-XXXXXX.png)"
HAVI_CONFIG_DIR="$(mktemp -d /tmp/havi-shell-layout-config-XXXXXX)"

log "Starting HAVI with Makepad relay and --no-pylon..."
(
    cd "$HAVI_ROOT"
    setsid env \
        HAVI_CONFIG="$HAVI_CONFIG_DIR" \
        HAVI_URL="file://$HTML_FILE" \
        python3 -u - <<'PY'
import os
import pathlib
import sys

havi_root = pathlib.Path.cwd()
sys.path.insert(0, str(havi_root / "ports" / "havishell"))

from mach_havi_studio import run_desktop_makepad_socket

cmd = [str(havi_root / "target" / "debug" / "havi"), "--foreground", "--no-pylon"]
raise SystemExit(run_desktop_makepad_socket(cmd, os.environ.copy(), havi_root))
PY
) >"$RUN_LOG" 2>&1 &
RUN_PID=$!

SOCKET_PATH=""
for _ in {1..600}; do
    if [[ -f "$RUN_LOG" ]]; then
        SOCKET_PATH=$(grep '^HAVI_MAKEPAD_SOCKET=' "$RUN_LOG" | sed -n '1s/^HAVI_MAKEPAD_SOCKET=//p') || true
        if [[ -n "$SOCKET_PATH" ]]; then
            break
        fi
        if ! kill -0 "$RUN_PID" 2>/dev/null; then
            cat "$RUN_LOG" >&2 || true
            fail "HAVI exited before exposing HAVI_MAKEPAD_SOCKET"
        fi
    fi
    sleep 0.1
done

[[ -n "$SOCKET_PATH" ]] || {
    cat "$RUN_LOG" >&2 || true
    fail "Timed out waiting for HAVI_MAKEPAD_SOCKET"
}

log "Waiting for Makepad socket at $SOCKET_PATH..."
for _ in {1..200}; do
    [[ -S "$SOCKET_PATH" ]] && break
    if ! kill -0 "$RUN_PID" 2>/dev/null; then
        cat "$RUN_LOG" >&2 || true
        fail "HAVI exited before socket became available"
    fi
    sleep 0.1
done
[[ -S "$SOCKET_PATH" ]] || fail "Makepad socket was not created"

log "Waiting for stable widget dump..."
for _ in {1..300}; do
    if "$HAVI_ROOT/havi-makepad-cli" --socket "$SOCKET_PATH" dump >"$DUMP_FILE" 2>/dev/null; then
        if grep -q ' tab_bar_wrap ' "$DUMP_FILE" && grep -q ' tab_bar ' "$DUMP_FILE"; then
            break
        fi
    fi
    if ! kill -0 "$RUN_PID" 2>/dev/null; then
        cat "$RUN_LOG" >&2 || true
        fail "HAVI exited before widget dump was ready"
    fi
    sleep 0.1
done

grep -q ' tab_bar_wrap ' "$DUMP_FILE" || {
    cat "$RUN_LOG" >&2 || true
    cat "$DUMP_FILE" >&2 || true
    fail "Widget dump does not contain tab_bar_wrap"
}
grep -q ' tab_bar ' "$DUMP_FILE" || {
    cat "$RUN_LOG" >&2 || true
    cat "$DUMP_FILE" >&2 || true
    fail "Widget dump does not contain tab_bar"
}

"$HAVI_ROOT/havi-makepad-cli" --socket "$SOCKET_PATH" screenshot "$SHOT_FILE" >/dev/null

python3 - "$DUMP_FILE" "$SHOT_FILE" <<'PY'
import math
import sys
from dataclasses import dataclass
from PIL import Image

dump_path = sys.argv[1]
shot_path = sys.argv[2]

@dataclass
class Widget:
    index: int
    parent: int
    name: str
    kind: str
    x: float
    y: float
    w: float
    h: float

widgets = []
with open(dump_path, 'r', encoding='utf-8') as fh:
    for line in fh:
        parts = line.split()
        if len(parts) < 8 or parts[0] in {'O', 'W'}:
            continue
        try:
            widgets.append(
                Widget(
                    index=int(parts[0]),
                    parent=int(parts[1]),
                    name=parts[2],
                    kind=parts[3],
                    x=float(parts[4]),
                    y=float(parts[5]),
                    w=float(parts[6]),
                    h=float(parts[7]),
                )
            )
        except ValueError:
            continue

by_name = {widget.name: widget for widget in widgets}
wrap = by_name.get('tab_bar_wrap')
tab_bar = by_name.get('tab_bar')
if wrap is None:
    raise SystemExit('missing tab_bar_wrap in widget dump')
if tab_bar is None:
    raise SystemExit('missing tab_bar in widget dump')

img = Image.open(shot_path).convert('RGBA')
top_border_x = min(int(tab_bar.x + 60), img.width - 1)
top_border_y = min(max(int(round(tab_bar.y)), 0), img.height - 1)
top_border_pixel = img.getpixel((top_border_x, top_border_y))[:3]

print(f'tab_bar_wrap y={wrap.y:.2f} h={wrap.h:.2f}')
print(f'tab_bar y={tab_bar.y:.2f} h={tab_bar.h:.2f}')
print(f'top_border sample=({top_border_x},{top_border_y}) pixel={top_border_pixel}')

if wrap.y > 1.0:
    raise SystemExit(f'tab_bar_wrap starts below top edge: y={wrap.y:.2f}')
if math.fabs(tab_bar.y - wrap.y) > 1.0:
    raise SystemExit(
        f'tab_bar is vertically offset inside tab_bar_wrap: wrap.y={wrap.y:.2f} tab_bar.y={tab_bar.y:.2f}'
    )
if not (31.0 <= tab_bar.h <= 33.0):
    raise SystemExit(f'tab_bar height is not the expected 32px row: h={tab_bar.h:.2f}')
if top_border_pixel == (255, 255, 255):
    raise SystemExit(
        f'first tab still leaves a white strip above its top border at ({top_border_x},{top_border_y})'
    )
PY

log "PASS"
