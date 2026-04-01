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
If no local route exists and `group` does not start with `~`, HAVI resolves the
public network:

- group `u`: `//u/network/app/<app>`
- other public groups:
  1. split the group on `.`
  2. fetch `//u/network/group/<rightmost-label>`
  3. walk leftward one label at a time via
     `//<resolved-parent-group>/network/group/<next-child-label>`
  4. fetch `//<resolved-group>/network/app/<app>`

For non-`u` groups, `Content-Authority` may fall back from the group app record
to `//u/network/app/<app>`.
That fallback applies only to `Content-Authority`, never to `Upstream`.
If public-network resolution fails for a public name, navigation fails.
HAVI does not silently fall back to the home repo for that case.

## JavaScript globals

HAVI exposes:

- `window.home`: local home repo client
- `window.route`: primary route repo client

See `060-JS-API.md` for full API details.
