#!/usr/bin/env bash
# svg-phase4-tspan-ordering-test.sh - nested tspan chunk ordering coverage
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="svg-phase4-tspan-ordering"

start_server
create_key

FILE_PAGE="file://$(realpath "$SCRIPT_DIR/content/svg-phase4-tspan-ordering.html")"
start_servo "$FILE_PAGE"
run_js_tests 30
