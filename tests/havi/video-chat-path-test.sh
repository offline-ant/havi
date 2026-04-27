#!/usr/bin/env bash
# video-chat-path-test.sh - Validate chat byte-stream path primitives
#
# Covers:
# 1) getUserMedia + video.srcObject local preview path
# 2) StreamPub/StreamSub incremental byte transport
# 3) Length-prefixed frame reconstruction across arbitrary chunk boundaries
#
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="video-chat-path"
TEST_GROUP="~videochatpath"
TEST_APP="testapp"

start_server
start_remote_server
setup_remote_acl "$TEST_GROUP" "$TEST_APP" "rwl"
create_remote_key

HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='ring1:ring0|init' \
  $HPPR add -k "$REMOTE_SECRET_KEY" "//$TEST_GROUP/$TEST_APP/test-utils.js" < "$SCRIPT_DIR/content/test-utils.js"
HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='ring1:ring0|init' \
  $HPPR add -k "$REMOTE_SECRET_KEY" "//$TEST_GROUP/$TEST_APP/video-chat-path-test.html" < "$SCRIPT_DIR/content/video-chat-path-test.html"

# Store signing key so JS test page can create a cooked StreamPub.
echo -n "$REMOTE_SECRET_KEY" | HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='ring1:ring0|init' \
  $HPPR add -k "$REMOTE_SECRET_KEY" "//$TEST_GROUP/$TEST_APP/testkey"

setup_remote_deploy "$TEST_GROUP" "$TEST_APP"
setup_route "$TEST_GROUP" "$TEST_APP"

start_servo "hppr://$TEST_GROUP/$TEST_APP/video-chat-path-test.html"
run_js_tests 30
