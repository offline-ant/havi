#!/usr/bin/env bash
# chunk-test.sh - Test chunk manifest transparency (protocol handler + JS API)
#
# Tests:
# 1. JS API chunk get — window.home.get() on chunked coordinate returns reassembled data
# 2. Non-chunked regression — small packet still works
# 3. document.packet is raw manifest — navigated chunked page has Chunk+Link headers
# 4. Content-Type preservation — chunked .html location renders as HTML
#
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="chunk"
TEST_GROUP="chunktest"
TEST_APP="testapp"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key
import_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"

# --- Generate chunked test data ---
CHUNK_DIR=$(mktemp -d)

# Generate 10KB text file with known pattern (100 lines, ~101 chars each)
for i in $(seq 0 99); do
    printf 'LINE_%03d:ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcdefghijklmnopqrstuvwxyz0123456789PADDING_END\n' "$i"
done > "$CHUNK_DIR/data.txt"

log "Test data size: $(wc -c < "$CHUNK_DIR/data.txt") bytes"

# Chunk and store (1024-byte chunks forces multiple chunks for ~10KB file)
HPPR_SIGNER='!ring0/init' $HPPR chunk "$CHUNK_DIR/data.txt" \
    --chunk-size 1024 \
    "//$TEST_GROUP/$TEST_APP/chunked/data.txt"
log "Chunked data stored"

# Store a small non-chunked packet for regression test
printf 'hello non-chunked world' | HPPR_SIGNER='!ring0/init' $HPPR add "//$TEST_GROUP/$TEST_APP/small/test.txt"
log "Small packet stored"

# Generate chunked HTML page that self-tests document.packet
# This page checks its own Chunk+Link headers and reports results
cat > "$CHUNK_DIR/chunk-nav.html" <<'NAVHTML'
<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>Chunk Nav Self-Test</title>
</head>
<body>
  <h1>Chunk Navigation Test</h1>
  <pre id="results"></pre>

  <script>
    // Inline test-utils (this page is served standalone, not from content/)
    const results = [];
    function log(msg) { results.push(msg); console.log('[test] ' + msg); }
    function assert(cond, name) {
        if (cond) { log('PASS: ' + name); return true; }
        else { log('FAIL: ' + name); return false; }
    }
    function assertEqual(a, b, name) {
        if (a === b) { log('PASS: ' + name); return true; }
        else { log('FAIL: ' + name + ' (expected: ' + b + ', got: ' + a + ')'); return false; }
    }
    function summarize() {
        let passed = 0, failed = 0;
        for (const r of results) {
            if (r.startsWith('PASS:')) passed++;
            if (r.startsWith('FAIL:')) failed++;
        }
        return { passed, failed, results };
    }

    (function() {
        log('--- document.packet chunk manifest tests ---');

        // document.packet should be the raw manifest (not synthetic)
        assert(document.packet !== null, 'document.packet exists');
        assertEqual(document.packet.type, 'Plex', 'document.packet is Plex (manifest)');

        // The raw manifest should have Chunk+Link headers
        const chunkLinks = document.packet.getHeaders('Chunk+Link');
        assert(chunkLinks.length > 0, 'document.packet has Chunk+Link headers (count: ' + chunkLinks.length + ')');

        // Each Chunk+Link should have format "start..end hash"
        if (chunkLinks.length > 0) {
            const first = chunkLinks[0];
            assert(first.includes('..'), 'Chunk+Link contains range (..)');
            assert(first.includes('B.'), 'Chunk+Link references blob hash');
        }

        // Content-Total-Length should be present
        const totalLen = document.packet.getHeader('Content-Total-Length');
        assert(totalLen !== null, 'manifest has Content-Total-Length header');
        assert(parseInt(totalLen) > 0, 'Content-Total-Length is positive');

        // Data length should be 0 (manifest has no body data)
        assertEqual(document.packet.dataLength, 0, 'manifest dataLength is 0');

        log('--- Content-Type preservation ---');
        // This page was stored with .html location, so Content-Type should be inferred as text/html
        // If we got here and the page rendered as HTML (not plain text), Content-Type works
        assert(document.querySelector('h1') !== null, 'page rendered as HTML (h1 element exists)');
        assertEqual(document.title, 'Chunk Nav Self-Test', 'page title correct (HTML parsed)');

        window.testResults = summarize();
        document.getElementById('results').textContent = results.join('\n');
    })();
  </script>
  <!--
    PADDING: This HTML page needs to be large enough to produce multiple chunks
    at 1024-byte chunk size. We pad it to ensure it exceeds 1024 bytes.
    ............................................................................
    ............................................................................
    ............................................................................
    ............................................................................
    ............................................................................
    ............................................................................
    ............................................................................
    ............................................................................
    ............................................................................
    ............................................................................
    ............................................................................
    ............................................................................
  -->
</body>
</html>
NAVHTML

log "Chunk-nav HTML size: $(wc -c < "$CHUNK_DIR/chunk-nav.html") bytes"

# Chunk and store the HTML page
HPPR_SIGNER='!ring0/init' $HPPR chunk "$CHUNK_DIR/chunk-nav.html" \
    --chunk-size 1024 \
    "//$TEST_GROUP/$TEST_APP/chunked/nav.html"
log "Chunked HTML page stored"

rm -rf "$CHUNK_DIR"

# === Phase 1: JS API tests (chunk-test.html, served normally) ===
log "=== Phase 1: JS API chunk tests ==="
start_servo "hppr://$TEST_GROUP/$TEST_APP/chunk-test.html"
run_js_tests

# Stop servo for phase 2
stop_pid "$SERVO_PID"
SERVO_PID=""
sleep 0.5

# === Phase 2: document.packet + Content-Type tests (chunked HTML page) ===
log "=== Phase 2: document.packet manifest tests ==="
start_servo "hppr://$TEST_GROUP/$TEST_APP/chunked/nav.html"
run_js_tests
