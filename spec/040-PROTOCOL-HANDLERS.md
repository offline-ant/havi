# Protocol Handlers

Protocol handlers map browser URLs to HPPR operations.

## URL form

General form:

`scheme://group/app/location{via:endpoint}`

`{via:endpoint}` is optional.

### Endpoint transport

When `via` is present, the browser connects directly to that endpoint.

`via` uses HPPR core via syntax from `../../hppr/spec/031-VIA-SYNTAX.md`.
Common forms include `host`, `host:port`, `tcp+host`, `quib+host:4776`,
`ws+host`, and `unix+/absolute/path`.

Packet signatures provide integrity and authorship regardless of transport.

## Supported schemes

This spec defines browser behavior for:

- `hppr://` for HPPR content navigation
- `file://` for local file rendering with HPPR browser APIs

Additional implementation-specific schemes may exist. HAVI-specific helper
schemes are documented in `../reference.md`.

## `hppr://`

Primary browsing scheme.

### Routed resolution

For routed non-repo pages (`hppr://<group>/<app>/...`), the browser applies the
HPPR route scheme effective resolver from `../../hppr/spec/100-SCHEMES.md`.

That resolver combines:

1. local exact-app route records in the home repo
2. local exact-group route anchors in the home repo
3. canonical public route discovery for public names
4. local route auth attachment with exact-app override and group fallback
5. remote app content pointer resolution

The packet structure and merge rules for route records are defined by the HPPR
route scheme. This spec only states browser behavior on top of that scheme.

Public-network rules:

- for group `u`, `//u/route/app/<app>` supplies endpoint, optional repo pin,
  and optional `Content-Authority`
- for non-`u` groups, app `Upstream` inherits from the group record when
  omitted
- for non-`u` groups, `Content-Authority` falls back to `//u/route/app/<app>`
  when the group app record omits it; only `Content-Authority` is inherited
  from public app defaults, never `Upstream`
- missing `//<group>/route/app/<app>` is a canonical public discovery failure
- effective resolution MAY still succeed when a terminal local exact-app record
  provides the exact route answer for `//<group>/<app>`
- when public-network resolution produces an effective `Content-Authority`, the
  browser MUST require exact equality with the deploy pointer
  `Content-Authority`
- if canonical public lookup fails for a public name and no local exact-group
  or terminal local exact-app record provides the effective route answer, the
  browser MUST fail navigation instead of silently falling back to the home
  repo
- an effective local route answer, including a terminal local exact-app
  bootstrap, is still a route-backed result; it does not convert navigation
  into a generic home-repo document fetch
- when effective resolution selects the home repo instead of a routed endpoint
  (for example by browser-local policy such as `repo` or a non-public home
  fallback), the source is not route-backed

Fetch behavior:

- GET uses sealed target: `<target>/|/seal/<Content-Authority>`
- LIST uses unsealed target: `<target>/`

This is a hard cutover fetch path. When `Content-Authority` changes, older signer
content is no longer reachable through the app URL.

Origin remains `//<group>/<app>/`.

How a browser persists routes, asks for user approval, or offers join/setup
flows is implementation-defined.

Auth selection is local policy.
Public route records do not carry auth metadata.
When no local route auth record exists, routed access defaults to `anyone`.

### Content fetch

A path without trailing `/` performs GET and renders packet data.

Content type comes from extension mapping unless packet `Content-Type` exists.

### Directory listing

A path with trailing `/` performs LIST and renders directory HTML.

### Chunk manifest handling

If GET returns a chunk manifest (`Chunk+Link` + `Data-Length: 0`), the browser:

1. fetches chunk packets
2. resolves nested manifests to depth 8
3. verifies `Content-Hash-Full` when present
4. serves reassembled content to the page

`document.packet` exposes the rendered packet.
For manifest-level inspection, use envelope/raw APIs in `060-JS-API.md`.

## `file://`

Local filesystem content rendered as a browser page.

- `window.home` available
- `window.route` is `null`
- `window.ring0` is `null`
- `document.packet` is `null`
- `document.URC` is `null`
- `document.URL` is the stripped file document URL without JSONqa view state
- `window.address` is the exact file address surface and preserves canonical
  `{...}` JSONqa state
- `window.location` is a compatibility shim, not the native `Location` object
- legacy native file `?` and `#` input normalize into file JSONqa during
  navigation; mixed native `?`/`#` with explicit JSONqa is invalid
- content type from file extension
- directory paths render HTML listing

Filesystem I/O, origin, and relative-base resolution use the stripped file URL.
JSONqa never becomes part of the filesystem path.

All `file://` pages share one browser-defined local origin and one browser-defined
site identity.
