#!/usr/bin/env bash
# ring0-test.sh - Test window.ring0 from havi:// origin
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="ring0"

start_server
start_servo "havi:///"

log "Testing ring0 API (havi:// origin, pre-fetched credentials)..."
debugtool="$HAVI_ROOT/havi-devtools-cli"

# Run all tests in one persistent session (repl subcommand)
output=$(printf '%s\n' \
    'window.ring0 !== null' \
    'window.ring0.ring1Name !== null' \
    'typeof window.ring0.endpoint === "string"' \
    'window.ring0.repo !== null' \
    'typeof window.ring0.repo.port === "function"' \
    'typeof window.ring0.repo.status === "function"' \
    | "$debugtool" repl)

results=$(echo "$output" | jq -r 'select(.event == "evalResult") | .value')
test1=$(echo "$results" | sed -n '1p')
test2=$(echo "$results" | sed -n '2p')
test3=$(echo "$results" | sed -n '3p')
test4=$(echo "$results" | sed -n '4p')
test5=$(echo "$results" | sed -n '5p')
test6=$(echo "$results" | sed -n '6p')

[[ "$test1" == "true" ]] || fail "window.ring0 should exist on havi:// page"
[[ "$test2" == "true" ]] || fail "ring0.ring1Name should not be null (has credentials)"
[[ "$test3" == "true" ]] || fail "ring0.endpoint should be a string"
[[ "$test4" == "true" ]] || fail "ring0.repo should not be null"
[[ "$test5" == "true" ]] || fail "repo.port should be a function"
[[ "$test6" == "true" ]] || fail "repo.status should be a function"

log "PASS"
