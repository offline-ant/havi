#!/usr/bin/env bash
# stream-test.sh - Test StreamIn/StreamOut/StreamIO JS API
#
# Tests:
# 1. StreamOut creation — object has readyState, prefix, stream attributes
# 2. StreamOut receives data — reads bytes published by CLI stream-in
# 3. StreamIn creation — object has readyState, prefix, onopen
# 4. StreamIn onopen — readyState transitions to OPEN
# 5. StreamIn write — write resolves for Uint8Array
# 6. StreamIO creation — input/output attributes are StreamIn/StreamOut
#
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="stream"
TEST_GROUP="streamtest"
TEST_APP="testapp"

HPPR_ROOT_ABS="$(cd "$HPPR_ROOT" && pwd)"
PY="$HPPR_ROOT_ABS/py/.venv/bin/python3"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key
import_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"

# --- Generate trailer segment for the publisher ---
STREAM_TEMP=$(mktemp -d)

make_trailer_segment() {
    local coord="$1" data="$2" outfile="$3"
    local group app location
    group=$(echo "$coord" | cut -d/ -f3)
    app=$(echo "$coord" | cut -d/ -f4)
    location=$(echo "$coord" | cut -d/ -f5-)

    $PY -c "
import sys, os, time
sys.path.insert(0, '$HPPR_ROOT_ABS/py')
from hppr.hsb3 import generate_key
from hppr.packets.trailer import create_seal_trailer
sk = generate_key()
tai = str(int(time.time())) + ':000000000'
segment = create_seal_trailer(sk, '$group', '$app', '$location', tai, None, b'$data')
sys.stdout.buffer.write(segment)
" > "$outfile"
}

make_trailer_segment "//$TEST_GROUP/$TEST_APP/live/seg/0001" "hello-havi" "$STREAM_TEMP/seg.bin"
log "Trailer segment size: $(wc -c < "$STREAM_TEMP/seg.bin") bytes"

# Start publisher: delayed data feed keeps stream open until data arrives
{ sleep 4; cat "$STREAM_TEMP/seg.bin"; } | HPPR_SIGNER='!ring0/init' $HPPR stream-in "//$TEST_GROUP/$TEST_APP/live" &
PUB_PID=$!
log "Publisher started (PID: $PUB_PID)"

# Give the publisher time to register the stream
sleep 1

start_servo "hppr://$TEST_GROUP/$TEST_APP/stream-test.html"
run_js_tests 30

# Cleanup publisher
stop_pid "$PUB_PID"
PUB_PID=""
rm -rf "$STREAM_TEMP"
