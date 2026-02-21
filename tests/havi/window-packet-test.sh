#!/usr/bin/env bash
# window-packet-test.sh - Test window.packet API migration
# Verifies that window.packet works and window.route.packet is removed
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="window-packet"
TEST_GROUP="packettest"
TEST_APP="testapp"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key
import_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"
start_servo "hppr://$TEST_GROUP/$TEST_APP/window-packet.html"
run_js_tests
