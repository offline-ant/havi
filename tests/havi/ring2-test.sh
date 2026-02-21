#!/usr/bin/env bash
# ring2-test.sh - Test window.route with two hpprd instances
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="remote"
TEST_GROUP="remotetest"
TEST_APP="testapp"

# Local server (browser storage, routes, trust)
start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key

# Remote server (serves content)
start_remote_server
setup_remote_acl "$TEST_GROUP" "$TEST_APP"
create_remote_key
import_remote_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"
setup_remote_ring2 "$TEST_GROUP" "$TEST_APP"

# Set up trust before route (setup_trust writes to local repo;
# after setup_route, the coordinate resolves to the remote)
setup_trust "$TEST_GROUP" "$TEST_APP"
setup_route "$TEST_GROUP" "$TEST_APP"

start_servo "hppr://$TEST_GROUP/$TEST_APP/ring2.html"
run_js_tests 30
