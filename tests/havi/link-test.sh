#!/usr/bin/env bash
# link-test.sh - Test navigation detection via debugger CLI
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="link"
TEST_GROUP="linktest"
TEST_APP="testapp"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key
import_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"
start_servo "hppr://$TEST_GROUP/$TEST_APP/link-start.html"

log "Testing link navigation..."
debugtool="$HAVI_ROOT/havi-debugger-cli"

# Verify start page (HAVI disables window.location; use window.address)
initial_url=$("$debugtool" --text eval 'window.address.href')
[[ "$initial_url" == *"link-start.html"* ]] || fail "Not on link-start.html"

# Click link and wait for navigation
"$debugtool" eval 'document.querySelector("a").click()' >/dev/null
sleep 1

# Verify we navigated to the target page
nav_url=$("$debugtool" --text --timeout 5 eval 'window.address.href' 2>/dev/null || true)
[[ -n "$nav_url" ]] || fail "No URL after navigation"
[[ "$nav_url" == *"link-target.html"* ]] || fail "Navigation target incorrect: $nav_url"

log "PASS"
