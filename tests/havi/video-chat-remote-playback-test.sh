#!/usr/bin/env bash
# video-chat-remote-playback-test.sh - Validate remote playback receive pipeline from StreamSub bytes
# shellcheck disable=SC1091,SC2034

source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"

TEST_NAME="video-chat-remote-playback"
TEST_GROUP="~videochatremote"
TEST_APP="testapp"

start_server
start_remote_server
setup_remote_acl "$TEST_GROUP" "$TEST_APP" "rwl"
create_remote_key

HPPR_HOME="tcp+127.0.0.1:$REMOTE_PORT" HPPR_SIGNER='ring1:ring0|init' \
  $HPPR add -k "$REMOTE_SECRET_KEY" "//$TEST_GROUP/$TEST_APP/video-chat.html" < "$FORGE_ROOT/video-chat.html"

setup_remote_deploy "$TEST_GROUP" "$TEST_APP"
setup_route "$TEST_GROUP" "$TEST_APP"

start_servo "hppr://$TEST_GROUP/$TEST_APP/video-chat.html"

"$HAVI_ROOT/havi-devtools-cli" --timeout 25 eval "$(cat <<'JS'
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

  const frameChunk = (payload) => {
    const out = new Uint8Array(4 + payload.length);
    const dv = new DataView(out.buffer);
    dv.setUint32(0, payload.length, true);
    out.set(payload, 4);
    return out;
  };

  const waitOpen = (socket, name) => new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error(name + ' onopen timeout')), 10000);
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
      assert(window.__videoChatReady === true, '__videoChatReady true');
      assert(window.__videoChat && typeof window.__videoChat.startReceiverOnly === 'function', '__videoChat.startReceiverOnly exists');

      const group = document.packet.group;
      const app = document.packet.app;
      const prefix = `//${group}/${app}/chat-remote-${Date.now()}`;
      window.__videoChat.setPrefixes(prefix, prefix);

      await window.__videoChat.startReceiverOnly();
      await waitFor(() => {
        const mode = window.__videoChat.getState().mode;
        return mode === 'receiving' || mode === 'receiving-playback-nyi';
      }, 15000, 'receiver start mode transition');

      const startState = window.__videoChat.getState();
      assert(
        startState.receiverMode === 'blob' ||
        startState.receiverMode === 'mse' ||
        startState.receiverMode === 'playback-nyi',
        'receiver mode is valid'
      );

      const streamPub = window.source.client.streamPub(prefix);
      await waitOpen(streamPub, 'streamPub');
      assert(streamPub.readyState === 1, 'streamPub opened for receiver feed');

      const fakeChunk = new TextEncoder().encode('fake-mp4-chunk-data-for-receiver-path');
      await streamPub.write(frameChunk(fakeChunk));
      streamPub.close();

      await waitFor(() => window.__videoChat.getState().remoteChunks >= 1, 10000, 'receiver got framed chunk');
      const afterChunk = window.__videoChat.getState();
      assert(afterChunk.remoteChunks >= 1, 'receiver parsed at least one framed chunk');
      assert(afterChunk.remoteBytes >= fakeChunk.byteLength, 'receiver byte counter advanced');

      if (afterChunk.receiverMode === 'blob') {
        await waitFor(() => window.__videoChat.getState().remotePlaybackAttempts >= 1, 10000, 'blob playback attempted');
        const blobState = window.__videoChat.getState();
        assert(blobState.remotePlaybackQueued >= 1, 'blob playback queued chunk');
        assert(blobState.remotePlaybackAttempts >= 1, 'blob playback attempted remote render');
      } else if (afterChunk.receiverMode === 'mse') {
        await waitFor(() => window.__videoChat.getState().remotePlaybackQueued >= 1, 10000, 'mse append queued');
        const mseState = window.__videoChat.getState();
        assert(mseState.remotePlaybackQueued >= 1, 'mse append queue received chunk');
      } else {
        assertContains(afterChunk.status || '', 'NotYetImplemented', 'playback NYI status surfaced');
      }

      await window.__videoChat.stop();
      await waitFor(() => window.__videoChat.getState().mode === 'stopped', 15000, 'receiver stopped mode transition');
      const stopped = window.__videoChat.getState();
      assert(stopped.mode === 'stopped', 'stop sets stopped mode');
      assertContains(stopped.status || '', 'Stopped', 'stop status text updated');
    } catch (e) {
      fail('video-chat-remote-playback threw: ' + (e && e.message ? e.message : e));
    }

    window.testResults = summarize();
  })();
})();
JS
)" >/dev/null

run_js_tests 45
