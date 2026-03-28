# Protocol Handlers

Protocol handlers map browser URLs to HPPR operations.

## URL form

General form:

`scheme://group/app/location{via:endpoint}`

`{via:endpoint}` is optional.

### Endpoint transport

When `via` is present, the browser connects directly to that endpoint.

| `via` value      | Transport | Default port |
|------------------|-----------|--------------|
| `host`           | TCP       | 4777         |
| `host:port`      | TCP       | none         |

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

For routed non-repo pages (`hppr://<group>/<app>/...`), the browser resolves in
this order:

1. local route packet -> endpoint
   (`//repo/admin/route/<group>/<app>/|/...`)
2. if no local route exists and `group` does not start with `~`, resolve the
   public network:
   - local override at `//repo/network/...`
   - fresh cached network record in home repo
   - remote public-network query
3. public group record at `//u/network/group/<group>`
4. delegated app record at `//<group>/network/app/<app>`
5. remote app content pointer
   (`//<group>/admin/deploy/<app>/|/seal/<repo-vkey>`)
6. target from `Content-Root` + requested location

Public-network rules:

- group `u` follows the same two-step path through `//u/network/group/u`
- app `Upstream` inherits from the group record when omitted
- `Content-Authority` inherits from `//u/network/app/<app>` only when the
  group app record explicitly includes `Content-Authority-Source: public`
- when public-network resolution produces an effective `Content-Authority`, the
  browser MUST require exact equality with the deploy pointer
  `Content-Authority`
- if public-network resolution fails for a public name, the browser MUST fail
  navigation instead of silently falling back to the home repo

Fetch behavior:

- GET uses sealed target: `<target>/|/seal/<Content-Authority>`
- LIST uses unsealed target: `<target>/`

This is a hard cutover fetch path. When `Content-Authority` changes, older signer
content is no longer reachable through the app URL.

Origin remains `//<group>/<app>/`.

How a browser persists routes, asks for user approval, or offers join/setup
flows is implementation-defined.

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
- disabled web APIs installed the same way as `hppr://`
- `window.location` is a compatibility shim, not the native `Location` object
- content type from file extension
- directory paths render HTML listing

All `file://` pages share one browser-defined local origin and one browser-defined
site identity.
