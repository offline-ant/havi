#!/usr/bin/env bash
# editor-test.sh - Test editor save button navigation
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="editor"
TEST_GROUP="u"
TEST_APP="web"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key
import_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"

# Use user/ path so ring1 sandbox has write permission (spec 060-SITE-RING1.md)
start_servo "hppr-editor://$TEST_GROUP/$TEST_APP/user/simple.html"

log "Testing editor save..."
debugtool="$HAVI_ROOT/havi-debugger-cli"

# Verify editor page via DOM (location.href is unreliable in devtools)

# Verify single Save button exists (no ring1/remote/admin buttons)
save_btn=$("$debugtool" --text eval 'document.getElementById("saveBtn") ? "found" : "missing"')
if [[ "$save_btn" != "found" ]]; then
    title=$("$debugtool" --text eval 'document.title')
    body=$("$debugtool" --text eval 'document.body ? document.body.innerText.slice(0, 200) : ""')
    fail "Save button not found (title=$title body=$body)"
fi

# Verify old ring1/remote/admin buttons are gone
ring1_btn=$("$debugtool" --text eval 'document.getElementById("saveLocalBtn") ? "found" : "missing"')
[[ "$ring1_btn" == "missing" ]] || fail "Old saveLocalBtn still exists"

# Click Save and wait for navigation back to hppr://
"$debugtool" --timeout 5 eval 'document.getElementById("saveBtn").click()' >/dev/null
sleep 1

expected_url="hppr://$TEST_GROUP/$TEST_APP/user/simple.html"
current_url=$("$debugtool" --text --timeout 5 eval 'window.address.href' 2>/dev/null || true)
if [[ "$current_url" == *"$expected_url"* ]]; then
    log "PASS"
    exit 0
else
    fail "Expected navigation to $expected_url, got: $current_url"
fi
