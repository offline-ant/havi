#!/usr/bin/env bash
# browse-test.sh - Test hppr:// LIST mode (directory browsing with trailing slash)
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="browse"
TEST_GROUP="browsetest"
TEST_APP="files"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key
import_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"
start_servo "hppr://$TEST_GROUP/$TEST_APP/"

log "Testing browse page structure..."
debugtool="$HAVI_ROOT/havi-devtools-cli"

# Test page title contains "Index of"
title=$("$debugtool" --text eval "document.title")
[[ "$title" == *"Index of"* ]] || fail "Title should contain 'Index of': $title"

# Test page has links (directory entries)
link_count=$("$debugtool" --text eval "document.querySelectorAll('a').length" | cut -d. -f1)
[[ "$link_count" -ge 1 ]] || fail "Should have at least 1 link: $link_count"

log "PASS"
