#!/usr/bin/env bash
# test-prelude.bash - Minimal test infrastructure
#
# Provides: repo daemon lifecycle, port allocation, screenshot + JS result test helpers.
#
# shellcheck disable=SC2034

set -euo pipefail

# ============================================================================
# Path Resolution
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[1]}")" && pwd)"
source "$SCRIPT_DIR/havi-build.bash"
if [[ -n "${FORGE_ROOT:-}" ]]; then
    HPPR_ROOT="$FORGE_ROOT/hppr"
    HAVI_ROOT="$FORGE_ROOT/havi"
else
    HPPR_ROOT="$(cd "$SCRIPT_DIR/../../../hppr" && pwd)"
    HAVI_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
    FORGE_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
fi

# Build hppr and hpprd (quiet, only rebuilds if needed)
cargo build -q --manifest-path "$HPPR_ROOT/rust/tools/cli/Cargo.toml" --bin hppr
cargo build -q --manifest-path "$HPPR_ROOT/rust/services/hpprd/Cargo.toml" --bin hpprd

HOST_OS="$(uname -s)"
if [[ "$HOST_OS" == "Linux" ]]; then
    cargo build -q --manifest-path "$HPPR_ROOT/rust/services/fuse/Cargo.toml" --bin hppr-fuse
else
    cargo build -q --manifest-path "$HPPR_ROOT/rust/services/nfs/Cargo.toml" --bin hppr-nfs
fi

# hppr binaries from cargo build, shell tools from hppr/bin/
export PATH="$HPPR_ROOT/target/debug:$HPPR_ROOT/bin:$PATH"
HPPR="$HPPR_ROOT/target/debug/hppr"
HPPR_FUSE="$HPPR_ROOT/target/debug/hppr-fuse"
HPPR_NFS="$HPPR_ROOT/target/debug/hppr-nfs"

ensure_havi_built

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
HAVI_DEVTOOLS_ENABLED="1"

REMOTE_REPO=""
REMOTE_PORT=""
REMOTE_HPPRD_PID=""
REMOTE_SECRET_KEY=""
REMOTE_SIGNING_KEY=""

FS_MNT=""
FS_PID=""
FS_PORT=""
FS_BACKEND=""

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
    if [[ -n "${FS_PID:-}" || -n "${FS_MNT:-}" ]]; then
        fs_unmount || true
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

    # HAVI_HOME selects the remote hpprd endpoint.
    # HAVI_CONFIG isolates browser-local state per test.
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
    HPPR_SIGNER='ring1:ring0|init' $HPPR ring1 acl anyone add "$perms" "//$group/$app/"
}

# Set up ACL on remote repo
setup_remote_acl() {
    local group="$1" app="$2" perms="${3:-r.l}"
    HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='ring1:ring0|init' \
        $HPPR ring1 acl anyone add "$perms" "//$group/$app/"
}

# Start filesystem mount and export FS_MNT / FS_PID / FS_PORT / FS_BACKEND.
# Linux uses hppr-fuse directly. macOS/Windows use hppr-nfs.
# Call fs_unmount to clean up.
# Usage: fs_mount <home> <signer> <root> [--seal-with <key>]
fs_mount() {
    local home="$1" signer="$2" root="$3"
    shift 3

    FS_MNT=$(mktemp -d)

    if [[ "$HOST_OS" == "Linux" ]]; then
        FS_BACKEND="fuse"

        local -a fuse_args=(
            --home "$home" --signer "$signer"
            --root "$root" --mount "$FS_MNT"
        )
        if [[ $# -gt 0 && "$1" == "--seal-with" ]]; then
            fuse_args+=(--rw --seal-with "$2")
        fi

        "$HPPR_FUSE" "${fuse_args[@]}" &
        FS_PID=$!

        local i=0
        while ! mountpoint -q "$FS_MNT" 2>/dev/null; do
            sleep 0.1
            ((i += 1))
            if ((i > 50)); then
                kill "$FS_PID" 2>/dev/null || true
                rm -rf "$FS_MNT"
                echo "ERROR: hppr-fuse did not mount $FS_MNT" >&2
                return 1
            fi
        done
        return 0
    fi

    FS_BACKEND="nfs"
    FS_PORT=$(_pick_port)

    local -a nfs_args=(
        --home "$home" --signer "$signer"
        --root "$root" --bind "127.0.0.1:$FS_PORT"
    )
    if [[ $# -gt 0 && "$1" == "--seal-with" ]]; then
        nfs_args+=(--rw --seal-with "$2")
    fi

    "$HPPR_NFS" "${nfs_args[@]}" &
    FS_PID=$!

    local i=0
    while ! nc -z 127.0.0.1 "$FS_PORT" 2>/dev/null; do
        sleep 0.1
        ((i += 1))
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

# Unmount and stop filesystem service started by fs_mount.
fs_unmount() {
    [[ -z "${FS_MNT:-}" ]] && return 0

    log "Unmounting $FS_BACKEND mount at $FS_MNT"
    if [[ "${FS_BACKEND:-}" == "fuse" ]]; then
        fusermount3 -u "$FS_MNT" 2>/dev/null || umount "$FS_MNT" 2>/dev/null || true
    else
        sudo umount "$FS_MNT" 2>/dev/null || true
    fi

    kill "${FS_PID:-}" 2>/dev/null || true
    wait "${FS_PID:-}" 2>/dev/null || true
    rm -rf "$FS_MNT"
    FS_MNT=""
    FS_PID=""
    FS_PORT=""
    FS_BACKEND=""
}

# Pick an unused TCP port.
_pick_port() {
    python3 -c 'import socket; s=socket.socket(); s.bind(("",0)); print(s.getsockname()[1]); s.close()'
}

copy_into_mount() {
    local source_dir="$1"
    shift

    local -a paths=()
    if [[ $# -eq 0 ]]; then
        shopt -s nullglob dotglob
        paths=("$source_dir"/*)
        shopt -u nullglob dotglob
    else
        for name in "$@"; do
            paths+=("$source_dir/$name")
        done
    fi

    log "Copying ${#paths[@]} path(s) into $FS_MNT"
    cp -a "${paths[@]}" "$FS_MNT/"
}

# Import content directory as sealed packets via filesystem mount + cp.
import_content() {
    local content_dir="$1" group="$2" app="$3"
    fs_mount "$HPPR_HOME" "ring1:ring0|init" "//$group/$app" --seal-with ring0
    copy_into_mount "$content_dir"
    fs_unmount
}

# Import selected content paths as sealed packets via filesystem mount + cp.
import_content_paths() {
    local content_dir="$1" group="$2" app="$3"
    shift 3
    fs_mount "$HPPR_HOME" "ring1:ring0|init" "//$group/$app" --seal-with ring0
    copy_into_mount "$content_dir" "$@"
    fs_unmount
}

# Import content to remote repo via filesystem mount + cp.
# Remote deployed content is authored under the explicit remote content key so
# deploy-pointer authority and later remote writes share one content signer.
import_remote_content() {
    local content_dir="$1" group="$2" app="$3"
    [[ -n "${REMOTE_SECRET_KEY:-}" ]] || fail "remote content import requires create_remote_key first"
    fs_mount "tcp+127.0.0.1:$REMOTE_PORT" "ring1:ring0|init" "//$group/$app" --seal-with "$REMOTE_SECRET_KEY"
    copy_into_mount "$content_dir"
    fs_unmount
}

# Set up local exact-app route record pointing to remote repo.
setup_route() {
    local group="$1" app="$2"

    HPPR_SIGNER='ring1:ring0|init' $HPPR route local app set \
        --verify \
        --signer 'ring1:ring0|init' \
        "//$group/$app" \
        "tcp+127.0.0.1:$REMOTE_PORT" >/dev/null
}

# Set up app content pointer on remote repo.
# Resolver reads //<group>/admin/deploy/<app>/|/seal/<remote-repo-vkey>.
# Remote deployed content is authored under REMOTE_SIGNING_KEY, so the content
# authority published here must match that signer.
setup_remote_deploy() {
    local group="$1" app="$2"
    [[ -n "${REMOTE_SIGNING_KEY:-}" ]] || fail "remote deploy setup requires create_remote_key first"

    HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='ring1:ring0|init' \
        $HPPR ring1 acl anyone add r.l "//$group/admin/deploy/" >/dev/null
    HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='ring1:ring0|init' \
        $HPPR add "//$group/admin/deploy/$app" \
        -H "Seal-By: ring0" \
        -H "Content-Root: //$group/$app" \
        -H "Content-Authority: $REMOTE_SIGNING_KEY" <<< ""
}

# Set up remote Ring2 and local route auth for routed access.
setup_remote_ring2() {
    local group="$1" app="$2"

    local route_keyname="route-$$"
    $HPPR key generate "$route_keyname" >/dev/null
    local route_sk route_vk
    route_sk=$($HPPR key show "$route_keyname")
    route_vk=$($HPPR key pubkey "$route_keyname")

    HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='ring1:ring0|init' \
        $HPPR ring2 setup "//$group" --init >/dev/null
    HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='ring1:ring0|init' \
        $HPPR ring2 setup "//$group" acl add r.l "//$group/$app/" >/dev/null
    HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='ring1:ring0|init' \
        $HPPR ring2 setup "//$group" acl add r.l "//$group/admin/deploy/" >/dev/null
    HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='ring1:ring0|init' \
        $HPPR ring2 members "//$group" add "$route_vk" >/dev/null

    HPPR_SIGNER='ring1:ring0|init' $HPPR add \
        "//repo/route/auth/$group" \
        -H "Seal-By: ring0" \
        -H "Auth: ring2:$group|$route_sk" <<< ""
}

# ============================================================================
# Servo
# ============================================================================

wait_for_devtools() {
    [[ "${HAVI_DEVTOOLS_ENABLED:-1}" == "1" ]] || return 0
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

start_servo() {
    local page="$1"
    local havi_bin="$HAVI_ROOT/target/debug/havi"
    ensure_havi_built

    cd "$HAVI_ROOT"

    local -a env_args=(
        HAVI_HOME="tcp+127.0.0.1:$HPPR_PORT"
        HAVI_URL="$page"
    )
    if [[ "${HAVI_DEVTOOLS_ENABLED:-1}" == "1" ]]; then
        DEVTOOLS_PORT=$(get_port)
        env_args+=(HAVI_DEVTOOLS="127.0.0.1:$DEVTOOLS_PORT")
        export HAVI_DEVTOOLS="127.0.0.1:$DEVTOOLS_PORT"
        log "Starting Servo (devtools: $DEVTOOLS_PORT)..."
    else
        DEVTOOLS_PORT=""
        unset HAVI_DEVTOOLS || true
        log "Starting Servo..."
    fi

    setsid env "${env_args[@]}" "$havi_bin" &
    SERVO_PID=$!

    wait_for_devtools
}

# ============================================================================
# Test Runner (THE ONLY TEST LOGIC IN BASH)
# ============================================================================

wait_for_js_results() {
    local timeout="${1:-15}"
    local debugtool="$HAVI_ROOT/havi-devtools-cli"

    for _ in $(seq 1 $((timeout * 10))); do
        local result
        result=$("$debugtool" --timeout 3 eval "JSON.stringify(window.testResults)" 2>/dev/null | \
            jq -r 'select(.ok == true) | .value' | tail -1) || true
        if [[ -n "$result" && "$result" != "null" && "$result" != "undefined" ]]; then
            printf '%s\n' "$result"
            return 0
        fi
        sleep 0.1
    done
    return 1
}

run_js_tests() {
    local timeout="${1:-15}"

    log "Running JS tests (timeout: ${timeout}s)..."
    local result
    result=$(wait_for_js_results "$timeout") || fail "Tests did not complete within ${timeout}s"

    local failed passed
    failed=$(echo "$result" | jq -r '.failed')
    passed=$(echo "$result" | jq -r '.passed')

    echo "$result" | jq -r '.results[]' >&2
    log "Passed: $passed, Failed: $failed"

    if [[ "$failed" == "0" ]]; then
        log "PASS"
        return 0
    fi
    fail "$failed test(s) failed"
}

# ============================================================================
# JS Helpers
# ============================================================================

# Execute JS and return the result value
run_js() {
    local js="$1"
    local debugtool="$HAVI_ROOT/havi-devtools-cli"
    "$debugtool" --timeout 3 eval "$js" 2>/dev/null | jq -r 'select(.ok == true) | .value' | tail -1
}
