#!/usr/bin/env bash
# media-source-playback-test.sh - Verify MediaSource valid fMP4 decode/present path
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="media-source-playback"
TEST_GROUP="~mediasource"
TEST_APP="testapp"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key
import_content_paths "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP" \
    media-source-playback.html \
    media-source-h264-frag.mp4 \
    test-utils.js
start_servo "hppr://$TEST_GROUP/$TEST_APP//media-source-playback.html"
run_js_tests
