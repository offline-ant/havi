# HAVI Overview

HAVI is a web browser for HPPR content.

## Core capabilities

HAVI can:

- Open repos with `hppr://group/app/path{via:endpoint}`.
- Open `hppr://group/app/path` using the HPPR route scheme effective resolver.
- Store local route and trust state in the home repo.
- Apply CSS transforms structurally to rendered subtrees so descendant text,
  images, stacking contexts, scroll clips, and iframe content render in the
  transformed coordinate space of their owning element.

## URL entry and routing

Supported input forms:

- `hppr://group/app/path{via:endpoint}`
- `hppr://group/app/path`

`via` selects an explicit upstream endpoint when present.
Endpoint syntax follows HPPR via syntax.
Without `via`, HAVI applies the HPPR route scheme effective resolver from
`../../hppr/spec/100-SCHEMES.md`.

That resolver combines local exact-app records, local exact-group anchors,
canonical public route discovery, and local route auth attachment.
Route packet structure and resolver semantics are defined by the HPPR route
scheme, not by HAVI-specific packet rules.

For non-`u` groups, `Content-Authority` may fall back from the group app record
to `//u/route/app/<app>`.
That fallback applies only to `Content-Authority`, never to `Upstream`.
If canonical public lookup fails for a public name and no local exact-group or
terminal local exact-app record supplies the effective route answer,
navigation fails.
HAVI does not silently fall back to a generic home-repo fetch for that case.
For non-public or otherwise home-resolved sources, the browser may still select
the home repo as the effective source. That case is not route-backed.

## JavaScript globals

HAVI exposes:

- `window.home`: local home repo client
- `window.route`: primary route repo client

See `060-JS-API.md` for full API details.
