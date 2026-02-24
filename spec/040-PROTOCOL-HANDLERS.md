# Protocol Handlers

Protocol handlers map HAVI URLs to HPPR operations.

## URL form

General form:

`scheme://group/app/location{via:endpoint}`

`{via:endpoint}` is optional in most cases.

### Endpoint transport

When `via` is present, HAVI connects directly to that endpoint.

| `via` value      | Transport | Default port |
|------------------|-----------|--------------|
| `host`           | TCP       | 4777         |
| `host:port`      | TCP       | none         |

Packet signatures provide integrity and authorship regardless of transport.

## Supported schemes

- `hppr://` for normal content navigation
- `hppr-setup://` for trust and route setup
- `hppr-sandbox://` for untrusted preview
- `hppr-browse://` for directory browsing
- `hppr-editor://` for local packet editing
- `file://` for local file rendering with home repo access
- `havi://` for internal pages

## `hppr://`

Primary browsing scheme.

### Content fetch

A path without trailing `/` performs GET and renders packet data.

Content type comes from extension mapping unless packet `Content-Type` exists.

### Directory listing

A path with trailing `/` performs LIST and renders directory HTML.

### Chunk manifest handling

If GET returns a chunk manifest (`Chunk+Link` + `Data-Length: 0`), HAVI:

1. fetches chunk packets
2. resolves nested manifests to depth 8
3. verifies `Content-Hash-Full` when present
4. serves reassembled content to the page

`document.packet` exposes the rendered packet.
For manifest-level inspection, use envelope/raw APIs in `060-JS-API.md`.

Publishers should use smaller chunk sizes for seek-heavy media.

### Direct endpoint redirect

If a direct endpoint URL has no local route config, HAVI redirects to setup:

`hppr-setup://group/app/path{via:endpoint}`

## `hppr-setup://`

Setup flow for new route endpoint trust.

This scheme exposes `window.ring0` so the setup page can store local route/trust
packets after user approval.

Setup pages may embed untrusted preview through `hppr-sandbox://`.

## `hppr-sandbox://`

Untrusted preview mode.

- anonymous access only
- strict CSP
- JavaScript and active features blocked

## `hppr-browse://`

Directory explorer for coordinate trees.

## `hppr-editor://`

Local editor with `window.ring0` and `window.home`.

- edit headers and data
- save via `ADD`
- redirect to `hppr://` on success

Endpoint is forbidden. Editor always targets localhost context.

## `file://`

Local filesystem content rendered as an HPPR HTML page.

- `window.home` available (site identity `site:file#local`)
- `window.route` is `null` (no remote endpoint)
- `window.ring0` is `null` (not a privileged scheme)
- `document.packet` is `null` (no HPPR packet)
- `<x>` elements work (child frames load via their own scheme)
- disabled web APIs installed (same as `hppr://`)
- content type from file extension
- directory paths render HTML listing

All `file://` pages share one origin and one site Ring1 identity.

## `havi://`

Internal admin pages for overview, routes, home-repo status, ring0 proxy
approvals, and pylon service management.

`havi:///services` exposes pylon controls for:

- service start/stop
- hpprd listen/unlisten
- mount/unmount
- status and mounts inspection

All `havi://` pages have pre-authorized `window.ring0` access.
