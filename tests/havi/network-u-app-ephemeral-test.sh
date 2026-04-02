#!/usr/bin/env bash
# network-u-app-ephemeral-test.sh - Public network lookup for group u must not persist a local route
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="network-u-app-ephemeral"
TEST_GROUP="u"
TEST_APP="netuapp"

start_server
start_remote_server

HOME_REPO_VKEY=$(HPPR_HOME="$HPPR_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" hello | awk -F': ' '/^Seal-By:/{print $2; exit}')
REMOTE_HOME="tcp+127.0.0.1:$REMOTE_PORT"
REMOTE_HELLO=$(HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" hello)
REMOTE_REPO_VKEY=$(awk -F': ' '/^Seal-By:/{print $2; exit}' <<<"$REMOTE_HELLO")
REMOTE_UDP_PORT=$(awk -F'udp:' '/^Transport: udp:/{print $2; exit}' <<<"$REMOTE_HELLO")

HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" ring2 setup "//u" --init
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" ring1 acl anyone add r.l "//u/$TEST_APP/"
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" ring1 acl anyone add r.l "//u/admin/deploy/"
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" add "//u/$TEST_APP/index.html" \
    -H 'Seal-By: oldest' \
    -H 'Content-Type: text/html; charset=utf-8' <<'EOF'
<!doctype html>
<title>Network U App Ephemeral</title>
<h1>Network U App Ephemeral</h1>
EOF
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" add "//u/admin/deploy/$TEST_APP" \
    -H 'Seal-By: oldest' \
    -H "Content-Root: //u/$TEST_APP" \
    -H "Content-Authority: $REMOTE_REPO_VKEY" <<< ''

ROUTE_ROOT_KEY="route-root-$TEST_NAME-$$"
"$HPPR" key generate "$ROUTE_ROOT_KEY" >/dev/null
ROUTE_ROOT_SK=$("$HPPR" key show "$ROUTE_ROOT_KEY")
ROUTE_ROOT_VK=$("$HPPR" key pubkey "$ROUTE_ROOT_KEY")

HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" route app put "//u/$TEST_APP" \
    --upstream "$REMOTE_HOME" \
    --upstream-vkey "$REMOTE_REPO_VKEY" \
    --content-authority "$REMOTE_REPO_VKEY" \
    --signing-key "$ROUTE_ROOT_SK" \
    --signer 'ring1:ring0#init' >/dev/null

HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" ring1 acl anyone add r.l "//u/route/"

export _HPPR_ROUTE_ROOT_SERVER="udp+127.0.0.1:$REMOTE_UDP_PORT"
export _HPPR_ROUTE_ROOT_PUBKEY="$ROUTE_ROOT_VK"

start_servo "hppr://u/$TEST_APP/index.html"

debugtool="$HAVI_ROOT/havi-devtools-cli"

for _ in {1..80}; do
    title=$($debugtool --timeout 5 eval 'document.title' 2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
    [[ "$title" == "Network U App Ephemeral" ]] && break
    sleep 0.1
done
[[ "$title" == "Network U App Ephemeral" ]] || fail "expected public-root app page, got title: ${title:-<none>}"

set +e
route_headers=$(HPPR_HOME="$HPPR_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" headers "//repo/route/app/u/$TEST_APP/|/seal/$HOME_REPO_VKEY" 2>&1)
route_status=$?
set -e
[[ "$route_status" -ne 0 ]] || fail "public root app navigation should not persist local route: $route_headers"
[[ "$route_headers" == *"NOT_FOUND"* ]] || fail "expected missing local route after public root app navigation, got: $route_headers"

log "PASS"
