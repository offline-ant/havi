#!/usr/bin/env bash
set -euo pipefail
# shellcheck disable=SC1091,SC2034
source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

start_server() {
    TEMP_REPO=$(mktemp -d)
    local stdout_file="$TEMP_REPO/hpprd.stdout"
    log "Starting hpprd(debug)..."

    RUST_LOG=debug hpprd --path "$TEMP_REPO" --bind "127.0.0.1:0" --phc "\$argon2id\$v=19\$m=8,t=1,p=1\$" > "$stdout_file" 2>&1 &
    HPPRD_PID=$!

    local bind_addr
    bind_addr=$(read_bind_addr "$stdout_file") || fail "hpprd failed to start"

    export HAVI_HOME="tcp+$bind_addr"
    export HPPR_HOME="tcp+$bind_addr"
        HPPR_PORT="${bind_addr##*:}"

    HPPR_CONFIG_HOME_DIR=$(mktemp -d)
    export HPPR_CONFIG_HOME="$HPPR_CONFIG_HOME_DIR"

    log "hpprd ready at $bind_addr (PID: $HPPRD_PID)"
}

TEST_NAME="browse"
TEST_GROUP="browsetest"
TEST_APP="files"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key
import_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"

SERVO_LOG=$(mktemp)
DEVTOOLS_PORT=$(get_port)
log "Starting Servo (devtools: $DEVTOOLS_PORT)..."
cd "$HAVI_ROOT"
HAVI_HOME="tcp+127.0.0.1:$HPPR_PORT" \
  ./mach run -- "hppr://$TEST_GROUP/$TEST_APP/" --devtools "$DEVTOOLS_PORT" \
  >"$SERVO_LOG" 2>&1 &
SERVO_PID=$!
export HAVI_DEVTOOLS="127.0.0.1:$DEVTOOLS_PORT"

log "Waiting for Servo devtools..."
for _ in {1..80}; do
  if "$HAVI_ROOT/havi-devtools-cli" --timeout 2 eval "true" 2>/dev/null | grep -q '"ok"'; then
    log "Servo ready"
    break
  fi
  sleep 0.1
done

DEBUGTOOL="$HAVI_ROOT/havi-devtools-cli"

echo "TITLE=$($DEBUGTOOL --text eval "document.title")"
echo "BODY=$($DEBUGTOOL --text eval "document.body?.innerText?.slice(0,300)")"

echo "--- hpprd debug tail ---"
tail -n 300 "$TEMP_REPO/hpprd.stdout"

echo "--- servo log tail ---"
tail -n 120 "$SERVO_LOG"
