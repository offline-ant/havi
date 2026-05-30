#!/usr/bin/env bash
# named-client-test.sh - browser-mediated named client grants and revocation
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="named-client"
TEST_GROUP="~namedclienttest"
TEST_API="testapp"

start_server
HPPR_SIGNER='ring1:ring0|init' "$HPPR" ring1 acl anyone add r.. "//$TEST_GROUP/$TEST_API//"
HPPR_SIGNER='ring1:ring0|init' "$HPPR" add "//$TEST_GROUP/$TEST_API//index.html" \
    -H 'Seal-By: ring0' \
    -H 'Content-Type: text/html; charset=utf-8' <<'EOF'
<!doctype html>
<title>Named Client Test</title>
<h1>Named Client Test</h1>
EOF

start_servo "hppr://$TEST_GROUP/$TEST_API//index.html"
debugtool="$HAVI_ROOT/havi-devtools-cli"

page_scope=$($debugtool --text eval 'window.address.href' 2>/dev/null || true)
[[ -n "$page_scope" ]] || fail "Could not read page URL"

unauthorized=$($debugtool --timeout 10 eval --await '
(async () => {
    try {
        await HpprClient.named("writer");
        return "unexpected";
    } catch (_e) {
        return "denied";
    }
})()' 2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$unauthorized" == "denied" ]] || fail "HpprClient.named should reject before grant: $unauthorized"

"$debugtool" --text navigate 'havi:///home-repo' >/dev/null
"$debugtool" --text wait-for 'document.getElementById("namedClientsList") !== null' >/dev/null

set_result=$($debugtool --timeout 10 eval --await "
(async () => {
    const q = new URLSearchParams({
        cmd: 'set_named_client',
        name: 'writer',
        endpoint: 'tcp+127.0.0.1:$HPPR_PORT',
        signer: 'ring1:ring0|init'
    });
    const resp = await fetch('havi:///home-repo/api?' + q.toString());
    const json = await resp.json();
    return json.ok ? 'ok' : String(json.error || 'set failed');
})()" 2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$set_result" == "ok" ]] || fail "Failed to create named client: $set_result"

grant_result=$($debugtool --timeout 10 eval --await "
(async () => {
    const q = new URLSearchParams({
        cmd: 'grant_named_client',
        name: 'writer',
        origin: '$page_scope'
    });
    const resp = await fetch('havi:///home-repo/api?' + q.toString());
    const json = await resp.json();
    return json.ok ? 'ok' : String(json.error || 'grant failed');
})()" 2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$grant_result" == "ok" ]] || fail "Failed to grant named client: $grant_result"

list_result=$($debugtool --timeout 10 eval --await '
(async () => {
    const resp = await fetch("havi:///home-repo/api?cmd=named_clients");
    const json = await resp.json();
    if (!json.ok) return String(json.error || "list failed");
    const hasClient = json.data.clients.some(c => c.name === "writer");
    const hasGrant = json.data.grants.some(g => g.client_name === "writer");
    return hasClient && hasGrant ? "ok" : "missing";
})()' 2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$list_result" == "ok" ]] || fail "Named client list did not show saved client + grant: $list_result"

"$debugtool" --text navigate "hppr://$TEST_GROUP/$TEST_API//index.html" >/dev/null
"$debugtool" --text wait-for 'document.title === "Named Client Test"' >/dev/null

use_result=$($debugtool --timeout 10 eval --await '
(async () => {
    const client = await HpprClient.named("writer");
    const hashes = await client.add({
        headers: ["Key: user/named-client.txt"],
        data: "named client ok"
    });
    if (!Array.isArray(hashes) || hashes.length < 1) return "bad-add";
    const packet = await window.source.client.get("//~namedclienttest/testapp//user/named-client.txt");
    return (await packet.text()).trim() === "named client ok" ? "ok" : "bad-readback";
})()' 2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$use_result" == "ok" ]] || fail "Named client use failed: $use_result"

"$debugtool" --text navigate 'havi:///home-repo' >/dev/null
"$debugtool" --text wait-for 'document.getElementById("namedClientsList") !== null' >/dev/null

revoke_result=$($debugtool --timeout 10 eval --await "
(async () => {
    const q = new URLSearchParams({
        cmd: 'revoke_named_client',
        name: 'writer',
        origin: '$page_scope'
    });
    const resp = await fetch('havi:///home-repo/api?' + q.toString());
    const json = await resp.json();
    return json.ok ? 'ok' : String(json.error || 'revoke failed');
})()" 2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$revoke_result" == "ok" ]] || fail "Failed to revoke named client: $revoke_result"

"$debugtool" --text navigate "hppr://$TEST_GROUP/$TEST_API//index.html" >/dev/null
"$debugtool" --text wait-for 'document.title === "Named Client Test"' >/dev/null

denied_after_revoke=$($debugtool --timeout 10 eval --await '
(async () => {
    try {
        await HpprClient.named("writer");
        return "unexpected";
    } catch (_e) {
        return "denied";
    }
})()' 2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$denied_after_revoke" == "denied" ]] || fail "HpprClient.named should reject after revocation: $denied_after_revoke"

log "PASS"
