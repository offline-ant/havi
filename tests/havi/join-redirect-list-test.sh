#!/usr/bin/env bash
# join-redirect-list-test.sh - Routed LIST unauthorized should redirect to hppr-join://
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="join-redirect-list"
TEST_GROUP="joinredir"
TEST_APP="listapp"

# Local home repo
start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key

# Remote content repo
start_remote_server
create_remote_key

# Seed remote content without NFS mount (Linux QA path should avoid NFS).
HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='ring1:ring0#init' \
    $HPPR add "//$TEST_GROUP/$TEST_APP/index.html" \
    -H "Content-Type: text/html" <<< "<html><body>join redirect test</body></html>"

# Ring2 enabled on remote, but intentionally no members added.
HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='ring1:ring0#init' \
    $HPPR ring2 setup "//$TEST_GROUP" --init
HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='ring1:ring0#init' \
    $HPPR ring2 setup "//$TEST_GROUP" acl add r.l "//$TEST_GROUP/$TEST_APP/"

# Deploy + route so hppr://group/app/ resolves to remote non-repo content.
setup_remote_deploy "$TEST_GROUP" "$TEST_APP"
setup_route "$TEST_GROUP" "$TEST_APP"

start_servo "hppr://$TEST_GROUP/$TEST_APP/"

debugtool="$HAVI_ROOT/havi-devtools-cli"

for _ in {1..80}; do
    current=$($debugtool --timeout 5 tabs 2>/dev/null | jq -r 'select(.ok == true) | .value[0].url' | tail -1)
    if [[ "$current" == "hppr-join://$TEST_GROUP/$TEST_APP/" ]]; then
        break
    fi
    sleep 0.1
done

[[ "$current" == "hppr-join://$TEST_GROUP/$TEST_APP/" ]] || fail "expected hppr-join redirect, got: $current"

join_btn=$($debugtool --timeout 5 eval 'Boolean(document.getElementById("join-btn"))' 2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
route_vkey=$($debugtool --timeout 5 eval 'Boolean(document.getElementById("route-vkey"))' 2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)

[[ "$join_btn" == "true" ]] || fail "join page missing #join-btn"
[[ "$route_vkey" == "true" ]] || fail "join page missing #route-vkey"

log "PASS"
