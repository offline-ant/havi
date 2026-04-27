#!/usr/bin/env bash
# ring0-test.sh - Test window.havi.admin from havi:// origin
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="ring0"

start_server
start_servo "havi:///"

log "Testing internal helper admin API (havi:// origin, pre-fetched credentials)..."
debugtool="$HAVI_ROOT/havi-devtools-cli"

# Run all tests in one persistent session (repl subcommand)
output=$(printf '%s\n' \
    'window.havi !== null' \
    'window.havi.admin !== null' \
    'window.havi.admin.client.ring1Name !== null' \
    'typeof window.havi.admin.client.endpoint === "string"' \
    'typeof window.ring0 === "undefined"' \
    'typeof window.havi.admin.repo.port === "function"' \
    'typeof window.havi.admin.repo.status === "function"' \
    | "$debugtool" repl)

results=$(echo "$output" | jq -r 'select(.event == "evalResult") | .value')
test1=$(echo "$results" | sed -n '1p')
test2=$(echo "$results" | sed -n '2p')
test3=$(echo "$results" | sed -n '3p')
test4=$(echo "$results" | sed -n '4p')
test5=$(echo "$results" | sed -n '5p')
test6=$(echo "$results" | sed -n '6p')
test7=$(echo "$results" | sed -n '7p')

[[ "$test1" == "true" ]] || fail "window.havi should exist on havi:// page"
[[ "$test2" == "true" ]] || fail "window.havi.admin should not be null"
[[ "$test3" == "true" ]] || fail "admin client ring1Name should not be null"
[[ "$test4" == "true" ]] || fail "admin client endpoint should be a string"
[[ "$test5" == "true" ]] || fail "window.ring0 should be removed from the Window surface"
[[ "$test6" == "true" ]] || fail "repo.port should be a function"
[[ "$test7" == "true" ]] || fail "repo.status should be a function"

log "PASS"
