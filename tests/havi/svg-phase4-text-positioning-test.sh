#!/usr/bin/env bash
# svg-phase4-text-positioning-test.sh - focused Phase 4 SVG text positioning coverage
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="svg-phase4-text-positioning"

start_server
create_key

FILE_PAGE="file://$(realpath "$SCRIPT_DIR/content/svg-phase4-text-positioning.html")"
start_servo "$FILE_PAGE"
run_js_tests 30
