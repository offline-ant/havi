#!/usr/bin/env bash
# missing-ring2-setup-error-test.sh - Routed repos without Ring2 setup should show setup error, not join flow
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="missing-ring2-setup-error"
TEST_GROUP="nor2setup"
TEST_API="site"

start_server
start_remote_server

REMOTE_HOME="tcp+127.0.0.1:$REMOTE_PORT"
REMOTE_REPO_VKEY=$(HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" hello | awk -F': ' '/^Seal-By:/{print $2; exit}')
HOME_REPO_VKEY=$(HPPR_HOME="$HPPR_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" hello | awk -F': ' '/^Seal-By:/{print $2; exit}')

# Remote target repo: public content + deploy pointer, but intentionally no
# Ring2 setup for the group.
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" ring1 acl anyone add r.l "//$TEST_GROUP/$TEST_API//"
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" ring1 acl anyone add r.l "//$TEST_GROUP/admin/deploy//"
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" add "//$TEST_GROUP/$TEST_API//index.html" \
    -H 'Seal-By: ring0' \
    -H 'Content-Type: text/html; charset=utf-8' <<'EOF'
<!doctype html>
<title>Missing Ring2 Setup</title>
<h1>Missing Ring2 Setup</h1>
EOF
HPPR_HOME="$REMOTE_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" add "//$TEST_GROUP/admin/deploy//$TEST_API" \
    -H 'Seal-By: ring0' \
    -H "Content-Root: //$TEST_GROUP/$TEST_API//" \
    -H "Content-Authority: $REMOTE_REPO_VKEY" <<< ''

# Home repo route points directly at the remote target repo.
HPPR_HOME="$HPPR_HOME" HPPR_SIGNER='ring1:ring0|init' "$HPPR" add "//repo/route/api//$TEST_GROUP/$TEST_API" \
    -H 'Seal-By: ring0' \
    -H "Upstream: $REMOTE_HOME" \
    -H "Upstream-Verifier: $REMOTE_REPO_VKEY" <<< '' >/dev/null

start_servo "hppr://$TEST_GROUP/$TEST_API//index.html"

debugtool="$HAVI_ROOT/havi-devtools-cli"

for _ in {1..80}; do
    title=$($debugtool --timeout 5 eval 'document.title' 2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
    [[ -n "$title" ]] || true
    [[ "$title" == "Route Setup Error" ]] && break
    sleep 0.1
done
[[ "$title" == "Route Setup Error" ]] || fail "expected route setup error page, got title: ${title:-<none>}"

body=$($debugtool --timeout 5 eval 'document.querySelector(".error")?.textContent || document.body.textContent' 2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
[[ "$body" == *"NOT_FOUND ring2 setup '$TEST_GROUP'"* ]] || fail "error page missing ring2 setup detail: $body"

current=$($debugtool --timeout 5 tabs 2>/dev/null | jq -r 'select(.ok == true) | .value[0].url' | tail -1)
[[ "$current" == "hppr://$TEST_GROUP/$TEST_API//index.html" ]] || fail "expected to stay on hppr:// page, got: $current"

set +e
join_title=$($debugtool --timeout 5 eval 'document.title === "Join group - HAVI"' 2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1)
set -e
[[ "$join_title" != "true" ]] || fail "missing Ring2 setup should not redirect to join page"

log "PASS"
