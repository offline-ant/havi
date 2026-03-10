#!/usr/bin/env bash
# stream-test.sh - Test StreamIn/StreamOut/StreamIO JS API
#
# Tests:
# 1. StreamOut creation — object has readyState, prefix, stream attributes
# 2. StreamOut receives data — reads bytes published by CLI stream-in
# 3. StreamIn creation — object has readyState, prefix, onopen
# 4. StreamIn onopen — readyState transitions to OPEN
# 5. StreamIn write — write resolves for Uint8Array
# 6. StreamIO creation — input/output attributes are StreamIn/StreamOut
#
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="stream"
TEST_GROUP="streamtest"
TEST_APP="testapp"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key
import_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"

echo -n "$SECRET_KEY" | HPPR_SIGNER='ring1:ring0#init' $HPPR add "//$TEST_GROUP/$TEST_APP/testkey"

# Start cooked publisher: delayed payload feed keeps stream open until data arrives.
# Uses --key for the cooked stream-in API (payload bytes in, trailer framing internal).
{ sleep 4; echo -n "hello-havi"; sleep 2; } | HPPR_SIGNER='ring1:ring0#init' $HPPR stream-in --key "$SECRET_KEY" "//$TEST_GROUP/$TEST_APP/live" &
PUB_PID=$!
log "Publisher started (PID: $PUB_PID)"

# Give the publisher time to register the stream
sleep 1

start_servo "hppr://$TEST_GROUP/$TEST_APP/stream-test.html"
run_js_tests 30

# Cleanup publisher
stop_pid "$PUB_PID"
PUB_PID=""
