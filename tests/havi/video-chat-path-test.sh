#!/usr/bin/env bash
# video-chat-path-test.sh - Validate chat byte-stream path primitives
#
# Covers:
# 1) getUserMedia + video.srcObject local preview path
# 2) StreamIn/StreamOut incremental byte transport
# 3) Length-prefixed frame reconstruction across arbitrary chunk boundaries
#
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="video-chat-path"
TEST_GROUP="videochatpath"
TEST_APP="testapp"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key

# Import only the pages this test uses (no mount helper needed).
HPPR_SIGNER='!ring0/init' $HPPR add "//$TEST_GROUP/$TEST_APP/test-utils.js" < "$SCRIPT_DIR/content/test-utils.js"
HPPR_SIGNER='!ring0/init' $HPPR add "//$TEST_GROUP/$TEST_APP/video-chat-path-test.html" < "$SCRIPT_DIR/content/video-chat-path-test.html"

start_servo "hppr://$TEST_GROUP/$TEST_APP/video-chat-path-test.html"
run_js_tests 30
