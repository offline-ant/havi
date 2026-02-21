#!/usr/bin/env bash
# home-repo-test.sh - Test havi:///home-repo daemon info display
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="home-repo"

start_server
start_servo "havi:///home-repo"

log "Waiting for page to load..."
sleep 2

debugtool="$HAVI_ROOT/havi-webview-remote-cli"

# Run tests: check that hello() returns greeting, and page elements exist
output=$(printf '%s\n' \
    'typeof window.ring0 !== "undefined" && window.ring0 !== null' \
    'document.getElementById("port") !== null' \
    'document.getElementById("status") !== null' \
    'document.getElementById("repoKey") !== null' \
    'document.getElementById("daemonInfoCard") !== null' \
    'document.getElementById("daemonStatus") !== null' \
    'document.getElementById("daemonUptime") !== null' \
    'document.getElementById("daemonBackend") !== null' \
    'document.getElementById("daemonVersion") !== null' \
    | "$debugtool" repl)

results=$(echo "$output" | jq -r 'select(.event == "evalResult") | .value')

i=0
while IFS= read -r val; do
    i=$((i + 1))
    [[ "$val" == "true" ]] || fail "Test $i failed (got: $val)"
done <<< "$results"

[[ $i -ge 9 ]] || fail "Expected 9 results, got $i"

# Test that hello() returns a string with repo info
hello_result=$("$debugtool" --timeout 10 eval --await \
    'window.ring0.hello().then(g => (g.repoName && g.verifyingKey && g.sessionId) ? "ok" : "bad")' \
    2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$hello_result" == "ok" ]] || fail "hello() greeting format wrong: $hello_result"

log "PASS"
