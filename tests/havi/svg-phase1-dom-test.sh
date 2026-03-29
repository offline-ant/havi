#!/usr/bin/env bash
# svg-phase1-dom-test.sh - focused Phase 1 SVG DOM shape coverage
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="svg-phase1-dom"

start_server
create_key

FILE_PAGE="file://$(realpath "$SCRIPT_DIR/content/svg-phase1-dom.html")"
start_servo "$FILE_PAGE"
run_js_tests 30
