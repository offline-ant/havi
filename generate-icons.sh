#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

ICON_SOURCE="${1:-$SCRIPT_DIR/../logo.svg}"
OUT_DIR="$SCRIPT_DIR/ports/havishell/resources"

if [ ! -f "$ICON_SOURCE" ]; then
  echo "error: icon source not found: $ICON_SOURCE" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"

for size in 32 64 128; do
  echo "[generate-icons] rendering ${size}x${size}"
  OUTPUT_SIZE="$size" "$SCRIPT_DIR/generate-icon.sh" "$ICON_SOURCE" "$OUT_DIR/icon_${size}.png"
done

uv run --frozen --project "$SCRIPT_DIR" python - "$OUT_DIR/icon_32.png" "$OUT_DIR/icon_64.png" "$OUT_DIR/icon_128.png" "$OUT_DIR/icon.ico" <<'PY'
import struct
import sys
from pathlib import Path

png_paths = [Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])]
ico_path = Path(sys.argv[4])
sizes = [32, 64, 128]
blobs = [p.read_bytes() for p in png_paths]

out = bytearray()
out += struct.pack('<HHH', 0, 1, len(blobs))
offset = 6 + 16 * len(blobs)

for size, blob in zip(sizes, blobs):
    out += bytes([
        size if size < 256 else 0,
        size if size < 256 else 0,
        0,
        0,
    ])
    out += struct.pack('<HHII', 1, 32, len(blob), offset)
    offset += len(blob)

for blob in blobs:
    out += blob

ico_path.write_bytes(out)
PY

echo "[generate-icons] wrote:"
ls -l "$OUT_DIR/icon_32.png" "$OUT_DIR/icon_64.png" "$OUT_DIR/icon_128.png" "$OUT_DIR/icon.ico"
