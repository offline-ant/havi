#!/usr/bin/env bash
# svg-phase5-textpath-test.sh - Phase 5 textPath and hidden-character coverage
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="svg-phase5-textpath"

start_server
create_key

FILE_PAGE="file://$(realpath "$SCRIPT_DIR/content/svg-phase5-textpath.html")"
start_servo "$FILE_PAGE"
run_js_tests 30
