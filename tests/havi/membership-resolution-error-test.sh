#!/usr/bin/env bash
# membership-resolution-error-test.sh - Routed MEMBERS resolution failures should show membership error page
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="membership-resolution-error"
TEST_GROUP="memberfail"
TEST_APP="site"

start_server
start_remote_server

REMOTE_HOME="tcp+127.0.0.1:$REMOTE_PORT"
REMOTE_REPO_VKEY=$(HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" hello | awk -F': ' '/^Seal-By:/{print $2; exit}')

# Remote target repo: routeable public content pointer and ring2 setup exist,
# but the membership entrypoint contains invalid Member-Delegate data so routed
# Ring2 auth fails with MEMBERS resolution failed.
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" ring2 setup "//$TEST_GROUP" --init >/dev/null
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" add "//$TEST_GROUP/admin/members" \
    -H 'Seal-By: ring0' \
    -H 'Member-Delegate: bad-delegate' <<< '' >/dev/null
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" add "//$TEST_GROUP/$TEST_APP/index.html" \
    -H 'Seal-By: ring0' \
    -H 'Content-Type: text/html; charset=utf-8' <<'EOF'
<!doctype html>
<title>Membership Failure</title>
<h1>Membership Failure</h1>
EOF
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" add "//$TEST_GROUP/admin/deploy/$TEST_APP" \
    -H 'Seal-By: ring0' \
    -H "Content-Root: //$TEST_GROUP/$TEST_APP" \
    -H "Content-Authority: $REMOTE_REPO_VKEY" <<< ''

# Home repo route points directly at the remote target repo.
HPPR_HOME="$HPPR_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" add "//repo/route/app/$TEST_GROUP/$TEST_APP" \
    -H 'Seal-By: ring0' \
    -H "Upstream: $REMOTE_HOME" \
    -H "Upstream-Verification-Key: $REMOTE_REPO_VKEY" <<< '' >/dev/null

start_servo "hppr://$TEST_GROUP/$TEST_APP/index.html"

debugtool="$HAVI_ROOT/havi-devtools-cli"

for _ in {1..80}; do
    title=$($debugtool --timeout 5 eval 'document.title' 2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
    [[ "$title" == "Route Membership Error" ]] && break
    sleep 0.1
done
[[ "$title" == "Route Membership Error" ]] || fail "expected route membership error page, got title: ${title:-<none>}"

body=$($debugtool --timeout 5 eval 'document.querySelector(".error")?.textContent || document.body.textContent' 2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$body" == *"MEMBERS resolution failed"* ]] || fail "error page missing MEMBERS detail: $body"
[[ "$body" == *"Member-Delegate"* ]] || fail "error page missing membership detail: $body"

current=$($debugtool --timeout 5 tabs 2>/dev/null | jq -r 'select(.ok == true) | .value[0].url' | tail -1)
[[ "$current" == "hppr://$TEST_GROUP/$TEST_APP/index.html" ]] || fail "expected to stay on hppr:// page, got: $current"

set +e
join_title=$($debugtool --timeout 5 eval 'document.title === "Join group - HAVI"' 2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
set -e
[[ "$join_title" != "true" ]] || fail "membership resolution failure should not redirect to join page"

log "PASS"
