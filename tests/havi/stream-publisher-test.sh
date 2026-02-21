#!/usr/bin/env bash
# stream-publisher-test.sh - Test StreamIn publisher mode (onpacket, finishSegment)
#
# Tests:
# 1. StreamIn with key creates publisher mode object
# 2. onpacket fires when finishSegment() is called
# 3. Event data contains segment hash
#
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="stream-publisher"
TEST_GROUP="streampubtest"
TEST_APP="testapp"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key
import_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"

# Store the signing key as a packet so the JS test page can fetch it
echo -n "$SECRET_KEY" | HPPR_SIGNER='!ring0/init' $HPPR add "//$TEST_GROUP/$TEST_APP/testkey"

start_servo "hppr://$TEST_GROUP/$TEST_APP/stream-publisher-test.html"
run_js_tests 30
