#!/usr/bin/env bash
# diagnostics-test.sh - Test havi:///diagnostics page and API surface
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="diagnostics"
TEST_GROUP="diagtest"
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

log "Waiting for diagnostics page to load..."
sleep 2

debugtool="$HAVI_ROOT/havi-devtools-cli"

output=$(printf '%s\n' \
    'document.title' \
    'document.querySelector("h1").textContent' \
    'document.getElementById("diagOutput") !== null' \
    'document.getElementById("joinFixtureState") !== null' \
    | "$debugtool" repl)

results=$(echo "$output" | jq -r 'select(.event == "evalResult") | .value')

[[ "$(echo "$results" | sed -n '1p')" == "Diagnostics - HAVI" ]] || fail "diagnostics page title mismatch"
[[ "$(echo "$results" | sed -n '2p')" == "Diagnostics" ]] || fail "diagnostics page heading mismatch"
[[ "$(echo "$results" | sed -n '3p')" == "true" ]] || fail "diagOutput missing"
[[ "$(echo "$results" | sed -n '4p')" == "true" ]] || fail "joinFixtureState missing"

fixture_default=$("$debugtool" --timeout 10 eval --await \
    'fetch("havi:///diagnostics/api?cmd=join_fixture_get").then(r => r.json()).then(j => j.ok === true && j.data && j.data.state === "none")' \
    2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$fixture_default" == "true" ]] || fail "join fixture default should be none"

fixture_pending=$("$debugtool" --timeout 10 eval --await \
    'fetch("havi:///diagnostics/api?cmd=join_fixture_set&state=pending").then(r => r.json()).then(j => j.ok === true && j.data && j.data.state === "pending")' \
    2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$fixture_pending" == "true" ]] || fail "failed to set pending fixture"

inspect_ok=$("$debugtool" --timeout 20 eval --await \
    'fetch("havi:///diagnostics/api?cmd=inspect&group='"$TEST_GROUP"'&app='"$TEST_APP"'&location=index.html").then(r => r.json()).then(j => j.ok === true && !!j.data && !!j.data.route && !!j.data.deploy && !!j.data.auth && !!j.data.join && j.data.route.configured === true)' \
    2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$inspect_ok" == "true" ]] || fail "diagnostics inspect API failed"

fixture_reset=$("$debugtool" --timeout 10 eval --await \
    'fetch("havi:///diagnostics/api?cmd=join_fixture_set&state=none").then(r => r.json()).then(j => j.ok === true && j.data && j.data.state === "none")' \
    2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$fixture_reset" == "true" ]] || fail "failed to reset fixture"

log "PASS"
