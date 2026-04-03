#!/usr/bin/env bash
# helper-address-test.sh - helper scheme native address/document surfaces
# shellcheck disable=SC1091

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="helper-address"

start_server
start_servo "havi:///overview"

debugtool="$HAVI_ROOT/havi-devtools-cli"

addr_exists=$($debugtool --text eval 'window.address !== null' 2>/dev/null || true)
[[ "${addr_exists,,}" == "true" ]] || fail "window.address should exist on helper pages"

addr_href=$($debugtool --text eval 'window.address.href' 2>/dev/null || true)
[[ "$addr_href" == "havi:///overview" ]] || fail "unexpected helper address href: $addr_href"

addr_scheme=$($debugtool --text eval 'window.address.scheme' 2>/dev/null || true)
[[ "$addr_scheme" == "havi" ]] || fail "unexpected helper address scheme: $addr_scheme"

addr_qa=$($debugtool --text eval 'window.address.qa === null' 2>/dev/null || true)
[[ "${addr_qa,,}" == "true" ]] || fail "helper address qa should be null"

addr_listing=$($debugtool --text eval 'window.address.isListing' 2>/dev/null || true)
[[ "${addr_listing,,}" == "false" ]] || fail "overview helper page should not be a listing"

doc_url=$($debugtool --text eval 'document.URL' 2>/dev/null || true)
[[ "$doc_url" == "havi:///overview" ]] || fail "unexpected helper document.URL: $doc_url"

doc_uri=$($debugtool --text eval 'document.documentURI' 2>/dev/null || true)
[[ "$doc_uri" == "$doc_url" ]] || fail "document.documentURI should mirror document.URL"

doc_urc_null=$($debugtool --text eval 'document.URC === null' 2>/dev/null || true)
[[ "${doc_urc_null,,}" == "true" ]] || fail "helper document.URC should be null"

doc_packet_null=$($debugtool --text eval 'document.packet === null' 2>/dev/null || true)
[[ "${doc_packet_null,,}" == "true" ]] || fail "helper document.packet should be null"

$debugtool eval 'window.address.href = "havi:///diagnostics"' >/dev/null
$debugtool --text wait-for 'window.address && window.address.href === "havi:///diagnostics"' >/dev/null

after_href=$($debugtool --text eval 'window.address.href' 2>/dev/null || true)
[[ "$after_href" == "havi:///diagnostics" ]] || fail "helper address navigation failed: $after_href"

after_url=$($debugtool --text eval 'document.URL' 2>/dev/null || true)
[[ "$after_url" == "havi:///diagnostics" ]] || fail "helper document.URL navigation failed: $after_url"

log "PASS"
