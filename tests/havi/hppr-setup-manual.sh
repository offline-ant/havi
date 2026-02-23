#!/usr/bin/env bash
# shellcheck disable=SC1091,SC2034
set -euo pipefail

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="hppr-setup-manual"
TEST_GROUP="trusttest"
TEST_APP="testapp"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key

start_remote_server
setup_remote_acl "$TEST_GROUP" "$TEST_APP"
create_remote_key
import_remote_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"
setup_remote_trust "$TEST_GROUP" "$TEST_APP"

start_servo "hppr-setup://$TEST_GROUP/$TEST_APP/{via:127.0.0.1:$REMOTE_PORT}"

echo "MANUAL_READY HAVI_DEVTOOLS=$HAVI_DEVTOOLS LOCAL_PORT=$HPPR_PORT REMOTE_PORT=$REMOTE_PORT"

# keep process alive for interactive debugger calls
# (cleanup trap runs when this script exits)
tail -f /dev/null
