#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: $0 <inputfile.(svg|png)> <outputfile.png>" >&2
  exit 2
fi

INPUT_FILE="$1"
OUTPUT_FILE="$2"

if [ -z "${OUTPUT_SIZE:-}" ]; then
  echo "error: OUTPUT_SIZE is required (example: OUTPUT_SIZE=64)" >&2
  exit 2
fi

case "$OUTPUT_SIZE" in
  ''|*[!0-9]*)
    echo "error: OUTPUT_SIZE must be a positive integer" >&2
    exit 2
    ;;
esac

if [ "$OUTPUT_SIZE" = "0" ]; then
  echo "error: OUTPUT_SIZE must be > 0" >&2
  exit 2
fi

if [ ! -f "$INPUT_FILE" ]; then
  echo "error: input file not found: $INPUT_FILE" >&2
  exit 1
fi

case "${INPUT_FILE##*.}" in
  svg|SVG|png|PNG) ;;
  *)
    echo "error: input must be .svg or .png" >&2
    exit 2
    ;;
esac

case "${OUTPUT_FILE##*.}" in
  png|PNG) ;;
  *)
    echo "error: output must be .png" >&2
    exit 2
    ;;
esac

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

HAVI_BIN=""
if [ -x "./target/debug/havi" ]; then
  HAVI_BIN="./target/debug/havi"
elif [ -x "./target/release/havi" ]; then
  HAVI_BIN="./target/release/havi"
else
  echo "error: havi binary not found. build first (./target/debug/havi or ./target/release/havi)" >&2
  exit 1
fi

if [ ! -x "./havi-devtools-cli" ]; then
  echo "error: missing executable: ./havi-devtools-cli" >&2
  exit 1
fi

INPUT_ABS="$(python3 -c 'import os,sys; print(os.path.abspath(sys.argv[1]))' "$INPUT_FILE")"
OUTPUT_DIR="$(dirname "$OUTPUT_FILE")"
mkdir -p "$OUTPUT_DIR"
OUTPUT_ABS="$(python3 -c 'import os,sys; print(os.path.abspath(sys.argv[1]))' "$OUTPUT_FILE")"

LOG_FILE="$SCRIPT_DIR/resources/icon-raster-runtime/havi.log"
mkdir -p "$(dirname "$LOG_FILE")"
: > "$LOG_FILE"

HAVI_PYLON_MODE=none HAVI_DEVTOOLS=127.0.0.1:0 HAVI_URL="about:blank" "$HAVI_BIN" --no-pylon >"$LOG_FILE" 2>&1 &
HAVI_PID=$!

cleanup() {
  if kill -0 "$HAVI_PID" >/dev/null 2>&1; then
    kill "$HAVI_PID" >/dev/null 2>&1 || true
    wait "$HAVI_PID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT INT TERM

DEVTOOLS_ADDR=""
for _ in $(seq 1 300); do
  if ! kill -0 "$HAVI_PID" >/dev/null 2>&1; then
    echo "error: havi exited before devtools became available" >&2
    tail -n 50 "$LOG_FILE" >&2 || true
    exit 1
  fi
  if grep -q '^HAVI_DEVTOOLS=' "$LOG_FILE"; then
    DEVTOOLS_ADDR="$(grep '^HAVI_DEVTOOLS=' "$LOG_FILE" | tail -n 1 | cut -d= -f2-)"
    break
  fi
  sleep 0.1
done

if [ -z "$DEVTOOLS_ADDR" ]; then
  echo "error: failed to get HAVI_DEVTOOLS from havi output" >&2
  tail -n 50 "$LOG_FILE" >&2 || true
  exit 1
fi

DEVTOOLS_PORT="${DEVTOOLS_ADDR##*:}"

python3 - "$INPUT_ABS" "$OUTPUT_ABS" "$OUTPUT_SIZE" "$DEVTOOLS_PORT" <<'PY'
import base64
import json
import subprocess
import sys

inp, outp, size, port = sys.argv[1:]

js = r"""
(async () => {
  const path = %INPUT_PATH%;
  const size = %SIZE%;

  const img = await new Promise((resolve, reject) => {
    const i = new Image();
    i.onload = () => resolve(i);
    i.onerror = () => reject(new Error('image load failed'));
    i.src = 'file://' + path;
  });

  const c = document.createElement('canvas');
  c.width = size;
  c.height = size;
  const ctx = c.getContext('2d');
  ctx.clearRect(0, 0, size, size);
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = 'high';
  ctx.drawImage(img, 0, 0, img.naturalWidth, img.naturalHeight, 0, 0, size, size);

  return c.toDataURL('image/png');
})()
"""

js = js.replace('%INPUT_PATH%', json.dumps(inp)).replace('%SIZE%', size)

proc = subprocess.run(
    ["./havi-devtools-cli", "-p", port, "--text", "eval", "--await", js],
    capture_output=True,
    text=True,
)
if proc.returncode != 0:
    sys.stderr.write(proc.stderr)
    sys.exit(proc.returncode)

out = proc.stdout.strip()
prefix = "data:image/png;base64,"
if not out.startswith(prefix):
    sys.stderr.write("error: unexpected eval output\n")
    sys.stderr.write(out + "\n")
    sys.exit(1)

png_bytes = base64.b64decode(out[len(prefix):])
with open(outp, "wb") as f:
    f.write(png_bytes)
PY
