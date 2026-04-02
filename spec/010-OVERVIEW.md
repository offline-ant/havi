# HAVI Overview

HAVI is a web browser for HPPR content.

## Core capabilities

HAVI can:

- Open repos with `hppr://group/app/path{via:endpoint}`.
- Open `hppr://group/app/path` using route config from the home repo.
- Store local route and trust state in the home repo.
- Apply CSS transforms structurally to rendered subtrees so descendant text,
  images, stacking contexts, scroll clips, and iframe content render in the
  transformed coordinate space of their owning element.

## URL entry and routing

Supported input forms:

- `hppr://group/app/path{via:endpoint}`
- `hppr://group/app/path`

`via` selects an explicit upstream endpoint when present.
Without `via`, HAVI resolves route config from the home repo.

For `hppr://group/app/path`, HAVI resolves route config from the home repo.
If no local route record exists and `group` does not start with `~`, HAVI resolves the
public route:

- group `u`: `//u/route/app/<app>`
- other public groups:
  1. split the group on `.`
  2. fetch `//u/route/group/<rightmost-label>`
  3. walk leftward one label at a time via
     `//<resolved-parent-group>/route/group/<next-child-label>`
  4. fetch `//<resolved-group>/route/app/<app>`

For non-`u` groups, `Content-Authority` may fall back from the group app record
to `//u/route/app/<app>`.
That fallback applies only to `Content-Authority`, never to `Upstream`.
If public-network resolution fails for a public name, navigation fails.
HAVI does not silently fall back to the home repo for that case.

## JavaScript globals

HAVI exposes:

- `window.home`: local home repo client
- `window.route`: primary route repo client

See `060-JS-API.md` for full API details.
