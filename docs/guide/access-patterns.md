# Access Patterns Guide

This guide covers practical read/write patterns for HAVI apps using the current
committed-source model.

## Mental model

- `window.source` is the one ordinary-page ambient repo story
- `window.source.client` is the client for the committed document source
- `window.source.kind` is `"repo"` or `"remote"`
- explicit extra repo power comes from `HpprClient.named(name)`, not from a
  second ambient client

Use `window.source.client` for the source capability the page actually loaded
with.
Use named clients only when the app truly needs extra repo power beyond that
ambient source.

## Pattern 1: Read from the committed source

```javascript
async function loadNote(path) {
  if (!window.source) throw new Error('No repo-backed source for this page');
  const urc = `//u/notes/${path}`;
  return await (await window.source.client.get(urc)).text();
}
```

## Pattern 2: Gate behavior on source kind

```javascript
async function loadTimeline(urc) {
  if (!window.source) throw new Error('No repo-backed source');

  try {
    return await (await window.source.client.get(urc)).json();
  } catch (e) {
    if (window.source.kind === 'remote') {
      // remote source may fail when upstream is offline
    }
    throw e;
  }
}
```

## Pattern 3: Use named clients for explicit extra power

```javascript
async function loadWithWriterProfile(urc) {
  const client = await HpprClient.named('writer');
  return await (await client.get(urc)).text();
}
```

This is the clean expansion path beyond `window.source`.
It is browser-mediated and grant-scoped.
Do not rebuild old `window.home ?? window.route` fallbacks around it.

## Connectivity checks

Two practical checks:

- `window.source === null`: no ordinary repo-backed source exists for the page
- repo/client failure with `fatal === true`: backend/session failed and the app
  must treat that as an operational failure for the current client

Treat remote-source failure as normal state, not as exceptional app crash state.

## Watch-driven updates

Only use watch/stream primitives on clients that actually support them.
Remote transport-backed clients do.
Browser-mediated local backends that have not grown watch/stream support yet
fail explicitly instead of pretending to be transport-backed.

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

Route packet structure, local route auth storage, and identity text remain part
of the general HPPR route scheme.
HAVI uses those route answers to decide the committed source before ordinary page
JS runs. Page code does not switch between an ambient home client and ambient
route client anymore.

## Write path recommendation

- write ordinary page state through `window.source.client` when that matches the
  actual committed source capability
- use named clients only for explicit extra power
- do not block critical UI on unavailable remote upstreams unless freshness is a
  hard requirement

## Next

- Publishing workflows: [Publishing](publishing.md)
- API details: `../../spec/060-JS-API.md`
- Runtime watch details: `../../spec/050-RUNTIME.md`
