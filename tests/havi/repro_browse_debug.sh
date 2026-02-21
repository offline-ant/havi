#!/usr/bin/env bash
set -euo pipefail
# shellcheck disable=SC1091,SC2034
source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

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
HAVI_REPO="tcp+127.0.0.1:$HPPR_PORT" \
  ./mach run -- "hppr://$TEST_GROUP/$TEST_APP/" --devtools "$DEVTOOLS_PORT" \
  >"$SERVO_LOG" 2>&1 &
SERVO_PID=$!
export SERVO_DEBUG_PORT="$DEVTOOLS_PORT"

log "Waiting for Servo devtools..."
for _ in {1..80}; do
  if "$HAVI_ROOT/havi-webview-remote-cli" --timeout 2 eval "true" 2>/dev/null | grep -q '"ok"'; then
    log "Servo ready"
    break
  fi
  sleep 0.1
done

DEBUGTOOL="$HAVI_ROOT/havi-webview-remote-cli"
echo "TITLE=$($DEBUGTOOL --text eval "document.title")"
echo "URL=$($DEBUGTOOL --text eval "window.location.href")"
echo "H1=$($DEBUGTOOL --text eval "document.querySelector('h1')?.textContent")"
echo "BODY=$($DEBUGTOOL --text eval "document.body?.innerText?.slice(0,500)")"

echo "--- hpprd stdout ---"
cat "$TEMP_REPO/hpprd.stdout"

echo "--- servo log tail ---"
tail -n 250 "$SERVO_LOG"
