#!/usr/bin/env bash
# network-route-ephemeral-test.sh - Public network lookup must not persist a local route
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="network-route-ephemeral"
TEST_GROUP="netephem"
TEST_APP="site"

start_server
start_remote_server

HOME_REPO_VKEY=$(HPPR_HOME="$HPPR_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" hello | awk -F': ' '/^Seal-By:/{print $2; exit}')
REMOTE_HOME="tcp+127.0.0.1:$REMOTE_PORT"
REMOTE_HELLO=$(HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" hello)
REMOTE_REPO_VKEY=$(awk -F': ' '/^Seal-By:/{print $2; exit}' <<<"$REMOTE_HELLO")
REMOTE_UDP_PORT=$(awk -F'udp:' '/^Transport: udp:/{print $2; exit}' <<<"$REMOTE_HELLO")

# Remote target repo: public content via anyone ACL, with Ring2 setup present so
# routed requests classify non-members as guest and fall back to anyone ACL.
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" ring2 setup "//$TEST_GROUP" --init
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" ring1 acl anyone add r.l "//$TEST_GROUP/$TEST_APP/"
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" ring1 acl anyone add r.l "//$TEST_GROUP/admin/deploy/"
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" add "//$TEST_GROUP/$TEST_APP/index.html" \
    -H 'Seal-By: oldest' \
    -H 'Content-Type: text/html; charset=utf-8' <<'EOF'
<!doctype html>
<title>Network Ephemeral</title>
<h1>Network Ephemeral</h1>
EOF
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" add "//$TEST_GROUP/admin/deploy/$TEST_APP" \
    -H 'Seal-By: oldest' \
    -H "Content-Root: //$TEST_GROUP/$TEST_APP" \
    -H "Content-Authority: $REMOTE_REPO_VKEY" <<< ''

# Publish network records: group record + app record signed by a test root key.
NETWORK_KEY="network-root-$TEST_NAME-$$"
"$HPPR" key generate "$NETWORK_KEY" >/dev/null
NETWORK_SK=$("$HPPR" key show "$NETWORK_KEY")
NETWORK_VK=$("$HPPR" key pubkey "$NETWORK_KEY")

# Group record: //u/network/group/<group>
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" network put-group "//$TEST_GROUP" \
    --upstream "$REMOTE_HOME" \
    --network-key "$NETWORK_VK" \
    --ttl 86400 \
    --upstream-vkey "$REMOTE_REPO_VKEY" \
    --signing-key "$NETWORK_SK" \
    --signer 'ring1:ring0#init' >/dev/null

# App record: //<group>/network/app/<app>
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" network put-app "//$TEST_GROUP/$TEST_APP" \
    --ttl 3600 \
    --content-authority "$REMOTE_REPO_VKEY" \
    --signing-key "$NETWORK_SK" \
    --signer 'ring1:ring0#init' >/dev/null

# Allow anyone to read network records
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" ring1 acl anyone add r.l "//u/network/"
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" ring1 acl anyone add r.l "//$TEST_GROUP/network/"

export _HPPR_NETWORK_ROOT_SERVER="udp+127.0.0.1:$REMOTE_UDP_PORT"
export _HPPR_NETWORK_ROOT_PUBKEY="$NETWORK_VK"

start_servo "hppr://$TEST_GROUP/$TEST_APP/index.html"

debugtool="$HAVI_ROOT/havi-devtools-cli"

for _ in {1..80}; do
    title=$($debugtool --timeout 5 eval 'document.title' 2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
    [[ "$title" == "Network Ephemeral" ]] && break
    sleep 0.1
done
[[ "$title" == "Network Ephemeral" ]] || fail "expected network-resolved page, got title: ${title:-<none>}"

set +e
route_headers=$(HPPR_HOME="$HPPR_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" headers "//repo/admin/route/$TEST_GROUP/$TEST_APP/|/seal/$HOME_REPO_VKEY" 2>&1)
route_status=$?
set -e
[[ "$route_status" -ne 0 ]] || fail "network navigation should not persist local route: $route_headers"
[[ "$route_headers" == *"NOT_FOUND"* ]] || fail "expected missing local route after network navigation, got: $route_headers"

log "PASS"
