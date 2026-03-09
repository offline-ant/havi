#!/usr/bin/env bash
# file-location-compat-test.sh - Verify lazy window.location compatibility on file:// pages
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="file-location-compat"

start_server
create_key

FILE_PAGE="file://$(realpath "$SCRIPT_DIR/content/file-location-compat.html")"
start_servo "$FILE_PAGE"
run_js_tests
