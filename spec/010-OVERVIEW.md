# HAVI Overview

HAVI is a web browser for HPPR content.

## Core capabilities

HAVI can:

- Open repos with `hppr://group/api//key{via:endpoint}`.
- Open `hppr://group/api//key` using the HPPR route scheme effective resolver.
- Store browser-owned local route, trust, history, and capability state in its
  private runtime.
- Apply CSS transforms structurally to rendered subtrees so descendant text,
  images, stacking contexts, scroll clips, and iframe content render in the
  transformed coordinate space of their owning element.

## URL entry and routing

Supported input forms:

- `hppr://group/api//key{via:endpoint}`
- `hppr://group/api//key`

`via` selects an explicit upstream endpoint when present.
Endpoint syntax follows HPPR via syntax.
Without `via`, HAVI applies the HPPR route scheme effective resolver from
`../../hppr/spec/schemes/100-SCHEMES.md`.

That resolver combines local exact-API records, local exact-group anchors,
canonical public route discovery, and local route auth attachment.
Route packet structure and resolver semantics are defined by the HPPR route
scheme, not by HAVI-specific packet rules.

For non-`u` groups, `Content-Authority` may fall back from the group API record
to `//u/route/api//<api>`.
That fallback applies only to `Content-Authority`, never to `Upstream`.
If canonical public lookup fails for a public name and no local exact-group or
terminal local exact-API record supplies the effective route answer,
navigation fails.
HAVI does not silently fall back to a generic browser-local repo fetch for that
case. For non-public or otherwise browser-local sources, the browser may still
select the browser-local repo source as the effective source. That case is not
route-backed.

## JavaScript globals

HAVI exposes one ordinary-page ambient repo story:

- `window.source`: browser-owned committed-source descriptor with:
  - `client`
  - `authority`
  - `kind`

Ordinary pages do not expose `window.home`, `window.route`, or `window.ring0`.
Explicit extra repo access uses browser-mediated named clients.
Surviving internal helper pages use page-owned `havi:///.../api?...` backends
instead of a generic privileged JS object.

See `060-JS-API.md` for full API details.
