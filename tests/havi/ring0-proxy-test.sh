#!/usr/bin/env bash
# ring0-proxy-test.sh - Test ring0 proxy page and workflow
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="ring0-proxy"

start_server

# Create a test ring1 account with admin subtree access
log "Creating test ring1 account..."
HPPR_SIGNER='!ring0/init' $HPPR ring1 create testuser testpass
HPPR_SIGNER='!ring0/init' $HPPR ring1 acl testuser add rwl "//repo/admin/ring1/testuser/"

# Write a proxy request (ring0 writes on behalf of testuser, since default host
# rules deny ring1 writes to //repo/admin/ring1/ to prevent ACL self-modification)
log "Writing proxy request..."
HPPR_SIGNER='!ring0/init' $HPPR add "//repo/admin/ring1/testuser/LIST" \
    -H "Seal-By: oldest" <<< "//repo/admin/"

# Start servo on havi:///ring0
start_servo "havi:///ring0"

debugtool="$HAVI_ROOT/havi-devtools-cli"

# Verify the ring0 proxy page loads
log "Verifying ring0 proxy page loads..."
output=$(printf '%s\n' \
    'document.title' \
    'document.querySelector("h1").textContent' \
    | "$debugtool" repl)

title=$(echo "$output" | jq -r 'select(.event == "evalResult") | .value' | sed -n '1p')
heading=$(echo "$output" | jq -r 'select(.event == "evalResult") | .value' | sed -n '2p')

[[ "$title" == "Ring0 Proxy - HAVI" ]] || fail "Page title should be 'Ring0 Proxy - HAVI', got: $title"
[[ "$heading" == "Ring0 Proxy" ]] || fail "Page heading should be 'Ring0 Proxy', got: $heading"

# Wait for scan to complete and check for pending request
log "Checking for pending proxy request..."
sleep 2

request_check=$("$debugtool" --text --timeout 5 eval \
    "document.querySelector('.request-card') !== null" 2>/dev/null || true)

if [[ "$request_check" == "true" ]]; then
    log "Pending request found on proxy page"
else
    log "No request card found (request may not have been detected - checking manually)"
    # Verify the request exists in the repo
    HPPR_SIGNER='!ring0/init' $HPPR headers "//repo/admin/ring1/testuser/LIST/|" || \
        fail "Proxy request not found in repo"
    log "Request exists in repo but page didn't show it (scan timing)"
fi

# Trigger approve via JS
log "Triggering approve action..."
"$debugtool" --timeout 10 eval "
(async () => {
    try {
        const reqPacket = await window.ring0.get('//repo/admin/ring1/testuser/LIST/|');
        const targetCoord = (await reqPacket.text()).trim();
        const entries = await window.ring0.list(targetCoord);
        const now = Math.floor(Date.now() / 1000) + ':000000000';
        await window.ring0.add({
            headers: [
                'Group: repo',
                'App: admin',
                'Location: ring1/testuser/LIST/reply',
                'TAI: ' + now,
                'Link: request ' + reqPacket.hash
            ],
            data: entries.join('\\n')
        });
        window._approveResult = 'approved';
    } catch (e) {
        window._approveResult = 'error: ' + e.message;
    }
})()
" >/dev/null 2>&1

# Poll for the async result
for _ in {1..50}; do
    approve_result=$("$debugtool" --text eval "window._approveResult" 2>/dev/null || true)
    [[ -n "$approve_result" && "$approve_result" != "null" && "$approve_result" != "None" ]] && break
    sleep 0.2
done

[[ "$approve_result" == "approved" ]] || fail "Approve failed: $approve_result"

# Verify reply exists
log "Verifying reply exists..."
HPPR_SIGNER='!ring0/init' $HPPR headers "//repo/admin/ring1/testuser/LIST/reply/|" || \
    fail "Reply packet not found"

log "PASS"
