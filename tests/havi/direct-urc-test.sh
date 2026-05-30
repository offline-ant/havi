#!/usr/bin/env bash
# direct-urc-test.sh - direct-hash document.URL/document.URC/window.address behavior
# shellcheck disable=SC1091

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="direct-urc"
TEST_GROUP="~directtest"
TEST_APP="testapp"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key
import_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"

seal_tip=$($HPPR tips "//$TEST_GROUP/$TEST_APP//direct.html" | head -n1)
[[ -n "$seal_tip" ]] || fail "No tip for //$TEST_GROUP/$TEST_APP//direct.html"
seal_hash=${seal_tip##*/}
[[ "$seal_hash" == S.*.H3 ]] || fail "Unexpected direct seal hash: $seal_hash"

start_servo "hppr:////$seal_hash"

debugtool="$HAVI_ROOT/havi-devtools-cli"
$debugtool --text wait-for 'document.packet !== null' >/dev/null

addr_href=$($debugtool --text eval 'window.address.href' 2>/dev/null || true)
[[ "$addr_href" == *"////$seal_hash" ]] || fail "unexpected direct address href: $addr_href"

urc_method=$($debugtool --text eval 'window.address.urc.method' 2>/dev/null || true)
[[ "$urc_method" == "hash" ]] || fail "expected hash urc method, got: $urc_method"

group_is_null=$($debugtool --text eval 'window.address.group === null' 2>/dev/null || true)
[[ "${group_is_null,,}" == "true" ]] || fail "expected null direct group"

app_is_null=$($debugtool --text eval 'window.address.app === null' 2>/dev/null || true)
[[ "${app_is_null,,}" == "true" ]] || fail "expected null direct app"

doc_url=$($debugtool --text eval 'document.URL' 2>/dev/null || true)
[[ "$doc_url" == "hppr://$TEST_GROUP/$TEST_APP//direct.html" ]] || fail "unexpected projected document.URL: $doc_url"

doc_uri=$($debugtool --text eval 'document.documentURI' 2>/dev/null || true)
[[ "$doc_uri" == "$doc_url" ]] || fail "document.documentURI should mirror document.URL"

doc_urc=$($debugtool --text eval 'document.URC' 2>/dev/null || true)
packet_urc=$($debugtool --text eval '"//'"$TEST_GROUP"'/'"$TEST_APP"'/direct.html/|/seal/" + document.packet.sealBy + "/" + document.packet.tai + "/" + document.packet.hash' 2>/dev/null || true)
[[ "$doc_urc" == "$packet_urc" ]] || fail "document.URC mismatch: $doc_urc != $packet_urc"

same_object=$($debugtool --text eval 'window.address.urc === window.address.urc' 2>/dev/null || true)
[[ "${same_object,,}" == "true" ]] || fail "window.address.urc should be SameObject"

log "PASS"
