#!/usr/bin/env bash
# svg-phase5-textlength-spacing-test.sh - Phase 5 textLength/lengthAdjust spacing coverage
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="svg-phase5-textlength-spacing"

start_server
create_key

FILE_PAGE="file://$(realpath "$SCRIPT_DIR/content/svg-phase5-textlength-spacing.html")"
start_servo "$FILE_PAGE"
run_js_tests 30
