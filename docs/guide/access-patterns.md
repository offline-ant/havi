# Access Patterns Guide

This guide covers practical read/write patterns for HAVI apps using
`window.home` and `window.route`.

## Mental model

- `window.home`: local home repo. Always available.
- `window.route`: remote routed repo client. May be `null` when no routed
  endpoint exists for the current source.

Use home for durability and offline safety. Use route for freshness.
Route packet structure and local route auth storage are defined by the general
HPPR route scheme, not by HAVI-specific packet rules.

## Pattern 1: Local-first with remote fallback

Use local data immediately. Fall back to remote when local miss occurs.

```javascript
async function loadNote(path) {
  const urc = `//u/notes/${path}`;
  try {
    return await (await window.home.get(urc)).text();
  } catch (_) {
    if (!window.route) throw _;
    return await (await window.route.get(urc)).text();
  }
}
```

When route succeeds, fetched packets are cached in home repo.

## Pattern 2: Remote-first with local fallback

Use this when freshness matters more than latency.

```javascript
async function loadTimeline(urc) {
  if (window.route) {
    try {
      return await (await window.route.get(urc)).json();
    } catch (_) {
      // continue to local fallback
    }
  }
  return await (await window.home.get(urc)).json();
}
```

## Pattern 3: Background refresh with hash compare

Render local snapshot first. Refresh in background and update UI only if
content hash changed.

```javascript
async function refreshIfChanged(urc, apply) {
  const local = await window.home.get(urc);
  apply(local);
  if (!window.route) return;

  try {
    const remote = await window.route.get(urc);
    if (remote.hash !== local.hash) apply(remote);
  } catch (_) {
    // offline or route failure: keep local view
  }
}
```

## Pattern 4: Queue writes locally, sync later

Write immediately to `user/` or app-local coordinates on home repo. Sync to
route when remote is available.

1. Append outgoing changes to a local queue packet/list.
2. Try route write.
3. On success, mark queue item synced.
4. On failure, keep queued and retry with backoff.

This keeps UX responsive during route outages.

## Connectivity checks

Two practical checks:

- `window.route === null`: no local route answer or no usable route endpoint.
- HPPR error with `fatal === true`: route/session failed; reconnect needed.

Treat route failure as normal state, not exceptional app crash state.

## Watch-driven updates

### In page code

Use `watch()` when you want event-driven refresh.

```javascript
const ws = window.home.watch('//u/site/');
ws.onmessage = (e) => {
  // e.data: "+ //..." or "- //..."
};
```

For remote watches, use `window.route.watch(...)` when route exists.

### In browser workflow (DevTools watch modes)

HAVI supports per-tab watch modes through DevTools:

- `off`
- `notify`
- `auto`
- `tree`

```bash
./havi/havi-devtools-cli watch --tab 0 auto
./havi/havi-devtools-cli watch --tab 0   # query current mode
```

Use:

- `notify` for editorial review (see changes, no forced reload)
- `auto` for exact-page live reload
- `tree` for backing-root reload during active development

## Route setup patterns

- First-time route setup: `hppr route join //group/app <address>`
- Reuse configured route: `hppr --via route get //group/app/path`
- Explicit endpoint: `hppr --via tcp+host:port get //group/app/path`

`route join` stores local route app metadata and a group-default local route auth record in the home repo.
Exact-app local route auth can override that default under `//repo/route/auth/<group>/<app>/|`.

## Write path recommendation

- User state: write to home repo first.
- Shared canonical content: write to route repo (or publish pipeline) with
  explicit retry/queue semantics.
- Do not block critical UI on route availability.

## Next

- Publishing workflows: [Publishing](publishing.md)
- API details: `../../spec/060-JS-API.md`
- Runtime watch details: `../../spec/050-RUNTIME.md`
