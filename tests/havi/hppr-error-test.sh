#!/usr/bin/env bash
# hppr-error-test.sh - Test HpprError JavaScript API
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="hppr-error"
TEST_GROUP="hpprerrortest"
TEST_APP="testapp"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key
import_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"
start_servo "hppr://$TEST_GROUP/$TEST_APP/hppr-error.html"
run_js_tests
