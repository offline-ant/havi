#!/usr/bin/env bash
# diagnostics-test.sh - Test havi:///diagnostics page and API surface
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="diagnostics"
TEST_GROUP="diagtest"
TEST_API="testapp"

start_server
setup_acl "$TEST_GROUP" "$TEST_API"
create_key

start_remote_server
setup_remote_acl "$TEST_GROUP" "$TEST_API"
create_remote_key
import_remote_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_API"
setup_remote_ring2 "$TEST_GROUP" "$TEST_API"
setup_remote_deploy "$TEST_GROUP" "$TEST_API"
setup_route "$TEST_GROUP" "$TEST_API"

start_servo "havi:///diagnostics"

log "Waiting for diagnostics page to load..."
sleep 2

debugtool="$HAVI_ROOT/havi-devtools-cli"

output=$(printf '%s\n' \
    'document.title' \
    'document.querySelector("h1").textContent' \
    'document.getElementById("diagOutput") !== null' \
    | "$debugtool" repl)

results=$(echo "$output" | jq -r 'select(.event == "evalResult") | .value')

[[ "$(echo "$results" | sed -n '1p')" == "Diagnostics - HAVI" ]] || fail "diagnostics page title mismatch"
[[ "$(echo "$results" | sed -n '2p')" == "Diagnostics" ]] || fail "diagnostics page heading mismatch"
[[ "$(echo "$results" | sed -n '3p')" == "true" ]] || fail "diagOutput missing"

inspect_ok=$("$debugtool" --timeout 20 eval --await \
    'fetch("havi:///diagnostics/api?cmd=inspect&group='"$TEST_GROUP"'&api='"$TEST_API"'&key=index.html").then(r => r.json()).then(j => j.ok === true && !!j.data && !!j.data.route && !!j.data.deploy && !!j.data.auth && j.data.route.configured === true)' \
    2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$inspect_ok" == "true" ]] || fail "diagnostics inspect API failed"

log "PASS"
