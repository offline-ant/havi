#!/usr/bin/env bash
# x-element-test.sh - Test <x> element rendering, same-origin access, cross-origin behavior, and packet property
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="x-element"
TEST_GROUP="test"
TEST_APP="app"
CROSS_APP="other"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
setup_acl "$TEST_GROUP" "$CROSS_APP"
create_key
import_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"
import_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$CROSS_APP"

# Run basic element test
log "Running basic element test..."
start_servo "hppr://$TEST_GROUP/$TEST_APP/x-element-basic.html"
run_js_tests

# Cleanup servo for next test
stop_pid "$SERVO_PID"
SERVO_PID=""

# Run content access test
log "Running content access test..."
start_servo "hppr://$TEST_GROUP/$TEST_APP/x-element-content.html"
run_js_tests

stop_pid "$SERVO_PID"
SERVO_PID=""

# Run packet property test
log "Running packet property test..."
start_servo "hppr://$TEST_GROUP/$TEST_APP/x-element-packet.html"
run_js_tests

stop_pid "$SERVO_PID"
SERVO_PID=""

# Run cross-origin restriction test
log "Running cross-origin restriction test..."
start_servo "hppr://$TEST_GROUP/$TEST_APP/x-element-cross-origin.html"
run_js_tests

log "All x-element tests passed"
