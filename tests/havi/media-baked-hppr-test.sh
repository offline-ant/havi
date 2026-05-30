#!/usr/bin/env bash
# media-baked-hppr-test.sh - Verify direct hppr:// media uses baked custom playback
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="media-baked-hppr"
TEST_GROUP="mediasource"
TEST_APP="testapp"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key
import_content_paths "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP" \
    media-baked-hppr.html \
    media-source-h264-frag.mp4 \
    test-utils.js
start_servo "hppr://$TEST_GROUP/$TEST_APP//media-baked-hppr.html"
run_js_tests
