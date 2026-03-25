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

- what `window.home` may do against the home repo
- what `window.route` may do against routed upstream repos
- ACL-bound read, write, and list access

Repo capability is independent from app origin and content authority.
A page may share app origin with another page while having different content
authority, different repo capability, or both.

## Site isolation and ACLs

Each site origin gets its own Ring1 identity for home repo isolation.

Ring1 name format:

`site:<group>#<app>`

Typical rules:

- read/list site namespace
- write only under `//<group>/<app>/user/`
- access own Ring1 admin area for proxy requests
- read route keys for authenticated remote access

```text
ACL-Rule: rdl //<group>/<app>/
ACL-Rule: rwl //<group>/<app>/user/
ACL-Rule: rwl //repo/admin/ring1/site:<group>#<app>/
ACL-Rule: r.. //repo/admin/route-keys/
```

`window.home`: Ring1 auth with `site:<group>#<app>` key.

`window.route`: Ring2 auth with route key for group.

ACL checks run in `hpprd`, not in page JavaScript.

## Security notes

- HPPR origins are secure contexts.
- XSS still applies when apps render untrusted content unsafely.
- Route key compromise affects routed repo capability for that group.
- App content pointer compromise affects which content root and signer back an
  app URL.
- Content-signer mismatch causes `<x policy="auto">` to isolate the child.
- `file://` pages use a browser-defined local origin and local site identity.
