#!/usr/bin/env bash
# disabled-apis-test.sh - Verify disabled web APIs throw TypeError on hppr:// pages
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="disabled-apis"
TEST_GROUP="dapitest"
TEST_APP="testapp"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key
import_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"
start_servo "hppr://$TEST_GROUP/$TEST_APP/disabled-apis.html"
run_js_tests
