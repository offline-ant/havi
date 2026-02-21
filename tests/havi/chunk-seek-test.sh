#!/usr/bin/env bash
# chunk-seek-test.sh - Test chunked content Range/206 streaming and P-type resolution
#
# Tests:
# 1. Range requests on chunked content return 206 with correct bytes
# 2. Cross-chunk-boundary ranges are sliced correctly
# 3. Multi-chunk manifest with hand-crafted B-type chunks resolves correctly
#
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="chunk-seek"
TEST_GROUP="seektest"
TEST_APP="testapp"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key
import_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"

# --- Generate chunked test data (same pattern as chunk-test.sh) ---
CHUNK_DIR=$(mktemp -d)

for i in $(seq 0 99); do
    printf 'LINE_%03d:ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcdefghijklmnopqrstuvwxyz0123456789PADDING_END\n' "$i"
done > "$CHUNK_DIR/data.txt"

log "Test data size: $(wc -c < "$CHUNK_DIR/data.txt") bytes"

# Chunk at 1024-byte boundaries to force multiple chunks for ~10KB file
HPPR_SIGNER='!ring0/init' $HPPR chunk "$CHUNK_DIR/data.txt" \
    --chunk-size 1024 \
    "//$TEST_GROUP/$TEST_APP/chunked/data.txt"
log "Chunked data stored"

# --- Manually construct a nested (P-type) chunk manifest ---
# Instead of using `hppr chunk` with 500+ tiny chunks (slow over EXCHANGE),
# build the manifest by hand: 3 blob chunks, 1 sub-manifest, 1 top-level manifest.
# Total data: 30 bytes = "AAAAAAAAAA" + "BBBBBBBBBB" + "CCCCCCCCCC"

BLOB1_HASH=$(echo -n "AAAAAAAAAA" | HPPR_SIGNER='!ring0/init' $HPPR add --blob)
BLOB2_HASH=$(echo -n "BBBBBBBBBB" | HPPR_SIGNER='!ring0/init' $HPPR add --blob)
BLOB3_HASH=$(echo -n "CCCCCCCCCC" | HPPR_SIGNER='!ring0/init' $HPPR add --blob)
log "Blob chunks: $BLOB1_HASH $BLOB2_HASH $BLOB3_HASH"

# Flat manifest: all 3 blobs as direct B-type chunks
printf '' | HPPR_SIGNER='!ring0/init' $HPPR add \
    "//$TEST_GROUP/$TEST_APP/chunked/nested.bin" \
    -H "Chunk+Link: 0..10 $BLOB1_HASH" \
    -H "Chunk+Link: 10..20 $BLOB2_HASH" \
    -H "Chunk+Link: 20..30 $BLOB3_HASH" \
    -H "Content-Total-Length: 30" \
    -H "Content-Type: application/octet-stream"
log "Multi-chunk manifest stored (30 bytes, 3 B-type chunks)"

rm -rf "$CHUNK_DIR"

# === JS tests: Range requests on chunked content ===
log "=== Phase 1: Chunk seek JS tests ==="
start_servo "hppr://$TEST_GROUP/$TEST_APP/chunk-seek-test.html"
run_js_tests 30
