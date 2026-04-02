#!/usr/bin/env bash
# video-chat-contract-test.sh - Validate /video-chat.html recorder branch contract
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="video-chat-contract"
TEST_GROUP="videochatcontract"
TEST_APP="testapp"

start_server
setup_acl "$TEST_GROUP" "$TEST_APP"
create_key

HPPR_SIGNER='ring1:ring0|init' $HPPR add "//$TEST_GROUP/$TEST_APP/video-chat.html" < "$FORGE_ROOT/video-chat.html"

start_servo "hppr://$TEST_GROUP/$TEST_APP/video-chat.html"

"$HAVI_ROOT/havi-devtools-cli" --timeout 20 eval "$(cat <<'JS'
(function () {
  const results = [];
  const log = (msg) => { results.push(msg); console.log('[test] ' + msg); };
  const pass = (name) => log('PASS: ' + name);
  const fail = (name) => log('FAIL: ' + name);
  const assert = (cond, name) => cond ? pass(name) : fail(name);
  const assertContains = (str, sub, name) => {
    if (String(str || '').includes(sub)) pass(name);
    else fail(name + ' (expected substring ' + sub + ')');
  };

  const summarize = () => {
    let passed = 0, failed = 0;
    for (const r of results) {
      if (r.startsWith('PASS:')) passed++;
      if (r.startsWith('FAIL:')) failed++;
    }
    return { passed, failed, results };
  };

  const waitFor = (cond, timeoutMs, label) => new Promise((resolve, reject) => {
    const start = Date.now();
    const tick = () => {
      let ok = false;
      try { ok = !!cond(); } catch (_) {}
      if (ok) return resolve();
      if (Date.now() - start > timeoutMs) return reject(new Error(label + ' timeout'));
      setTimeout(tick, 30);
    };
    tick();
  });

  (async () => {
    try {
      assert(window.__videoChatReady === true, '__videoChatReady true');
      assert(window.__videoChat && typeof window.__videoChat.start === 'function', '__videoChat.start exists');

      const group = document.packet.group;
      const app = document.packet.app;
      const prefix = `//${group}/${app}/chat-contract-${Date.now()}`;
      window.__videoChat.setPrefixes(prefix, prefix);

      window.__videoChat.start();
      await waitFor(() => {
        const mode = window.__videoChat.getState().mode;
        return mode === 'sending' || mode === 'receive-only-nyi' || mode === 'error';
      }, 20000, 'chat start mode transition');

      const state = window.__videoChat.getState();
      assert(state.mode === 'sending' || state.mode === 'receive-only-nyi', 'start resolves into expected sender branch');

      const statusText = state.status || '';
      assert(statusText.length > 0, 'status text is visible');
      if (state.mode === 'sending') {
        assertContains(statusText, 'sending', 'status reports sending branch');
      } else {
        assertContains(statusText, 'Receive-only mode', 'status reports NYI receive-only branch');
        assertContains(statusText, 'NotYetImplemented', 'status includes NotYetImplemented marker');
      }

      window.__videoChat.stop();
      await waitFor(() => window.__videoChat.getState().mode === 'stopped', 15000, 'chat stopped mode transition');
      const stopped = window.__videoChat.getState();
      assert(stopped.mode === 'stopped', 'stop sets stopped mode');
      assertContains(stopped.status || '', 'Stopped', 'stop status text updated');
    } catch (e) {
      fail('video-chat-contract threw: ' + (e && e.message ? e.message : e));
    }

    window.testResults = summarize();
  })();
})();
JS
)" >/dev/null

run_js_tests 45
