#!/usr/bin/env bash
# reveal-file-compat-test.sh - Verify reveal.js initializes under the file:// location shim
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="reveal-file-compat"

start_server
create_key

FILE_PAGE="file://$(realpath "$SCRIPT_DIR/content/reveal-file-compat.html")"
start_servo "$FILE_PAGE"
run_js_tests
