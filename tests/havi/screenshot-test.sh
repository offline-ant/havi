#!/usr/bin/env bash
# screenshot-test.sh - Test havi-devtools-cli screenshot command
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="screenshot"

start_server
setup_acl u test

# Store a simple HTML page
HPPR_SIGNER='!ring0/init' $HPPR add //u/test/screenshot.html \
    -H "Seal-By: oldest" -H "Content-Type: text/html" \
    <<< '<html><head><title>Screenshot Test</title></head><body style="background:#3366cc;color:white"><h1>Screenshot Test</h1></body></html>'

start_servo "hppr://u/test/screenshot.html"

# Wait for page to load
for _ in {1..30}; do
    title=$(run_js "document.title")
    [[ "$title" == "Screenshot Test" ]] && break
    sleep 0.3
done
[[ "$title" == "Screenshot Test" ]] || fail "Page did not load, title: $title"
log "Page loaded"

# Take screenshot
SCREENSHOT="/tmp/screenshot-test-$$.png"
debugtool="$HAVI_ROOT/havi-devtools-cli"
"$debugtool" screenshot "$SCREENSHOT"

# Verify file exists and is a valid PNG
[[ -f "$SCREENSHOT" ]] || fail "Screenshot file not created"
size=$(wc -c < "$SCREENSHOT")
[[ "$size" -gt 1000 ]] || fail "Screenshot too small: $size bytes"

file_type=$(file -b "$SCREENSHOT")
[[ "$file_type" == *PNG* ]] || fail "Not a PNG: $file_type"
log "Screenshot OK: $size bytes, $file_type"

rm -f "$SCREENSHOT"
log "PASS"
