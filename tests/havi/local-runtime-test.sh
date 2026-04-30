#!/usr/bin/env bash
# local-runtime-test.sh - default browser-local runtime path without pylon or HAVI_HOME
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="local-runtime"

HAVI_CONFIG_ROOT=$(mktemp -d)
TEMP_REPO="$HAVI_CONFIG_ROOT"
export HAVI_CONFIG="$HAVI_CONFIG_ROOT/havi-config"
mkdir -p "$HAVI_CONFIG"

havi_bin="$HAVI_ROOT/target/debug/havi"
debugtool="$HAVI_ROOT/havi-devtools-cli"
DEVTOOLS_PORT=$(get_port)
export HAVI_DEVTOOLS="127.0.0.1:$DEVTOOLS_PORT"

log "Starting HAVI in browser-local runtime mode..."
cd "$HAVI_ROOT"
setsid env \
    HAVI_URL='havi:///home-repo' \
    HAVI_CONFIG="$HAVI_CONFIG" \
    HAVI_DEVTOOLS="$HAVI_DEVTOOLS" \
    "$havi_bin" --no-pylon &
SERVO_PID=$!

wait_for_devtools

"$debugtool" --text wait-for 'document.getElementById("namedClientsList") !== null' >/dev/null

runtime_mode=$($debugtool --timeout 10 eval --await '
(async () => {
    const resp = await fetch("havi:///home-repo/api?cmd=runtime_status");
    const json = await resp.json();
    if (!json.ok) return String(json.error || "runtime-status-failed");
    if (json.data.mode !== "local") return "bad-mode:" + json.data.mode;
    if (!(json.data.packetStorePath || "").endsWith("havi-packets.sqlite")) return "bad-path";
    if (!json.data.verifyingKey) return "missing-key";
    return "ok";
})()' 2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$runtime_mode" == "ok" ]] || fail "local runtime status check failed: $runtime_mode"

seed_result=$($debugtool --timeout 10 eval --await '
(async () => {
    const q = new URLSearchParams({
        cmd: "local_add",
        group: "~localruntime",
        app: "app",
        location: "index.html",
        content_type: "text/html; charset=utf-8",
        data: "<!doctype html><title>Local Runtime Test</title><h1>Local Runtime Test</h1>"
    });
    const resp = await fetch("havi:///home-repo/api?" + q.toString());
    const json = await resp.json();
    return json.ok ? "ok" : String(json.error || "local-add-failed");
})()' 2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$seed_result" == "ok" ]] || fail "local runtime seed failed: $seed_result"

"$debugtool" --text navigate 'hppr://~localruntime/app/index.html' >/dev/null
"$debugtool" --text wait-for 'document.title === "Local Runtime Test"' >/dev/null

client_result=$($debugtool --timeout 10 eval --await '
(async () => {
    const source = window.source;
    if (!source) return "missing-source";
    if (source.kind !== "repo") return "bad-kind:" + source.kind;
    const hashes = await source.client.add({
        headers: ["Location: user/from-client.txt", "Content-Type: text/plain"],
        data: "browser local ok"
    });
    if (!Array.isArray(hashes) || hashes.length < 1) return "bad-add";
    const packet = await source.client.get("//~localruntime/app/user/from-client.txt");
    const text = await packet.text();
    return text.trim() === "browser local ok" ? "ok" : "bad-readback";
})()' 2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$client_result" == "ok" ]] || fail "local committed-source client failed: $client_result"

log "PASS"
