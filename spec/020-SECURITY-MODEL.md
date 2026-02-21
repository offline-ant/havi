# Security Model

HAVI security has three layers:

1. trust decisions
2. site sandboxing
3. repo ACL enforcement

## Trust decisions

HAVI enables JavaScript only when packet signer keys are trusted for the page
origin.

Origin format: `//<group>/<app>/`.

Trust config is local site-trust data. Route config and trust config are
separate:

- route controls where packets are fetched
- site-trust controls which signer keys may run code

### Site-trust packet

Coordinate:

`//<group>/<app>/site-trust/|/seal/<repo-vkey>`

Content uses membership headers such as `Member` and `Member-Delegate`.

`repo-vkey` is the home repo's oldest ring0 verification key, returned in HELLO
`Seal-By`.

### Route packet

Coordinate:

`//repo/admin/route/<group>/<app>/|/seal/<repo-vkey>`

Typical headers:

- `Upstream`
- `Upstream-Verification-Key`

Route endpoint keys authenticate endpoint identity. They do not authorize site
JavaScript.

Route config is stored in the home repo admin namespace. Only ring0 can write
route packets.

### Route keys

Per-group route keys provide Ring2 identity for authenticated remote operations.

Coordinate:

`//repo/admin/route-keys/<group>/|/seal/<repo-vkey>`

Headers:

- `Secret-Key: &.<b64a>.H3`
- `Verification-Key: V.<b64a>.H3`

Route keys are secrets. Access is restricted to ring0 and site Ring1 accounts
with explicit ACL grants. Route keys are shared by CLI and HAVI.

### Setup flow

For a new direct endpoint, HAVI:

1. fetches HELLO
2. fetches remote trust info
3. asks the user to approve
4. stores route config and trust packets locally
5. creates route key for group if not exists
6. navigates to normal `hppr://` URL

Revocation is local trust editing. Remove keys from site-trust or detach the
trust packet.

## Site sandboxing

Each site origin gets its own Ring1 identity for home repo isolation.

Ring1 name format:

`site:<group>#<app>`

Sites are isolated across group/app origins, including origins signed by the
same publisher key.

### Default site ACL pattern

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

This allows local app state and blocks cross-site access.

### Remote identity

Remote (Ring2) operations use the per-group route key, not the site key. This
separates home sandbox isolation (per-app) from remote group identity
(per-group).

`window.home`: Ring1 auth with `site:<group>#<app>` key.

`window.route`: Ring2 auth with route key for group.

## Enforcement

ACL checks run in `hpprd`, not in page JavaScript.

When page code calls `window.home` APIs:

1. HAVI signs the request with the site Ring1 key
2. repo verifies signature and session
3. repo applies ACL rules
4. repo allows or denies

Privilege escalation requires explicit ring0 approval for proxy actions.

## Security notes

- HPPR origins are secure contexts.
- XSS still applies when apps render untrusted content unsafely.
- CSRF behavior differs because identities are per-site.
- Delegated trust chains increase compromise blast radius.
- Route key compromise affects all routes in the group. Per-group isolation is
  the default. Users who want stronger isolation can create separate route keys.
