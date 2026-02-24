#!/usr/bin/env bash
# services-test.sh - Test havi:///services page and API surface
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="services"

start_server
start_servo "havi:///services"

log "Waiting for services page to load..."
sleep 2

debugtool="$HAVI_ROOT/havi-devtools-cli"

output=$(printf '%s\n' \
    'document.title' \
    'document.querySelector("h1").textContent' \
    'document.getElementById("servicesList") !== null' \
    'document.getElementById("listenersList") !== null' \
    'document.getElementById("mountsList") !== null' \
    'document.getElementById("natInfo") !== null' \
    | "$debugtool" repl)

results=$(echo "$output" | jq -r 'select(.event == "evalResult") | .value')

[[ "$(echo "$results" | sed -n '1p')" == "Services - HAVI" ]] || fail "services page title mismatch"
[[ "$(echo "$results" | sed -n '2p')" == "Services" ]] || fail "services page heading mismatch"
[[ "$(echo "$results" | sed -n '3p')" == "true" ]] || fail "servicesList missing"
[[ "$(echo "$results" | sed -n '4p')" == "true" ]] || fail "listenersList missing"
[[ "$(echo "$results" | sed -n '5p')" == "true" ]] || fail "mountsList missing"
[[ "$(echo "$results" | sed -n '6p')" == "true" ]] || fail "natInfo missing"

status_ok=$("$debugtool" --timeout 10 eval --await \
    'fetch("havi:///services/api?cmd=status").then(r => r.json()).then(j => !!j && typeof j === "object" && ((j.ok === true && !!j.data && typeof j.data === "object") || (j.ok === false && typeof j.error === "string")))' \
    2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$status_ok" == "true" ]] || fail "services status api failed"

mounts_ok=$("$debugtool" --timeout 10 eval --await \
    'fetch("havi:///services/api?cmd=mounts").then(r => r.json()).then(j => !!j && typeof j === "object" && ((j.ok === true && Array.isArray(j.data)) || (j.ok === false && typeof j.error === "string")))' \
    2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$mounts_ok" == "true" ]] || fail "services mounts api failed"

list_ok=$("$debugtool" --timeout 10 eval --await \
    'fetch("havi:///services/api?cmd=list").then(r => r.json()).then(j => !!j && typeof j === "object" && ((j.ok === true && j.data && Array.isArray(j.data.services)) || (j.ok === false && typeof j.error === "string")))' \
    2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$list_ok" == "true" ]] || fail "services list api failed"

listen_cmd_ok=$("$debugtool" --timeout 10 eval --await \
    'fetch("havi:///services/api?cmd=listen").then(r => r.json()).then(j => j.ok === false && !String(j.error || "").includes("unknown command"))' \
    2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$listen_cmd_ok" == "true" ]] || fail "listen command not recognized"

log "PASS"
