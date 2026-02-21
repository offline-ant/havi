#!/usr/bin/env bash
# address-navigate-test.sh - Test Address setters trigger navigation
#
# Tests:
#   1. navigate command (window.address.href setter)
#   2. window.address.location setter triggers navigation
#   3. window.address = url (PutForwards)
#
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="address-navigate"
TEST_GROUP="navtest"
TEST_APP="testapp"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key
import_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"

debugtool="$HAVI_ROOT/havi-debugger-cli"

start_servo "hppr://$TEST_GROUP/$TEST_APP/nav-start.html"

# ============================================================================
# Test 1: navigate command (href setter)
# ============================================================================

log "Test 1: navigate command..."
initial=$("$debugtool" --text eval 'window.address.location')
[[ "$initial" == "nav-start.html" ]] || fail "Not on nav-start.html: $initial"

"$debugtool" --text navigate "hppr://$TEST_GROUP/$TEST_APP/nav-dest.html" >/dev/null
loc=$("$debugtool" --text eval 'window.address.location')
[[ "$loc" == "nav-dest.html" ]] || fail "navigate did not reach nav-dest.html: $loc"
log "  navigate OK"

# ============================================================================
# Test 2: location setter triggers navigation
# ============================================================================

log "Test 2: window.address.location setter..."
"$debugtool" eval "window.address.location = 'nav-start.html'" >/dev/null
"$debugtool" --text wait-for "window.address.location === 'nav-start.html'" >/dev/null
loc=$("$debugtool" --text eval 'window.address.location')
[[ "$loc" == "nav-start.html" ]] || fail "location setter did not navigate: $loc"
log "  location setter OK"

# ============================================================================
# Test 3: PutForwards (window.address = url)
# ============================================================================

log "Test 3: window.address = url (PutForwards)..."
"$debugtool" eval "window.address = 'hppr://$TEST_GROUP/$TEST_APP/nav-dest.html'" >/dev/null
"$debugtool" --text wait-for "window.address.location === 'nav-dest.html'" >/dev/null
loc=$("$debugtool" --text eval 'window.address.location')
[[ "$loc" == "nav-dest.html" ]] || fail "PutForwards did not navigate: $loc"
log "  PutForwards OK"

log "PASS"
