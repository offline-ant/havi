#!/usr/bin/env bash
# network-route-ephemeral-test.sh - Public network lookup must not persist a local route
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="network-route-ephemeral"
TEST_GROUP="netephem"
TEST_API="site"

start_server
start_remote_server

HOME_REPO_VKEY=$(HPPR_HOME="$HPPR_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" hello | awk -F': ' '/^Seal-By:/{print $2; exit}')
REMOTE_HOME="tcp+127.0.0.1:$REMOTE_PORT"
REMOTE_HELLO=$(HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" hello)
REMOTE_REPO_VKEY=$(awk -F': ' '/^Seal-By:/{print $2; exit}' <<<"$REMOTE_HELLO")
REMOTE_UDP_PORT=$(awk -F'udp:' '/^Transport: udp:/{print $2; exit}' <<<"$REMOTE_HELLO")

# Remote target repo: public content via anyone ACL, with Ring2 setup present so
# routed requests classify non-members as guest and fall back to anyone ACL.
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" ring2 setup "//$TEST_GROUP" --init
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" ring1 acl anyone add r.l "//$TEST_GROUP/$TEST_API//"
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" ring1 acl anyone add r.l "//$TEST_GROUP/admin/deploy//"
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" add "//$TEST_GROUP/$TEST_API//index.html" \
    -H 'Seal-By: ring0' \
    -H 'Content-Type: text/html; charset=utf-8' <<'EOF'
<!doctype html>
<title>Network Ephemeral</title>
<h1>Network Ephemeral</h1>
EOF
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" add "//$TEST_GROUP/admin/deploy//$TEST_API" \
    -H 'Seal-By: ring0' \
    -H "Content-Root: //$TEST_GROUP/$TEST_API//" \
    -H "Content-Authority: $REMOTE_REPO_VKEY" <<< ''

# Publish route records: group record + API record signed by a test route root key.
ROUTE_ROOT_KEY="route-root-$TEST_NAME-$$"
"$HPPR" key generate "$ROUTE_ROOT_KEY" >/dev/null
ROUTE_ROOT_SK=$("$HPPR" key show "$ROUTE_ROOT_KEY")
ROUTE_ROOT_VK=$("$HPPR" key pubkey "$ROUTE_ROOT_KEY")

# Group record: //u/route/group//<group>
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" route group put "//$TEST_GROUP" \
    --upstream "$REMOTE_HOME" \
    --route-authority "$ROUTE_ROOT_VK" \
    --upstream-verifier "$REMOTE_REPO_VKEY" \
    --signing-secret "$ROUTE_ROOT_SK" >/dev/null

# API record: //<group>/route/api//<api>
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" route api put "//$TEST_GROUP/$TEST_API//" \
    --content-authority "$REMOTE_REPO_VKEY" \
    --signing-secret "$ROUTE_ROOT_SK" >/dev/null

# Allow anyone to read network records
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" ring1 acl anyone add r.l "//u/route/"
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" ring1 acl anyone add r.l "//$TEST_GROUP/route/"

export _HPPR_ROUTE_ROOT_SERVER="udp+127.0.0.1:$REMOTE_UDP_PORT"
export _HPPR_ROUTE_ROOT_PUBKEY="$ROUTE_ROOT_VK"

start_servo "hppr://$TEST_GROUP/$TEST_API//index.html"

debugtool="$HAVI_ROOT/havi-devtools-cli"

for _ in {1..80}; do
    title=$($debugtool --timeout 5 eval 'document.title' 2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
    [[ "$title" == "Network Ephemeral" ]] && break
    sleep 0.1
done
[[ "$title" == "Network Ephemeral" ]] || fail "expected network-resolved page, got title: ${title:-<none>}"

set +e
route_headers=$(HPPR_HOME="$HPPR_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" headers "//repo/route/api//$TEST_GROUP/$TEST_API/|/seal/$HOME_REPO_VKEY" 2>&1)
route_status=$?
set -e
[[ "$route_status" -ne 0 ]] || fail "network navigation should not persist local route: $route_headers"
[[ "$route_headers" == *"NOT_FOUND"* ]] || fail "expected missing local route after network navigation, got: $route_headers"

log "PASS"
