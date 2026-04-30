#!/usr/bin/env bash
# home-repo-test.sh - Test havi:///home-repo page-owned API surface
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="home-repo"

start_server
start_servo "havi:///home-repo"

log "Waiting for page to load..."
sleep 2

debugtool="$HAVI_ROOT/havi-devtools-cli"

output=$(printf '%s\n' \
    'typeof window.havi === "undefined"' \
    'document.getElementById("port") !== null' \
    'document.getElementById("status") !== null' \
    'document.getElementById("repoKey") !== null' \
    'document.getElementById("daemonInfoCard") !== null' \
    'document.getElementById("daemonStatus") !== null' \
    'document.getElementById("daemonUptime") !== null' \
    'document.getElementById("daemonBackend") !== null' \
    'document.getElementById("daemonVersion") !== null' \
    'document.getElementById("namedClientsList") !== null' \
    'document.getElementById("namedClientRevocations") !== null' \
    | "$debugtool" repl)

results=$(echo "$output" | jq -r 'select(.event == "evalResult") | .value')

i=0
while IFS= read -r val; do
    i=$((i + 1))
    [[ "$val" == "true" ]] || fail "Test $i failed (got: $val)"
done <<< "$results"

[[ $i -ge 11 ]] || fail "Expected 11 results, got $i"

runtime_ok=$("$debugtool" --timeout 10 eval --await \
    'fetch("havi:///home-repo/api?cmd=runtime_status").then(r => r.json()).then(j => j.ok && j.data && j.data.repoName && j.data.verifyingKey ? "ok" : "bad")' \
    2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$runtime_ok" == "ok" ]] || fail "runtime_status response wrong: $runtime_ok"

log "PASS"
