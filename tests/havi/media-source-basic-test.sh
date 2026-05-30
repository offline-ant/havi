#!/usr/bin/env bash
# media-source-basic-test.sh - Verify MediaSource / SourceBuffer custom playback wiring
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="media-source-basic"
TEST_GROUP="mediasource"
TEST_APP="testapp"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key
import_content_paths "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP" \
    media-source-basic.html \
    test-utils.js
start_servo "hppr://$TEST_GROUP/$TEST_APP//media-source-basic.html"
run_js_tests
