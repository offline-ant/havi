#!/usr/bin/env bash
# test-prelude.bash - Minimal test infrastructure
#
# Provides: repo daemon lifecycle, port allocation, run_js_tests
# All assertions are in JavaScript (test.js) - bash only handles infrastructure.
#
# shellcheck disable=SC2034

set -euo pipefail

# ============================================================================
# Path Resolution
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[1]}")" && pwd)"
if [[ -n "${FORGE_ROOT:-}" ]]; then
    HPPR_ROOT="$FORGE_ROOT/hppr"
    HAVI_ROOT="$FORGE_ROOT/havi"
else
    HPPR_ROOT="$(cd "$SCRIPT_DIR/../../../hppr" && pwd)"
    HAVI_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
    FORGE_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
fi

# Build hppr and hpprd (quiet, only rebuilds if needed)
cargo build -q --manifest-path "$HPPR_ROOT/cli/Cargo.toml" --bin hppr
cargo build -q --manifest-path "$HPPR_ROOT/hpprd/Cargo.toml" --bin hpprd

# hppr binaries from cargo build, shell tools from hppr/bin/
export PATH="$HPPR_ROOT/target/debug:$HPPR_ROOT/bin:$PATH"
HPPR="$HPPR_ROOT/target/debug/hppr"
HPPR_FS="$HPPR_ROOT/target/debug/hppr-nfs"

echo "--- CLI versions ---"
"$HAVI_ROOT/target/debug/havi" --version || true
hppr --version
hpprd --version
echo "---"

# ============================================================================
# State Variables
# ============================================================================

TEMP_REPO=""
HPPRD_PID=""
SERVO_PID=""
HPPR_PORT=""
DEVTOOLS_PORT=""
SECRET_KEY=""
SIGNING_KEY=""

REMOTE_REPO=""
REMOTE_PORT=""
REMOTE_HPPRD_PID=""
REMOTE_SECRET_KEY=""
REMOTE_SIGNING_KEY=""

# Temp dir for key storage (cleaned up on exit)
HPPR_CONFIG_DIR=""

# ============================================================================
# Cleanup
# ============================================================================

stop_pid() {
    local pid="$1"
    [[ -z "$pid" ]] && return
    if kill -0 "$pid" 2>/dev/null; then
        # First try process-group shutdown (for setsid + child browser trees)
        kill -- "-$pid" 2>/dev/null || true
        kill "$pid" 2>/dev/null || true
        for _ in {1..50}; do
            kill -0 "$pid" 2>/dev/null || break
            sleep 0.1
        done
        if kill -0 "$pid" 2>/dev/null; then
            kill -9 -- "-$pid" 2>/dev/null || true
            kill -9 "$pid" 2>/dev/null || true
        fi
    fi
}

cleanup() {
    local exit_code=$?
    stop_pid "${SERVO_PID:-}"
    # Shut down pylon cleanly (stops satellites, exits immediately)
    if [[ -n "${HAVI_CONFIG:-}" && -f "$HAVI_CONFIG/repo/pylon.pid" ]]; then
        local pylon_port
        pylon_port=$(awk '{print $2}' "$HAVI_CONFIG/repo/pylon.pid" 2>/dev/null || true)
        [[ -n "$pylon_port" ]] && echo '{"id":1,"cmd":"shutdown"}' | nc -q0 127.0.0.1 "$pylon_port" 2>/dev/null || true
    fi
    stop_pid "${HPPRD_PID:-}"
    stop_pid "${REMOTE_HPPRD_PID:-}"
    [[ -n "${TEMP_REPO:-}" && -d "$TEMP_REPO" ]] && rm -rf "$TEMP_REPO"
    [[ -n "${REMOTE_REPO:-}" && -d "$REMOTE_REPO" ]] && rm -rf "$REMOTE_REPO"
    [[ -n "${HPPR_CONFIG_DIR:-}" && -d "$HPPR_CONFIG_DIR" ]] && rm -rf "$HPPR_CONFIG_DIR"
    exit $exit_code
}
trap cleanup EXIT INT TERM

# ============================================================================
# Logging
# ============================================================================

log() { echo "[${TEST_NAME:-test}] $*" >&2; }
fail() { echo "FAIL: $*" >&2; exit 1; }

# ============================================================================
# Port Allocation
# ============================================================================

get_port() {
    local port
    for _ in {1..100}; do
        port=$(shuf -i 10000-60000 -n 1)
        nc -z 127.0.0.1 "$port" 2>/dev/null || { echo "$port"; return; }
    done
    fail "No available port"
}

# Parse TCP address from HPPRD_LISTEN= in hpprd stdout file.
# hpprd prints "HPPRD_LISTEN=tcp+host:port,..." on startup.
read_bind_addr() {
    local stdout_file="$1" max="${2:-50}"
    for _ in $(seq 1 "$max"); do
        if grep -q '^HPPRD_LISTEN=' "$stdout_file" 2>/dev/null; then
            grep '^HPPRD_LISTEN=' "$stdout_file" | sed -n 's/.*tcp+\([^,]*\).*/\1/p'
            return 0
        fi
        sleep 0.1
    done
    return 1
}

# ============================================================================
# Repo Daemon Setup
# ============================================================================

start_server() {
    TEMP_REPO=$(mktemp -d)
    local stdout_file="$TEMP_REPO/hpprd.stdout"
    log "Starting hpprd..."

    RUST_LOG=warn hpprd --path "$TEMP_REPO" --bind "127.0.0.1:0" --phc "\$argon2id\$v=19\$m=8,t=1,p=1\$" > "$stdout_file" &
    HPPRD_PID=$!

    local bind_addr
    bind_addr=$(read_bind_addr "$stdout_file") || fail "hpprd failed to start"

    # HAVI_HOME selects remote mode (pylon connects to this hpprd).
    # HAVI_CONFIG isolates config/pylon state per test.
    export HAVI_HOME="tcp+$bind_addr"
    export HAVI_CONFIG="$TEMP_REPO/havi-config"
    export HPPR_HOME="tcp+$bind_addr"
    HPPR_PORT="${bind_addr##*:}"

    # Set up temp key storage
    HPPR_CONFIG_DIR=$(mktemp -d)
    export HPPR_CONFIG="$HPPR_CONFIG_DIR"

    log "hpprd ready at $bind_addr (PID: $HPPRD_PID)"
}

start_remote_server() {
    REMOTE_REPO=$(mktemp -d)
    local stdout_file="$REMOTE_REPO/hpprd.stdout"
    log "Starting remote hpprd..."

    RUST_LOG=warn hpprd --path "$REMOTE_REPO" --bind "127.0.0.1:0" --phc "\$argon2id\$v=19\$m=8,t=1,p=1\$" > "$stdout_file" &
    REMOTE_HPPRD_PID=$!

    local bind_addr
    bind_addr=$(read_bind_addr "$stdout_file") || fail "Remote hpprd failed to start"
    REMOTE_PORT="${bind_addr##*:}"

    log "Remote hpprd ready at $bind_addr (PID: $REMOTE_HPPRD_PID)"
}

# ============================================================================
# HPPR CLI Helpers
# ============================================================================

# Generate a key pair, store in SECRET_KEY and SIGNING_KEY
create_key() {
    local keyname="testkey-$$"
    $HPPR key generate "$keyname" >/dev/null
    SECRET_KEY=$($HPPR key show "$keyname")
    SIGNING_KEY=$($HPPR key pubkey "$keyname")
}

# Generate a remote key pair, store in REMOTE_SECRET_KEY and REMOTE_SIGNING_KEY
create_remote_key() {
    local keyname="remotekey-$$"
    $HPPR key generate "$keyname" >/dev/null
    REMOTE_SECRET_KEY=$($HPPR key show "$keyname")
    REMOTE_SIGNING_KEY=$($HPPR key pubkey "$keyname")
}

# Set up ACL for group/app (allows anyone access)
# Uses ring1 anyone ACL rules for unauthenticated access
setup_acl() {
    local group="$1" app="$2" perms="${3:-rwl}"
    HPPR_SIGNER='!ring0/init' $HPPR ring1 acl anyone add "$perms" "//$group/$app/"
}

# Set up ACL on remote repo
setup_remote_acl() {
    local group="$1" app="$2" perms="${3:-r.l}"
    HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='!ring0/init' \
        $HPPR ring1 acl anyone add "$perms" "//$group/$app/"
}

# Start hppr-nfs, mount, and export FS_MNT / FS_PID / FS_PORT.
# Call fs_unmount to clean up.
# Usage: fs_mount <home> <signer> <root> [--seal-with <key>]
fs_mount() {
    local home="$1" signer="$2" root="$3"
    shift 3

    FS_PORT=$(_pick_port)
    FS_MNT=$(mktemp -d)

    local -a fs_args=(
        --home "$home" --signer "$signer"
        --root "$root" --bind "127.0.0.1:$FS_PORT"
    )
    if [[ $# -gt 0 && "$1" == "--seal-with" ]]; then
        fs_args+=(--rw --seal-with "$2")
    fi

    "$HPPR_FS" "${fs_args[@]}" &
    FS_PID=$!

    local i=0
    while ! nc -z 127.0.0.1 "$FS_PORT" 2>/dev/null; do
        sleep 0.1
        ((i++))
        if ((i > 50)); then
            kill "$FS_PID" 2>/dev/null || true
            rm -rf "$FS_MNT"
            echo "ERROR: hppr-nfs did not start on port $FS_PORT" >&2
            return 1
        fi
    done

    sudo mount -t nfs -o "port=$FS_PORT,mountport=$FS_PORT,nfsvers=3,tcp,nolock" \
        "127.0.0.1:/" "$FS_MNT"
}

# Unmount and stop hppr-nfs started by fs_mount.
fs_unmount() {
    sudo umount "$FS_MNT" 2>/dev/null || true
    kill "$FS_PID" 2>/dev/null || true
    wait "$FS_PID" 2>/dev/null || true
    rm -rf "$FS_MNT"
}

# Pick an unused TCP port.
_pick_port() {
    python3 -c 'import socket; s=socket.socket(); s.bind(("",0)); print(s.getsockname()[1]); s.close()'
}

# Import content directory as sealed packets via hppr-nfs mount + cp.
import_content() {
    local content_dir="$1" group="$2" app="$3"
    fs_mount "$HPPR_HOME" "!ring0/init" "//$group/$app" --seal-with oldest
    cp -a "$content_dir/." "$FS_MNT/"
    fs_unmount
}

# Import content to remote repo via hppr-nfs mount + cp.
import_remote_content() {
    local content_dir="$1" group="$2" app="$3"
    fs_mount "tcp+127.0.0.1:$REMOTE_PORT" "!ring0/init" "//$group/$app" --seal-with oldest
    cp -a "$content_dir/." "$FS_MNT/"
    fs_unmount
}

# Set up route packet pointing to remote repo.
# Uses ring0/init identity so the route is sealed by the repo admin key
# (Seal-By: oldest). This matches what get_admin_identity() returns.
setup_route() {
    local group="$1" app="$2"
    HPPR_SIGNER='!ring0/init' $HPPR add "//repo/admin/route/$group/$app" \
        -H "Seal-By: oldest" \
        -H "Upstream: tcp+127.0.0.1:$REMOTE_PORT" \
        -H "Upstream-Verification-Key: $REMOTE_SIGNING_KEY" <<< ""
}

# Set up site-trust packet with remote signing key as member.
# Uses ring0/init identity so site-trust is sealed by the repo admin key
# (Seal-By: oldest). This matches what get_site_trust_keys() expects.
setup_trust() {
    local group="$1" app="$2"
    HPPR_SIGNER='!ring0/init' $HPPR add "//$group/$app/site-trust" \
        -H "Seal-By: oldest" \
        -H "Member: $REMOTE_SIGNING_KEY" <<< ""
}

# Set up site-trust on remote repo.
# Must be sealed by remote repo-vkey (ring0) so hppr-setup can read it at
# //<group>/<app>/site-trust/|/seal/<remote-repo-vkey>.
setup_remote_trust() {
    local group="$1" app="$2"
    HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='!ring0/init' \
        $HPPR add "//$group/$app/site-trust" \
        -H "Seal-By: oldest" \
        -H "Member: $REMOTE_SIGNING_KEY" <<< ""
}

# Set up ring2 on remote repo and pre-create site ring1 account locally.
#
# Creates a site keypair and a route keypair, configures ring2 on the remote
# with both keys as members, and creates the matching ring1 account and
# route-keys packet on the home repo so HAVI finds them at page load.
setup_remote_ring2() {
    local group="$1" app="$2"
    local ring1_name="site:${group}#${app}"

    # Generate site keypair (for window.home ring1 identity)
    local keyname="site-$$"
    $HPPR key generate "$keyname" >/dev/null
    local site_sk site_vk
    site_sk=$($HPPR key show "$keyname")
    site_vk=$($HPPR key pubkey "$keyname")

    # Generate route keypair (for window.route ring2 identity)
    local route_keyname="route-$$"
    $HPPR key generate "$route_keyname" >/dev/null
    local route_sk route_vk
    route_sk=$($HPPR key show "$route_keyname")
    route_vk=$($HPPR key pubkey "$route_keyname")

    # Ring2 setup on remote
    HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='!ring0/init' \
        $HPPR ring2 setup "//$group" --init
    HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='!ring0/init' \
        $HPPR ring2 setup "//$group" acl add r.l "//$group/$app/"
    # Register both site key and route key as ring2 members
    HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='!ring0/init' \
        $HPPR ring2 members "//$group" add "$site_vk"
    HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='!ring0/init' \
        $HPPR ring2 members "//$group" add "$route_vk"

    # Create site ring1 account on home repo with that key.
    # Setup must be sealed by ring0's oldest key (Seal-By: oldest).
    # Keys packet is self-signed by the site key.
    HPPR_SIGNER='!ring0/init' $HPPR add \
        "//repo/admin/ring1/${ring1_name}/setup" \
        -H "Seal-By: oldest" \
        -H "Member: $site_vk" \
        -H "Ring1-Name: $ring1_name" \
        -H "ACL-Rule: rdl //$group/$app/" \
        -H "ACL-Rule: rwl //$group/$app/user/" \
        -H "ACL-Rule: rwl //repo/admin/ring1/${ring1_name}/" \
        -H "ACL-Rule: r.. //repo/admin/route-keys/" <<< ""
    HPPR_SIGNER='!ring0/init' $HPPR add -k "$site_sk" \
        "//repo/admin/ring1/${ring1_name}/keys" \
        -H "Secret-Key: $site_sk" <<< ""

    # Store route key on home repo so ensure_route_key finds it.
    HPPR_SIGNER='!ring0/init' $HPPR add \
        "//repo/admin/route-keys/$group" \
        -H "Seal-By: oldest" \
        -H "Secret-Key: $route_sk" \
        -H "Verification-Key: $route_vk" <<< ""
}

# ============================================================================
# Servo
# ============================================================================

start_servo() {
    local page="$1"
    DEVTOOLS_PORT=$(get_port)
    log "Starting Servo (devtools: $DEVTOOLS_PORT)..."

    local havi_bin="$HAVI_ROOT/target/debug/havi"
    if [[ ! -x "$havi_bin" ]]; then
        log "Building havi..."
        cargo build -q --manifest-path "$HAVI_ROOT/ports/havishell/Cargo.toml"
    fi

    cd "$HAVI_ROOT"

    setsid env \
        HAVI_HOME="tcp+127.0.0.1:$HPPR_PORT" \
        HAVI_DEVTOOLS="127.0.0.1:$DEVTOOLS_PORT" \
        HAVI_URL="$page" \
        "$havi_bin" &
    SERVO_PID=$!

    export HAVI_DEVTOOLS="127.0.0.1:$DEVTOOLS_PORT"

    # Wait for devtools to respond
    log "Waiting for Servo devtools..."
    for _ in {1..80}; do
        if "$HAVI_ROOT/havi-devtools-cli" --timeout 2 eval "true" 2>/dev/null | grep -q '"ok"'; then
            log "Servo ready"
            return 0
        fi
        sleep 0.1
    done
    fail "Servo devtools not ready"
}

# ============================================================================
# Test Runner (THE ONLY TEST LOGIC IN BASH)
# ============================================================================

run_js_tests() {
    local timeout="${1:-15}"
    local debugtool="$HAVI_ROOT/havi-devtools-cli"

    log "Running JS tests (timeout: ${timeout}s)..."

    for _ in $(seq 1 $((timeout * 10))); do
        local result
        result=$("$debugtool" --timeout 3 eval "JSON.stringify(window.testResults)" 2>/dev/null | \
            jq -r 'select(.ok == true) | .value' | tail -1) || true

        if [[ -n "$result" && "$result" != "null" && "$result" != "undefined" ]]; then
            local failed passed
            failed=$(echo "$result" | jq -r '.failed')
            passed=$(echo "$result" | jq -r '.passed')

            # Print individual results
            echo "$result" | jq -r '.results[]' >&2

            log "Passed: $passed, Failed: $failed"

            if [[ "$failed" == "0" ]]; then
                log "PASS"
                return 0
            else
                fail "$failed test(s) failed"
            fi
        fi
        sleep 0.1
    done
    fail "Tests did not complete within ${timeout}s"
}

# ============================================================================
# JS Injection for Generated Pages
# ============================================================================

# Execute JS and return the result value
run_js() {
    local js="$1"
    local debugtool="$HAVI_ROOT/havi-devtools-cli"
    "$debugtool" --timeout 3 eval "$js" 2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1
}

# Inject test suite into hppr-setup:// page (generated by protocol handler)
# Waits for page init(), then injects test-utils and test suite, sets window.testResults
inject_hppr_setup_tests() {
    local debugtool="$HAVI_ROOT/havi-devtools-cli"

    log "Waiting for hppr-setup page to initialize..."

    # Wait for page content to load (init() completes) or error
    for _ in {1..100}; do
        local content_visible error_visible
        content_visible=$(run_js "document.getElementById('content')?.style.display === 'block'")
        error_visible=$(run_js "document.getElementById('error')?.style.display === 'block'")

        [[ "$content_visible" == "true" ]] && break
        [[ "$error_visible" == "true" ]] && break
        sleep 0.1
    done

    log "Injecting test suite..."

    # Inject test-utils functions and test suite
    # Uses heredoc for readability - the JS runs assertions and sets window.testResults
    "$debugtool" --timeout 15 eval "$(cat <<'TESTJS'
(function() {
    // test-utils.js inline
    const results = [];
    function log(msg) { results.push(msg); console.log('[test] ' + msg); }
    function assert(condition, name) {
        if (condition) { log('PASS: ' + name); return true; }
        else { log('FAIL: ' + name); return false; }
    }
    function assertEqual(actual, expected, name) {
        if (actual === expected) { log('PASS: ' + name); return true; }
        else { log('FAIL: ' + name + ' (expected: ' + expected + ', got: ' + actual + ')'); return false; }
    }
    function assertContains(str, substr, name) {
        if (str && str.includes(substr)) { log('PASS: ' + name); return true; }
        else { log('FAIL: ' + name + ' (expected to contain: ' + substr + ', got: ' + str + ')'); return false; }
    }
    function summarize() {
        let passed = 0, failed = 0;
        for (const r of results) {
            if (r.startsWith('PASS:')) passed++;
            if (r.startsWith('FAIL:')) failed++;
        }
        return { passed, failed, results };
    }

    // Test suite for hppr-setup page
    log('--- hppr-setup page tests ---');

    // Check for error first
    const errorEl = document.getElementById('error');
    const errorVisible = errorEl?.style.display === 'block';
    if (errorVisible) {
        log('FAIL: Page shows error: ' + (errorEl?.textContent || 'unknown'));
        window.testResults = summarize();
        return;
    }

    // Content should be visible
    const contentEl = document.getElementById('content');
    assert(contentEl?.style.display === 'block', 'Content is visible');

    // Repo info should be displayed
    const serverKey = document.getElementById('repo-key')?.textContent;
    assert(serverKey && serverKey.length > 10, 'Repo key displayed');

    const serverId = document.getElementById('repo-id')?.textContent;
    assert(serverId && serverId.length > 0, 'Repo ID displayed');

    // Greeting should be parsed (global var from page)
    assert(typeof greeting !== 'undefined' && greeting !== null, 'greeting object exists');
    assert(greeting?.verifyingKey !== null, 'greeting.verifyingKey parsed');

    // Preview xframe should be configured
    const previewFrame = document.getElementById('preview-frame');
    const previewSrc = previewFrame?.src || '';
    assert(previewSrc.includes('hppr-sandbox:'), 'Preview xframe uses hppr-sandbox');

    // Accept button should be ready
    const acceptBtn = document.getElementById('accept-btn');
    assert(acceptBtn && !acceptBtn.disabled, 'Accept button enabled');

    // Checkboxes should be checked by default
    assert(document.getElementById('adopt-admin-key')?.checked, 'Adopt trust checkbox checked');
    assert(document.getElementById('set-endpoint')?.checked, 'Set endpoint checkbox checked');

    // window.ring0 should be available (pre-fetched admin credentials)
    assert(window.ring0 !== null && window.ring0 !== undefined, 'window.ring0 available');

    window.testResults = summarize();
})();
TESTJS
)" >/dev/null 2>&1

    log "Test suite injected"
}
