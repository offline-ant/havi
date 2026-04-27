#!/usr/bin/env bash
# chunk-test.sh - Test chunk manifest transparency through committed source JS APIs
#
# Tests:
# 1. JS API chunk get — window.source.client.get() on chunked coordinate returns reassembled data
# 2. Non-chunked regression — small packet still works
#
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="chunk"
TEST_GROUP="~chunktest"
TEST_APP="testapp"

start_server
start_remote_server
setup_remote_acl "$TEST_GROUP" "$TEST_APP"
create_remote_key
import_remote_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"

# --- Generate chunked test data ---
CHUNK_DIR=$(mktemp -d)

# Generate 10KB text file with known pattern (100 lines, ~101 chars each)
for i in $(seq 0 99); do
    printf 'LINE_%03d:ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcdefghijklmnopqrstuvwxyz0123456789PADDING_END\n' "$i"
done > "$CHUNK_DIR/data.txt"

log "Test data size: $(wc -c < "$CHUNK_DIR/data.txt") bytes"

# Chunk and store (1024-byte chunks forces multiple chunks for ~10KB file)
HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='ring1:ring0|init' $HPPR chunk "$CHUNK_DIR/data.txt" \
    --chunk-size 1024 \
    --seal-by "$REMOTE_SECRET_KEY" \
    "//$TEST_GROUP/$TEST_APP/chunked/data.txt"
log "Chunked data stored"

# Store a small non-chunked packet for regression test
printf 'hello non-chunked world' | HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='ring1:ring0|init' $HPPR add -k "$REMOTE_SECRET_KEY" "//$TEST_GROUP/$TEST_APP/small/test.txt"
log "Small packet stored"

rm -rf "$CHUNK_DIR"

setup_remote_deploy "$TEST_GROUP" "$TEST_APP"
setup_route "$TEST_GROUP" "$TEST_APP"

start_servo "hppr://$TEST_GROUP/$TEST_APP/chunk-test.html"
run_js_tests
