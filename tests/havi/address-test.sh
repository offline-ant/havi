#!/usr/bin/env bash
# address-test.sh - Test Address DOM object end-to-end
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="address"
TEST_GROUP="addrtest"
TEST_APP="testapp"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key
import_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"
start_servo "hppr://$TEST_GROUP/$TEST_APP/address.html"
run_js_tests
