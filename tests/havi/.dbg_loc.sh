#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"
TEST_NAME="dbg-loc"
TEST_GROUP="~loccompat"
TEST_APP="testapp"
start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key
import_content_paths "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP" location-compat.html test-utils.js
start_servo "hppr://$TEST_GROUP/$TEST_APP/location-compat.html"
"$HAVI_ROOT/havi-devtools-cli" --timeout 5 eval 'JSON.stringify({nativeDesc: !!window.__haviNativeWindowLocationDesc, nativeLocType: typeof window.__haviNativeLocation, nativeLocHref: window.__haviNativeLocation && window.__haviNativeLocation.href, ready: window.__haviLocationCompatReady, compat: window.__haviCompatLocation, src: typeof window.__haviLocationCompatSource})'
