#!/usr/bin/env bash
# location-compat-test.sh - Verify lazy window.location compatibility on hppr:// pages
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="location-compat"
TEST_GROUP="~loccompat"
TEST_APP="testapp"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key
import_content_paths "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP" \
    location-compat.html \
    test-utils.js
start_servo "hppr://$TEST_GROUP/$TEST_APP//location-compat.html"
run_js_tests
