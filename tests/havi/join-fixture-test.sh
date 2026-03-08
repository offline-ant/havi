#!/usr/bin/env bash
# join-fixture-test.sh - Deterministic hppr-join fixture state controls
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="join-fixture"
TEST_GROUP="joinfixture"
TEST_APP="testapp"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key

start_remote_server
setup_remote_acl "$TEST_GROUP" "$TEST_APP"
create_remote_key
import_remote_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"
setup_remote_ring2 "$TEST_GROUP" "$TEST_APP"
setup_remote_deploy "$TEST_GROUP" "$TEST_APP"
setup_route "$TEST_GROUP" "$TEST_APP"

start_servo "havi:///diagnostics"

debugtool="$HAVI_ROOT/havi-devtools-cli"

set_pending=$("$debugtool" --timeout 10 eval --await \
    'fetch("havi:///diagnostics/api?cmd=join_fixture_set&state=pending").then(r => r.json()).then(j => j.ok === true && j.data && j.data.state === "pending")' \
    2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$set_pending" == "true" ]] || fail "failed to set pending fixture"

"$debugtool" --timeout 10 eval 'window.address.href = "hppr-join://'"$TEST_GROUP"'/'"$TEST_APP"'/"' >/dev/null
sleep 1

pending_status=$("$debugtool" --timeout 10 eval --await \
    '(() => { const btn = document.getElementById("join-btn"); if (!btn) return false; btn.click(); return true; })()' \
    2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$pending_status" == "true" ]] || fail "join button missing on hppr-join page"

sleep 1

pending_effect=$("$debugtool" --timeout 10 eval --await \
    '(() => { const status = (document.getElementById("join-status") || {}).textContent || ""; return status.includes("pending") && window.address.href.startsWith("hppr-join://"); })()' \
    2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$pending_effect" == "true" ]] || fail "pending fixture did not keep deterministic pending state"

"$debugtool" --timeout 10 eval 'window.address.href = "havi:///diagnostics"' >/dev/null
sleep 1

set_approved=$("$debugtool" --timeout 10 eval --await \
    'fetch("havi:///diagnostics/api?cmd=join_fixture_set&state=approved").then(r => r.json()).then(j => j.ok === true && j.data && j.data.state === "approved")' \
    2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$set_approved" == "true" ]] || fail "failed to set approved fixture"

"$debugtool" --timeout 10 eval 'window.address.href = "hppr-join://'"$TEST_GROUP"'/'"$TEST_APP"'/"' >/dev/null
sleep 1

"$debugtool" --timeout 10 eval --await \
    '(() => { const btn = document.getElementById("join-btn"); if (!btn) return false; btn.click(); return true; })()' \
    >/dev/null 2>&1 || fail "failed to click join button for approved fixture"

sleep 2

approved_effect=$("$debugtool" --timeout 15 eval --await \
    'window.address.href.startsWith("hppr://'"$TEST_GROUP"'/'"$TEST_APP"'/")' \
    2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$approved_effect" == "true" ]] || fail "approved fixture did not navigate to hppr://$TEST_GROUP/$TEST_APP/"

"$debugtool" --timeout 10 eval 'window.address.href = "havi:///diagnostics"' >/dev/null
sleep 1

set_none=$("$debugtool" --timeout 10 eval --await \
    'fetch("havi:///diagnostics/api?cmd=join_fixture_set&state=none").then(r => r.json()).then(j => j.ok === true && j.data && j.data.state === "none")' \
    2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$set_none" == "true" ]] || fail "failed to reset fixture"

log "PASS"
