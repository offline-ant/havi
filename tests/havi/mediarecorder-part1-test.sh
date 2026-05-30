#!/usr/bin/env bash
# mediarecorder-part1-test.sh - Verify MediaRecorder part1 API surface and NotYetImplemented paths
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="mediarecorder-part1"
TEST_GROUP="mediarec"
TEST_APP="testapp"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key

HPPR_SIGNER='ring1:ring0|init' $HPPR add "//$TEST_GROUP/$TEST_APP//test-utils.js" < "$SCRIPT_DIR/content/test-utils.js"
HPPR_SIGNER='ring1:ring0|init' $HPPR add "//$TEST_GROUP/$TEST_APP//mediarecorder-part1-test.html" < "$SCRIPT_DIR/content/mediarecorder-part1-test.html"

start_servo "hppr://$TEST_GROUP/$TEST_APP//mediarecorder-part1-test.html"
run_js_tests
