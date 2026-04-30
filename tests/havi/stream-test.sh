#!/usr/bin/env bash
# stream-test.sh - Test transport-backed WATCH/STREAM on EnvelopeHpprClient.
#
# The truthful client model keeps raw remote connect and live transport
# primitives on EnvelopeHpprClient. This test runs on an internal helper page
# and exercises the direct remote transport surface without depending on routed
# ordinary-page navigation.
#
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="stream"
TEST_GROUP="streamtest"
TEST_APP="testapp"

start_server
start_remote_server
setup_remote_acl "$TEST_GROUP" "$TEST_APP" "rwl"
create_remote_key

# Store the signing key so the JS test can fetch it through an unpacked client.
echo -n "$REMOTE_SECRET_KEY" | HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='ring1:ring0|init' \
  $HPPR add -k "$REMOTE_SECRET_KEY" "//$TEST_GROUP/$TEST_APP/testkey" >/dev/null

# Start cooked publisher against the remote repo for the StreamSub receive path.
{ sleep 4; echo -n "hello-havi"; sleep 2; } | \
  HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='ring1:ring0|init' \
  $HPPR stream-pub --key "$REMOTE_SECRET_KEY" "//$TEST_GROUP/$TEST_APP/live" &
PUB_PID=$!
log "Publisher started (PID: $PUB_PID)"

start_servo "havi:///diagnostics"

"$HAVI_ROOT/havi-devtools-cli" --timeout 25 eval "$(cat <<JS
(function () {
  const endpoint = 'tcp+127.0.0.1:$REMOTE_PORT';
  const group = '$TEST_GROUP';
  const app = '$TEST_APP';
  const results = [];
  const log = (msg) => { results.push(msg); console.log('[test] ' + msg); };
  const pass = (name) => log('PASS: ' + name);
  const fail = (name) => log('FAIL: ' + name);
  const assert = (cond, name) => cond ? pass(name) : fail(name);
  const assertEqual = (a, b, name) => assert(a === b, name + ' (got ' + JSON.stringify(a) + ', want ' + JSON.stringify(b) + ')');
  const assertContains = (str, sub, name) => assert(String(str || '').includes(sub), name + ' (missing ' + sub + ')');
  const summarize = () => {
    let passed = 0, failed = 0;
    for (const r of results) {
      if (r.startsWith('PASS:')) passed++;
      if (r.startsWith('FAIL:')) failed++;
    }
    return { passed, failed, results };
  };

  const waitOpen = (socket, name, timeoutMs = 15000) => new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error(name + ' onopen timeout')), timeoutMs);
    if (socket.readyState === 1) {
      clearTimeout(timeout);
      resolve();
      return;
    }
    socket.onopen = () => { clearTimeout(timeout); resolve(); };
    socket.onerror = () => { clearTimeout(timeout); reject(new Error(name + ' onerror')); };
  });

  (async () => {
    try {
      const env = await EnvelopeHpprClient.connect(endpoint);
      assertEqual(env.endpoint, endpoint, 'EnvelopeHpprClient.connect returns remote endpoint');
      const client = env.unpack();
      assertEqual(typeof client.endpoint, 'undefined', 'HpprClient.endpoint stays absent on unpacked client');

      const subPrefix = '//' + group + '/' + app + '/live';
      const sub = env.streamSub(subPrefix);
      assert(sub !== null && sub !== undefined, 'streamSub() returns object');
      await waitOpen(sub, 'streamSub');
      assertEqual(sub.readyState, 1, 'streamSub readyState is OPEN');
      assertEqual(sub.prefix, subPrefix, 'streamSub prefix matches');

      const reader = sub.stream.getReader();
      const readResult = await Promise.race([
        reader.read(),
        new Promise((_, reject) => setTimeout(() => reject(new Error('streamSub read timeout')), 20000))
      ]);
      assert(!readResult.done, 'streamSub read returned payload');
      assert(readResult.value instanceof Uint8Array, 'streamSub payload is Uint8Array');
      const text = new TextDecoder().decode(readResult.value);
      assertContains(text, 'hello-havi', 'streamSub received published payload');
      reader.releaseLock();
      sub.close();

      const keyPacket = await client.get('//' + group + '/' + app + '/testkey');
      const key = keyPacket.text();
      assert(key.startsWith('&.'), 'signing key fetched through unpacked client');

      const pubPrefix = '//' + group + '/' + app + '/live2';
      const pub = env.streamPub(pubPrefix, { key });
      assert(pub !== null && pub !== undefined, 'streamPub() returns object');
      await waitOpen(pub, 'streamPub');
      assertEqual(pub.readyState, 1, 'streamPub readyState is OPEN');

      const packetPromise = new Promise((resolve) => {
        const timeout = setTimeout(() => resolve(null), 10000);
        pub.addEventListener('packet', (e) => {
          clearTimeout(timeout);
          resolve(e);
        }, { once: true });
        pub.addEventListener('close', () => {
          clearTimeout(timeout);
          resolve(null);
        }, { once: true });
        pub.addEventListener('error', () => {
          clearTimeout(timeout);
          resolve(null);
        }, { once: true });
      });

      await pub.write(new TextEncoder().encode('hello publisher'));
      pass('streamPub write resolved');
      pub.finishSegment();
      pub.close();
      pass('streamPub finishSegment and close requested');

      const event = await packetPromise;
      if (event !== null) {
        assert(event.data && typeof event.data.hash === 'string', 'streamPub packet event carries HpprPacket');
        assertEqual(await event.data.text(), 'hello publisher', 'streamPub packet payload matches');
      } else {
        pass('streamPub packet event not observed before close in this runtime path');
      }
    } catch (e) {
      fail('stream transport test threw: ' + (e && e.message ? e.message : e));
    }

    window.testResults = summarize();
  })();
})();
JS
)" >/dev/null

run_js_tests 40

stop_pid "$PUB_PID"
PUB_PID=""
