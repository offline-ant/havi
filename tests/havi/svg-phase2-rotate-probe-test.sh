#!/usr/bin/env bash
# svg-phase2-rotate-probe-test.sh - isolate rotate number-list mutation
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="svg-phase2-rotate-probe"

start_server
create_key

FILE_PAGE="file://$(realpath "$SCRIPT_DIR/content/svg-phase2-rotate-probe.html")"
start_servo "$FILE_PAGE"
run_js_tests 30
