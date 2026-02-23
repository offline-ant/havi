# HAVI Quickstart

This guide is the conceptual companion to `docs/README.md` in the package.
Use it to understand what HAVI renders, how trust works, and how to use the
browser-side HPPR API.

For protocol depth beyond this page, read `QUICKSTART-HPPR.md` (included in this
package) and the full specs in `docs/spec/hppr/` and `docs/spec/havi/`.

## HPPR in 60 seconds

HPPR stores content as signed packets.

- **Blob (`B.`)**: raw bytes (`Data-Length` + data)
- **Plex (`P.`)**: metadata wrapper around a Blob (`Group`, `App`, `Location`,
  `TAI`)
- **Seal (`S.`)**: signature wrapper around a Plex (`Seal-By`, `Seal-Sig`)

Every packet starts with a markline:

```text
🖧: <T.<hash>.H3>
```

Hash and signature checks are content integrity and authorship proofs:

- hash proves bytes are unchanged
- seal signature proves which key signed the packet

Address forms:

- coordinate (latest by default): `//<group>/<app>/<location>`
- immutable by hash: `////<T.hash.H3>`
- version-pinned coordinate: `.../|/plex/<tai>/<hash>` or `.../|/seal/...`

HAVI is a browser on top of this model.

## HAVI model

HAVI has a **home repo** (local persistent store) and optional **route repos**
(remote endpoints).

- `window.home`: always local, persistent, offline-capable
- `window.route`: remote route client, nullable/unavailable when route/auth is
  missing

Route config and trust config are separate:

- route decides **where** content is fetched
- site-trust decides **which signer keys** may run JavaScript

Origin boundary is HPPR-native: `//<group>/<app>/`.

## URL schemes and handlers

General form:

```text
scheme://group/app/location{via:endpoint}
```

Supported schemes:

- `hppr://` — normal content navigation
- `hppr-setup://` — endpoint trust/route setup flow
- `hppr-sandbox://` — untrusted preview (JS blocked, strict CSP)
- `hppr-browse://` — coordinate tree browser
- `hppr-editor://` — local packet editor (home + ring0 tools)
- `havi://` — internal admin pages (`overview`, routes, ring0 approvals)

Routing behavior:

- `{via:endpoint}` forces a direct upstream endpoint
- `hppr://...` without `via` uses automatic route lookup from the home repo

Trailing slash means LIST view; no trailing slash means GET and render content.

## Trust and execution rules

HAVI enables page JavaScript only when signer trust for `//group/app/` passes.

Trust source is local site-trust data in the home repo. ACL enforcement still
happens server-side in `hpprd` for every command.

Each site gets an isolated Ring1 identity (`HAVI-site:<group>#<app>`), so
cross-site privilege sharing does not happen implicitly.

## Content type resolution

HAVI resolves render type in this order:

1. packet `Content-Type` header (if present)
2. extension mapping from coordinate path
3. fallback `application/octet-stream`

Chunk manifests (`Chunk+Link` + `Data-Length: 0`) are fetched and reassembled
transparently by `get()` and page rendering.

## Launch and first content

From the project/package root:

```bash
./bin/havi
```

HAVI uses embedded home repo at `~/.config/HAVI/repo` by default.

Import a local directory into home repo:

```bash
export HPPR_HOME=unix+$HOME/.config/HAVI/repo/hppr.sock
export HPPR_SIGNER='!ring0/init'
pylon nfs mount /mnt/hppr --root //u/showcase --rw --seal-with oldest
cp -a showcase/. /mnt/hppr/
pylon nfs unmount /mnt/hppr
```

Open in HAVI address bar:

```text
hppr://u/showcase/index.html
```

You can also run against external repo:

```bash
./bin/hpprd --bind 127.0.0.1:4777
HAVI_HOME=tcp+127.0.0.1:4777 ./bin/havi
```

## Simple JS page using HPPR API

CLI prerequisites for this section:

```bash
export HPPR_HOME=unix+$HOME/.config/HAVI/repo/hppr.sock
export HPPR_SIGNER='!ring0/init'
```

HAVI embedded repo must be running.
If you use an external `hpprd`, set `HPPR_HOME` to that endpoint instead.

Create content with CLI:

```bash
echo 'Hello from HPPR' | ./bin/hppr add //u/demo/data/msg.txt
cat > /tmp/index.html <<'HTML'
<!doctype html>
<meta charset="utf-8">
<h1>HAVI API demo</h1>
<pre id="out">loading...</pre>
<script>
(async () => {
  try {
    const p = await window.home.get('//u/demo/data/msg.txt');
    const text = await p.text();
    const lines = [];
    lines.push(`Current address: ${window.address.href}`);
    lines.push(`Packet hash: ${p.hash}`);
    lines.push(`Coordinate: ${p.coordinate}`);
    lines.push(`Data: ${text}`);

    if (window.route) {
      lines.push('Route client: available');
    } else {
      lines.push('Route client: null');
    }

    document.getElementById('out').textContent = lines.join('\n');
  } catch (e) {
    const msg = `${e.type || e.name}: ${e.detail || e.message}`;
    document.getElementById('out').textContent = msg;
  }
})();
</script>
HTML
pylon nfs mount /mnt/hppr --root //u/demo/site --rw --seal-with oldest
cp -a /tmp/. /mnt/hppr/
pylon nfs unmount /mnt/hppr
```

Open:

```text
hppr://u/demo/site/index.html
```

## Browser JS API map (what you use most)

Globals:

- `window.address` — HPPR-aware address object (navigation by assignment)
- `window.home` — local `HpprClient`
- `window.route` — routed `HpprClient | null`
- `window.ring0` — privileged client on setup/editor/internal pages
- `document.packet` — source `HpprPacket` for `hppr://` documents

`HpprClient` core methods:

- read: `get`, `headers`, `list`, `tips`, `members`
- write: `add`, `store`, `detach`
- session/status: `hello`
- live: `watch`, `streamIn`, `streamOut`
- debug envelope: `envelope()` wrapper

Live primitives:

- `WatchSocket`: `+ <coord>` / `- <coord>` events
- `StreamIn`: publish trailer-format segments
- `StreamOut`: subscribe as `ReadableStream<Uint8Array>`

Embedding primitive:

- `<x src="...">` for HPPR-native embedding
- optional `trustParent` to reuse parent trust set

JSONqa for client-side view state on coordinates:

- example: `//docs/app/page{page:5,#:results}`
- accessible via `URC.qa` and `URC.fragment`

## Debugger CLI

Start HAVI with DevTools port:

```bash
./bin/havi --devtools 6000 hppr://u/showcase/index.html
```

Evaluate JS from terminal:

```bash
./bin/havi-devtools-cli -p 6000 eval 'window.address.href'
./bin/havi-devtools-cli -p 6000 eval 'document.packet && document.packet.hash'
```

## What to read next

- HPPR protocol quickstart: `QUICKSTART-HPPR.md`
- Full protocol spec: `docs/spec/hppr/010-PACKETS.md` through
  `docs/spec/hppr/080-REPLICATION.md`
- Full browser spec: `docs/spec/havi/010-OVERVIEW.md` through
  `docs/spec/havi/080-PUBLISHING.md`
