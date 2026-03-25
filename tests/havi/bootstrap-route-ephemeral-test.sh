#!/usr/bin/env bash
# bootstrap-route-ephemeral-test.sh - Bootstrap lookup must not persist a local route
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="bootstrap-route-ephemeral"
TEST_GROUP="bootephem"
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
<title>Bootstrap Ephemeral</title>
<h1>Bootstrap Ephemeral</h1>
EOF
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" add "//$TEST_GROUP/admin/deploy/$TEST_APP" \
    -H 'Seal-By: oldest' \
    -H "Content-Root: //$TEST_GROUP/$TEST_APP" \
    -H "Content-Authority: $REMOTE_REPO_VKEY" <<< ''

# Publish a bootstrap index entry signed by a test bootstrap key.
BOOTSTRAP_KEY="bootstrap-index-$TEST_NAME-$$"
"$HPPR" key generate "$BOOTSTRAP_KEY" >/dev/null
BOOTSTRAP_SK=$("$HPPR" key show "$BOOTSTRAP_KEY")
BOOTSTRAP_VK=$("$HPPR" key pubkey "$BOOTSTRAP_KEY")
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" index put "//$TEST_GROUP/$TEST_APP" \
    --upstream "$REMOTE_HOME" \
    --upstream-vkey "$REMOTE_REPO_VKEY" \
    --signing-key "$BOOTSTRAP_SK" \
    --signer 'ring1:ring0#init' >/dev/null

export _HPPR_INDEX_SERVER="udp+127.0.0.1:$REMOTE_UDP_PORT"
export _HPPR_INDEX_PUBKEY="$BOOTSTRAP_VK"

start_servo "hppr://$TEST_GROUP/$TEST_APP/index.html"

debugtool="$HAVI_ROOT/havi-devtools-cli"

for _ in {1..80}; do
    title=$($debugtool --timeout 5 eval 'document.title' 2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
    [[ "$title" == "Bootstrap Ephemeral" ]] && break
    sleep 0.1
done
[[ "$title" == "Bootstrap Ephemeral" ]] || fail "expected bootstrap-resolved page, got title: ${title:-<none>}"

set +e
route_headers=$(HPPR_HOME="$HPPR_HOME" HPPR_SIGNER='ring1:ring0#init' "$HPPR" headers "//repo/admin/route/$TEST_GROUP/$TEST_APP/|/seal/$HOME_REPO_VKEY" 2>&1)
route_status=$?
set -e
[[ "$route_status" -ne 0 ]] || fail "bootstrap navigation should not persist local route: $route_headers"
[[ "$route_headers" == *"NOT_FOUND"* ]] || fail "expected missing local route after bootstrap navigation, got: $route_headers"

log "PASS"
