#!/usr/bin/env bash
# svg-phase2-textlength-probe-test.sh - isolate textLength wrapper mutation
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="svg-phase2-textlength-probe"

start_server
create_key

FILE_PAGE="file://$(realpath "$SCRIPT_DIR/content/svg-phase2-textlength-probe.html")"
start_servo "$FILE_PAGE"
run_js_tests 30
