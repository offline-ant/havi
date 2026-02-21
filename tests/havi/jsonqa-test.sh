#!/usr/bin/env bash
# jsonqa-test.sh - Test JSONqa metadata parsing and manipulation
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="jsonqa"
TEST_GROUP="jsonqatest"
TEST_APP="testapp"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key
import_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"
start_servo "hppr://$TEST_GROUP/$TEST_APP/jsonqa.html"
run_js_tests
