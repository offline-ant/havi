#!/usr/bin/env bash
# storage-test.sh - Test localStorage and sessionStorage on hppr:// pages
# Verifies that HPPR's tuple origin model allows Web Storage access.
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="storage"
TEST_GROUP="storagetest"
TEST_APP="testapp"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key
import_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"
start_servo "hppr://$TEST_GROUP/$TEST_APP/storage.html"
run_js_tests
