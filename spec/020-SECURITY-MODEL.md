# Security Model

HAVI security has three distinct dimensions:

1. app origin
2. content authority
3. repo capability

They are separate. Matching one dimension does not imply matching the others.

## App origin

App origin is the browser composition boundary.

Origin format:

`//<group>/<app>/`

App origin controls ordinary browser relationships:

- same-origin DOM access under ordinary rules
- storage and window relationships
- app-space identity

For routed app URLs, origin remains `//<group>/<app>/` even when content is
fetched from a different coordinate under an app content pointer.

## Content authority

Content authority is the authority that defines executable or inheritable app
content.

Current concrete representation:

- app-content URLs use exact `Content-Authority` equality
- direct explicit Seal URLs use `Seal-By`
- unsealed direct content has no content authority
- `hppr-sandbox://` has no content authority

For app-content URLs, HAVI resolves the app content pointer at:

`//<group>/admin/deploy/<app>/|/seal/<repo-vkey>`

Required headers:

- `Content-Root`
- `Content-Authority`

The browser appends the requested location to `Content-Root` and fetches the
final document from:

`<target>/|/seal/<Content-Authority>`

That fetch path is a hard cutover. If `Content-Authority` changes, old signer
content is no longer reachable through the app URL.

### `<x>` authority comparison

`<x>` compares parent and child content authority by exact verification-key
equality.

`//<group>/<app>` equality is not enough.
Route equality is not enough.
Repo endpoint equality is not enough.

Default `policy="auto"` behavior:

- same content authority -> `inherited`
- different content authority -> `isolated`
- missing child content authority -> `isolated`
- `hppr-sandbox://` -> `sandbox-preview`

Cross-signer default is `isolated`.

## Repo capability

Repo capability is the HPPR API authority exposed through browser-managed
clients.

It controls:

- what the committed `window.source.client` may do for the current document
- what explicit named clients may do when the user granted them to the page
- what explicit helper-only internal capability paths may do on privileged pages
- ACL-bound read, write, and list access on the backing repo

Repo capability is independent from app origin and content authority.
A page may share app origin with another page while having different content
authority, different repo capability, or both.

## Capability tiers

HAVI keeps repo capability in three distinct tiers:

1. ambient committed source (`window.source.client`)
2. explicit named clients (`HpprClient.named(name)` after browser grant)
3. explicit internal helper page APIs (`havi:///.../api?...` on internal pages only)

Ordinary pages do not get ambient `window.home` or ambient `window.route`.
Extra repo power is not acquired by raw arbitrary `connect*()`.
It is acquired through browser mediation.

## Browser-owned local state

HAVI still keeps local route, trust, cache, history, and capability state in its
browser-owned runtime.
That state is not itself an ordinary page API and does not imply a per-origin
ambient Ring1 identity on the page surface.

ACL checks run in `hpprd`, not in page JavaScript.

## Security notes

- HPPR origins are secure contexts.
- XSS still applies when apps render untrusted content unsafely.
- Local route auth compromise affects routed repo capability for the affected
  group or app scope.
- App content pointer compromise affects which content root and signer back an
  app URL.
- Content-signer mismatch causes `<x policy="auto">` to isolate the child.
- `file://` pages use a browser-defined local origin. `window.source === null`
  there by default.
