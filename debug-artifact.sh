#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
HAVI_DIR="$ROOT_DIR/havi"
OUT_DIR="${TMPDIR:-/tmp}/havi-artifact"
mkdir -p "$OUT_DIR"

TS="$(date +%Y%m%d-%H%M%S)"
LOG="$OUT_DIR/run-$TS.log"
FULL="$OUT_DIR/full-$TS.png"
CROP="$OUT_DIR/top-left-$TS.png"
SOCKET="$OUT_DIR/havi-debug.sock"

cd "$HAVI_DIR"

# Kill older HAVI runs so the socket/screenshot target is unambiguous.
pkill -f '/havi/target/debug/havi' >/dev/null 2>&1 || true
pkill -f 'mach-havi run --makepad-socket' >/dev/null 2>&1 || true
rm -f "$SOCKET"
sleep 0.3

./mach-havi run --makepad-socket --makepad-socket-path "$SOCKET" >"$LOG" 2>&1 &
RUN_PID=$!

cleanup() {
  if kill -0 "$RUN_PID" >/dev/null 2>&1; then
    kill "$RUN_PID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

READY=0
for _ in $(seq 1 180); do
  if [[ -S "$SOCKET" ]]; then
    if "$HAVI_DIR/havi-makepad-cli" --socket "$SOCKET" dump >/dev/null 2>&1; then
      READY=1
      break
    fi
  fi
  sleep 0.5
done

if [[ "$READY" -ne 1 ]]; then
  echo "ERROR: makepad socket not ready at $SOCKET (log: $LOG)" >&2
  exit 1
fi

# Allow first frames/layout to settle before capture.
sleep "${HAVI_ARTIFACT_WAIT_SECONDS:-4}"

"$HAVI_DIR/havi-makepad-cli" --socket "$SOCKET" screenshot "$FULL" >/dev/null

python3 - <<'PY' "$FULL" "$CROP"
import sys
from PIL import Image
src, dst = sys.argv[1], sys.argv[2]
img = Image.open(src)
# Tab bar + toolbar area in top-left corner
crop = img.crop((0, 0, 640, 120))
crop.save(dst)
PY

echo "$CROP"
