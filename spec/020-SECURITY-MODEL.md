# Security Model

HAVI security has three layers:

1. route and deployment resolution
2. site sandboxing
3. repo ACL enforcement

## Route and deployment resolution

HAVI keeps origin semantics stable while resolving content
through local route and remote deployment pointers.

Origin format: `//<group>/<app>/`.

For routed non-repo pages (`hppr://<group>/<app>/...`):

1. Read local route packet from home repo:
   `//repo/admin/route/<group>/<app>/|/seal/<home-repo-vkey>`
2. Connect to route upstream.
3. Read remote deployment packet:
   `//<group>/admin/deploy/<app>/|/seal/<remote-repo-vkey>`
4. Use deployment headers:
   - `Deploy-Root: //<...>`
   - `Deploy-Signer: V.<...>.H3`
5. Build target coordinate by appending requested location to `Deploy-Root`.
6. Execute:
   - GET: `<target>/|/seal/<Deploy-Signer>`
   - LIST: `<target>/`

The page origin remains `//<group>/<app>/` even when content resolves to a
different coordinate under `Deploy-Root`.

### Route packet

Coordinate:

`//repo/admin/route/<group>/<app>/|/seal/<repo-vkey>`

Typical headers:

- `Upstream`
- `Upstream-Verification-Key`

Route config is local. Only ring0 can write route packets.

### Deployment packet

Coordinate:

`//<group>/admin/deploy/<app>/|/seal/<repo-vkey>`

Required headers:

- `Deploy-Root`
- `Deploy-Signer`

Deployment packet is evaluated on the upstream repo. It controls what content
coordinate and signer back the routed origin.

### Route keys

Per-group route keys provide Ring2 identity for authenticated remote operations.

Coordinate:

`//repo/admin/route-keys/<group>/|/seal/<repo-vkey>`

Headers:

- `Secret-Key: &.<b64a>.H3`
- `Verification-Key: V.<b64a>.H3`

Route keys are secrets. Access is restricted to ring0 and site Ring1 accounts
with explicit ACL grants.

### Bootstrap index fallback

If no local route packet exists for `//<group>/<app>/`, HAVI may resolve one
from the public bootstrap index.

Lookup target:

`//u/index/<group>/<app>`

Rules:

- groups starting with `~` are local/private and skip bootstrap lookup
- bootstrap lookup responses must be Seals signed by the configured bootstrap
  verification key
- on success, HAVI uses the returned `Upstream` and
  `Upstream-Verification-Key` values and attempts to store a local route packet

If lookup is skipped, missing, or invalid, HAVI falls back to the home repo
endpoint.

### Setup flow

For a new direct endpoint, HAVI:

1. fetches remote HELLO
2. compares current local route endpoint
3. asks the user to approve route update
4. stores route config locally
5. creates route key for group if not exists
6. navigates to normal `hppr://` URL

## Site sandboxing

Each site origin gets its own Ring1 identity for home repo isolation.

Ring1 name format:

`site:<group>#<app>`

Sites are isolated across group/app origins.

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

`window.home`: Ring1 auth with `site:<group>#<app>` key.

`window.route`: Ring2 auth with route key for group.

## Enforcement

ACL checks run in `hpprd`, not in page JavaScript.

When page code calls `window.home` APIs:

1. HAVI signs request with the site Ring1 key
2. repo verifies signature and session
3. repo applies ACL rules
4. repo allows or denies

Privilege escalation requires explicit ring0 approval for proxy actions.

## Security notes

- HPPR origins are secure contexts.
- XSS still applies when apps render untrusted content unsafely.
- Route key compromise affects all routes in the group.
- Deployment pointer compromise affects routed content selection for that app.
