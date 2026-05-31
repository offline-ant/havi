# HAVI Quickstart

This guide is the conceptual companion to `docs/README.md` in the package.
Use it to understand what HAVI renders, how content authority works, and how to
use the browser-side HPPR API.

For protocol depth beyond this page, read `QUICKSTART-HPPR.md` (included in this
package) and the full specs in `docs/spec/hppr/` and `docs/spec/havi/`.

## HPPR in 60 seconds

HPPR stores content as signed packets.

- **Blob (`B.`)**: raw bytes (`Data-Length` + data)
- **Plex (`P.`)**: metadata wrapper around a Blob (`Group`, `API`, `Key`,
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

- coordinate (latest by default): `//<group>/<api>//<key>`
- immutable by hash: `////<T.hash.H3>`
- version-pinned coordinate: `.../|/plex/<tai>/<hash>` or `.../|/seal/...`

HAVI is a browser on top of this model.

## HAVI model

HAVI has a browser-owned local runtime plus committed document sources.

Ordinary pages get one ambient repo story:

- `window.source`: committed source descriptor
- `window.source.client`: client for the actual committed source
- `window.source.kind`: `"repo"` or `"remote"`

Ordinary pages do not get `window.home` or `window.route`.
Extra repo power uses browser-mediated named clients instead of a second ambient
repo handle.

Route and API content pointer config are separate. Route packet structure,
local route auth storage, and identity text are general HPPR route-scheme
behavior.

Route and API content pointer config are separate:

- route decides **which upstream repo** is used
- API content pointer decides **which content root and signer** back `//<group>/<api>//`

Origin boundary is HPPR-native: `//<group>/<api>/`.

## URL schemes and handlers

General form:

```text
scheme://group/api//key{via:endpoint}
```

Supported schemes:

- `hppr://` — normal content navigation
- `hppr-sandbox://` — untrusted preview (JS blocked, strict CSP)
- `hppr-browse://` — coordinate tree browser
- `havi://` — internal privileged pages (`diagnostics`)

Routing behavior:

- `{via:endpoint}` forces a direct upstream endpoint
- `hppr://...` without `via` uses automatic route lookup from browser-local route state

Trailing slash means LIST view; no trailing slash means GET and render content.

## Content authority and execution rules

For routed origins, HAVI resolves content through a group API content pointer
(`//<group>/admin/deploy//<api>/|`) that declares `Content-Root` and
`Content-Authority`.

ACL enforcement still happens server-side in `hpprd` for every command.

Ordinary pages do not get hidden per-site Ring1 credentials.
Ambient repo access comes only from the committed `window.source` descriptor,
and extra repo power requires an explicit named-client grant.

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

Default desktop HAVI browsing now uses the browser-owned packet store at
`~/.config/HAVI/havi-packets.sqlite` when `HAVI_HOME` is unset. Browser-local
non-packet state stays in `~/.config/HAVI/havi.sqlite`.

Explicit `hpprd` workflows remain valid for authoring, publishing, and
operator tasks, but they are not required for ordinary browsing.

Open in HAVI address bar:

```text
hppr://u/showcase//index.html
```

You can also run against external repo:

```bash
./bin/hpprd --bind 127.0.0.1:4777
HAVI_HOME=tcp+127.0.0.1:4777 ./bin/havi
```

## Simple JS page using HPPR API

CLI prerequisites for this section:

```bash
./bin/hpprd --path ./repo --bind 127.0.0.1:4777 --daemon
export HPPR_HOME=tcp+127.0.0.1:4777
export HPPR_SIGNER='ring1:ring0|init'
```

This CLI workflow targets the `hpprd` named by `HPPR_HOME`; it does not write
into HAVI's browser-local `havi-packets.sqlite` store.

Create content with CLI:

```bash
echo 'Hello from HPPR' | ./bin/hppr add //u/demo//data/msg.txt
cat > /tmp/index.html <<'HTML'
<!doctype html>
<meta charset="utf-8">
<h1>HAVI API demo</h1>
<pre id="out">loading...</pre>
<script>
(async () => {
  try {
    if (!window.source) throw new Error('No repo-backed source');
    const p = await window.source.client.get('//u/demo//data/msg.txt');
    const text = await p.text();
    const lines = [];
    lines.push(`Current address: ${window.address.href}`);
    lines.push(`Source kind: ${window.source.kind}`);
    lines.push(`Packet hash: ${p.hash}`);
    lines.push(`Coordinate: ${p.coordinate}`);
    lines.push(`Data: ${text}`);

    document.getElementById('out').textContent = lines.join('\n');
  } catch (e) {
    const msg = `${e.type || e.name}: ${e.detail || e.message}`;
    document.getElementById('out').textContent = msg;
  }
})();
</script>
HTML
hppr-fuse --home "$HPPR_HOME" --signer "$HPPR_SIGNER" \
  --root //u/demo/site// --mount /mnt/hppr --rw --seal-with ring0 &
cp -a /tmp/. /mnt/hppr/
fusermount3 -u /mnt/hppr
```

Open:

```text
hppr://u/demo/site//index.html
```

## Browser JS API map (what you use most)

Globals:

- `window.address` — HPPR-aware address object (navigation by assignment)
- `window.source` — committed source descriptor for ordinary repo-backed pages
- internal helper pages use page-owned `havi:///.../api?...` endpoints instead of a generic privileged JS client
- `document.packet` — source `HpprPacket` for `hppr://` documents

`HpprClient` core methods:

- read: `get`, `headers`, `list`, `tips`, `members`
- write: `add`, `store`, `detach`
- session/status: `hello`
- transport bridge: `envelope()` wrapper

`EnvelopeHpprClient` transport methods:

- remote connect: `connect(endpoint, identity?)`
- live: `watch`, `streamPub`, `streamSub`

Live primitives:

- `WatchSocket`: `+ <coord>` / `- <coord>` events
- `StreamPub`: publish trailer-format segments
- `StreamSub`: subscribe as `ReadableStream<Uint8Array>`

Embedding primitive:

- `<x src="...">` for HPPR-native embedding
- optional `policy="auto|isolated|strict"` to control embed mode
- readonly `embedMode` and `contentAuthority` for diagnostics

JSONqa for client-side view state on coordinates:

- example: `//docs/app//page{page:5,#:results}`
- accessible via `URC.qa` and `URC.fragment`

## Debugger CLI

Start HAVI with DevTools port:

```bash
./bin/havi --devtools 6000 hppr://u/showcase//index.html
```

Evaluate JS from terminal:

```bash
./bin/havi-devtools-cli -p 6000 eval 'window.address.href'
./bin/havi-devtools-cli -p 6000 eval 'document.packet && document.packet.hash'
```

## What to read next

- HPPR protocol quickstart: `QUICKSTART-HPPR.md`
- Full HPPR specs: `docs/spec/hppr/010-PACKETS.md`,
  `docs/spec/hppr/030-COMMAND-MESSAGES.md`, and `docs/spec/hppr/080-REPLICATION-AND-STREAMS.md`
- Full browser spec: `docs/spec/havi/010-OVERVIEW.md` through
  `docs/spec/havi/080-PUBLISHING.md`
