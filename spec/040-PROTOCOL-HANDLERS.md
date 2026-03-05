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
- `hppr-setup://` for route setup
- `hppr-join://` for Ring2 membership requests
- `hppr-sandbox://` for untrusted preview
- `hppr-browse://` for directory browsing
- `hppr-editor://` for local packet editing
- `file://` for local file rendering with home repo access
- `havi://` for internal pages

## `hppr://`

Primary browsing scheme.

### Routed non-repo resolution

For routed non-repo pages (`hppr://<group>/<app>/...`), HAVI resolves in this
order:

1. local route packet → endpoint (`//repo/admin/route/<group>/<app>/|/...`)
2. if route is missing and `group` does not start with `.`, try bootstrap index
   lookup at `//u/index/<group>/<app>` and use/store returned route values
3. remote deploy packet (`//<group>/admin/deploy/<app>/|/seal/<repo-vkey>`)
4. target from `Deploy-Root` + requested location

Fetch behavior:

- GET uses sealed target: `<target>/|/seal/<Deploy-Signer>`
- LIST uses unsealed target: `<target>/`

Origin remains `//<group>/<app>/`.

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

### Direct endpoint redirect

If a direct endpoint URL has no local route config, HAVI redirects to setup:

`hppr-setup://group/app/path{via:endpoint}`

### Ring2 unauthorized redirect

For routed non-repo content, if remote GET or LIST returns `UNAUTHORIZED`,
HAVI redirects to:

`hppr-join://group/app/`

## `hppr-setup://`

Setup flow for route configuration.

This scheme exposes `window.ring0` so the setup page can store local route
packets after user approval.

Setup pages may embed untrusted preview through `hppr-sandbox://`.

## `hppr-join://`

Ring2 membership request flow.

The page is local HTML and receives `window.route` credentials for the target
route endpoint and group route key signer.

Join flow:

1. show group/app and requester route verification key
2. submit request with `window.route.add()` to `//<group>/admin/request/member/|`
3. watch `//<group>/admin/request/member/<requester-vkey>/reply/`
4. on `Request-Status: approved`, navigate to `hppr://<group>/<app>/`

Join fixture mode for deterministic tests can override join result handling with
process-local state:

- `none`: normal network join flow
- `pending`: show pending state without sending network request
- `approved`: navigate directly to `hppr://<group>/<app>/`

Fixture state is controlled from `havi:///diagnostics` API and resets when HAVI
restarts.

`hppr-join://` does not expose `window.ring0`.

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

Internal browser pages for local administration and diagnostics.

The `havi://` scheme is implementation-defined UI. The spec defines only the
handler-level behavior and privilege model, not specific page inventory or
layout.

`havi:///services` exposes pylon controls for:

- service start/stop
- hpprd listen/unlisten
- mount/unmount
- status and mounts inspection

`havi:///diagnostics` exposes route/deploy/auth/join diagnostics and fixture
controls.

`havi:///diagnostics/api` commands:

- `inspect` with `group`, `app`, optional `location`
- `join_fixture_get`
- `join_fixture_set` with `state=none|pending|approved`

Join fixture state is process-local and resets on restart.

All `havi://` pages have pre-authorized `window.ring0` access.
