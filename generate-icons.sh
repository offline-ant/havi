#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

ICON_SOURCE="${1:-$SCRIPT_DIR/../logo.svg}"
OUT_DIR="$SCRIPT_DIR/ports/havishell/resources"
ANDROID_RES_DIR="$OUT_DIR/android/res"

if [ ! -f "$ICON_SOURCE" ]; then
  echo "error: icon source not found: $ICON_SOURCE" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"

# Desktop / platform build-time icons expected by makepad-platform build.rs
for size in 32 64 128 256 512 1024; do
  echo "[generate-icons] rendering desktop icon_${size}.png"
  OUTPUT_SIZE="$size" "$SCRIPT_DIR/generate-icon.sh" "$ICON_SOURCE" "$OUT_DIR/icon_${size}.png"
done

# Android launcher icons expected by cargo makepad android pipeline
for density_size in \
  "mipmap-mdpi 48" \
  "mipmap-hdpi 72" \
  "mipmap-xhdpi 96" \
  "mipmap-xxhdpi 144" \
  "mipmap-xxxhdpi 192"
do
  density="${density_size% *}"
  size="${density_size#* }"
  dst_dir="$ANDROID_RES_DIR/$density"
  mkdir -p "$dst_dir"
  echo "[generate-icons] rendering android $density/ic_launcher.png (${size}x${size})"
  OUTPUT_SIZE="$size" "$SCRIPT_DIR/generate-icon.sh" "$ICON_SOURCE" "$dst_dir/ic_launcher.png"
done

uv run --frozen --project "$SCRIPT_DIR" python - "$OUT_DIR/icon_32.png" "$OUT_DIR/icon_64.png" "$OUT_DIR/icon_128.png" "$OUT_DIR/icon_256.png" "$OUT_DIR/icon.ico" <<'PY'
import struct
import sys
from pathlib import Path

png_paths = [Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3]), Path(sys.argv[4])]
ico_path = Path(sys.argv[5])
sizes = [32, 64, 128, 256]
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

# iOS app icon — only needs 1024x1024 (already generated above as icon_1024.png)
# cargo-makepad apple build picks up resources/icon_1024.png automatically.

echo "[generate-icons] wrote desktop resources (also used by iOS):"
ls -l \
  "$OUT_DIR/icon_32.png" \
  "$OUT_DIR/icon_64.png" \
  "$OUT_DIR/icon_128.png" \
  "$OUT_DIR/icon_256.png" \
  "$OUT_DIR/icon_512.png" \
  "$OUT_DIR/icon_1024.png" \
  "$OUT_DIR/icon.ico"
echo "[generate-icons] wrote android resources:"
ls -l \
  "$ANDROID_RES_DIR/mipmap-mdpi/ic_launcher.png" \
  "$ANDROID_RES_DIR/mipmap-hdpi/ic_launcher.png" \
  "$ANDROID_RES_DIR/mipmap-xhdpi/ic_launcher.png" \
  "$ANDROID_RES_DIR/mipmap-xxhdpi/ic_launcher.png" \
  "$ANDROID_RES_DIR/mipmap-xxxhdpi/ic_launcher.png"
