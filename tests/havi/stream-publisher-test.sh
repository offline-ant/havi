#!/usr/bin/env bash
# stream-publisher-test.sh - Test StreamPub publisher mode (onpacket, finishSegment)
#
# Tests:
# 1. StreamPub with key creates publisher mode object
# 2. onpacket fires when finishSegment() is called
# 3. Event data contains segment hash
#
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="stream-publisher"
TEST_GROUP="~streampubtest"
TEST_APP="testapp"

start_server
start_remote_server
setup_remote_acl "$TEST_GROUP" "$TEST_APP" "rwl"
create_remote_key
import_remote_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"

# Store the signing key as a packet so the JS test page can fetch it.
echo -n "$REMOTE_SECRET_KEY" | HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='ring1:ring0|init' \
  $HPPR add -k "$REMOTE_SECRET_KEY" "//$TEST_GROUP/$TEST_APP/testkey"

setup_remote_deploy "$TEST_GROUP" "$TEST_APP"
setup_route "$TEST_GROUP" "$TEST_APP"

start_servo "hppr://$TEST_GROUP/$TEST_APP/stream-publisher-test.html"
run_js_tests 30
