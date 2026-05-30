# Protocol Handlers

Protocol handlers map browser URLs to HPPR operations.

## URL form

Canonical coordinate URL form:

`scheme://<group>/<api>//<key>{via:endpoint}`

`{via:endpoint}` is optional. The empty path segment between API and Key is the API/Key delimiter and must be preserved before generic URL path normalization. Direct hash URLs use `hppr:////<hash>` and are parsed as immutable hash addresses, not coordinates.

The address bar, `document.URL`, `document.documentURI`, and `window.address.href` preserve the `//<api>//<key>` delimiter for HPPR document URLs.

Group landing shorthand `hppr://<group>` resolves through the group's `Home-API` when a route answer supplies one, otherwise through the implementation fallback `home`. The resulting document fetch targets `//<group>/<home-api>//index.html`.

Relative links on HPPR documents resolve inside the current Key. `./x`, `x`, and `../x` change only Key segments. `../` cannot ascend above the Key root. A leading `/x` targets Key `/x` in the current group and API. Absolute `//<group>/<api>//<key>` coordinates may cross the API/Key boundary. Encoded slash bytes remain data inside the segment where they appear and do not create API or Key separators.

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

For routed non-repo pages (`hppr://<group>/<api>//...`), the browser applies the
HPPR route scheme effective resolver from `../../hppr/spec/100-SCHEMES.md`.

That resolver combines:

1. browser-local exact-API route records
2. browser-local exact-group route anchors
3. canonical public route discovery for public names
4. browser-local route auth attachment with exact-API override and group fallback
5. remote API content pointer resolution

The packet structure and merge rules for route records are defined by the HPPR
route scheme. This spec only states browser behavior on top of that scheme.

Public-network rules:

- for group `u`, `//u/route/api//<api>` supplies endpoint, optional repo pin,
  and optional `Content-Authority`
- for non-`u` groups, API `Upstream` inherits from the group record when
  omitted
- for non-`u` groups, `Content-Authority` falls back to `//u/route/api//<api>`
  when the group API record omits it; only `Content-Authority` is inherited
  from public API defaults, never `Upstream`
- missing `//<group>/route/api//<api>` is a canonical public discovery failure
- effective resolution MAY still succeed when a terminal local exact-API record
  provides the exact route answer for `//<group>/<api>`
- when public-network resolution produces an effective `Content-Authority`, the
  browser MUST require exact equality with the deploy pointer
  `Content-Authority`
- if canonical public lookup fails for a public name and no local exact-group
  or terminal local exact-API record provides the effective route answer, the
  browser MUST fail navigation instead of silently falling back to a
  browser-local repo source
- an effective local route answer, including a terminal local exact-API
  bootstrap, is still a route-backed result; it does not convert navigation
  into a generic browser-local document fetch
- when effective resolution selects a browser-local repo source instead of a
  routed endpoint (for example by browser-local policy such as `repo` or a
  non-public local fallback), the source is not route-backed

Fetch behavior:

- GET uses sealed target: `<target>/|/seal/<Content-Authority>`
- LIST uses unsealed target: `<target>/`

This is a hard cutover fetch path. When `Content-Authority` changes, older signer
content is no longer reachable through the API URL.

Origin remains `//<group>/<api>/`.

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

- `window.source` is `null`
- no generic privileged helper JS object is exposed
- `document.packet` is `null`
- `document.URC` is `null`
- `document.URL` is the stripped file document URL without JSONqa view state
- `document.documentURI` matches `document.URL`
- `window.address` is the exact native file address surface and preserves
  canonical `{...}` JSONqa state
- `window.location` is a compatibility shim, not the native `Location` object
- legacy native file `?` and `#` input normalize into file JSONqa during
  navigation; mixed native `?`/`#` with explicit JSONqa is invalid
- content type from file extension
- directory paths render HTML listing

Filesystem I/O, origin, and relative-base resolution use the stripped file URL.
JSONqa never becomes part of the filesystem path.

All `file://` pages share one browser-defined local origin and one
browser-defined local permission scope.

## Helper schemes

Helper documents such as `havi:///diagnostics` are browser-owned pages.

- `window.address` exists natively and exposes the exact helper `href` and
  `scheme`
- `window.address.qa` is `null`
- `document.URL` is the helper document URL
- `document.documentURI` matches `document.URL`
- `document.URC` is `null`
- `document.packet` is `null` unless one real HPPR packet was loaded into that
  helper page for rendering
- `window.location` keeps normal platform behavior; the HAVI compatibility shim
  is only installed on `hppr://` and `file://` pages

Unsupported non-HAVI schemes keep normal platform behavior and do not receive a
dummy HPPR fallback address surface.
