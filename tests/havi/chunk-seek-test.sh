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
TEST_GROUP="~seektest"
TEST_APP="testapp"

start_server
start_remote_server
setup_remote_acl "$TEST_GROUP" "$TEST_APP"
create_remote_key
import_remote_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"

# --- Generate chunked test data (same pattern as chunk-test.sh) ---
CHUNK_DIR=$(mktemp -d)

for i in $(seq 0 99); do
    printf 'LINE_%03d:ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcdefghijklmnopqrstuvwxyz0123456789PADDING_END\n' "$i"
done > "$CHUNK_DIR/data.txt"

log "Test data size: $(wc -c < "$CHUNK_DIR/data.txt") bytes"

# Chunk at 1024-byte boundaries to force multiple chunks for ~10KB file
HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='ring1:ring0|init' $HPPR chunk "$CHUNK_DIR/data.txt" \
    --chunk-size 1024 \
    --seal-by "$REMOTE_SECRET_KEY" \
    "//$TEST_GROUP/$TEST_APP/chunked/data.txt"
log "Chunked data stored"

# --- Store a small multi-chunk manifest through the surviving chunk writer ---
# Total data: 30 bytes = "AAAAAAAAAA" + "BBBBBBBBBB" + "CCCCCCCCCC"
# This stays on the real chunk path instead of the deleted top-level Blob STORE path.
printf 'AAAAAAAAAABBBBBBBBBBCCCCCCCCCC' > "$CHUNK_DIR/nested.bin"
HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='ring1:ring0|init' $HPPR chunk "$CHUNK_DIR/nested.bin" \
    --chunk-size 10 \
    --seal-by "$REMOTE_SECRET_KEY" \
    "//$TEST_GROUP/$TEST_APP/chunked/nested.bin"
log "Multi-chunk manifest stored (30 bytes via chunk writer)"

rm -rf "$CHUNK_DIR"

setup_remote_deploy "$TEST_GROUP" "$TEST_APP"
setup_route "$TEST_GROUP" "$TEST_APP"

# === JS tests: Range requests on chunked content ===
log "=== Phase 1: Chunk seek JS tests ==="
start_servo "hppr://$TEST_GROUP/$TEST_APP/chunk-seek-test.html"
run_js_tests 30
