# Runtime

This spec defines browser-visible runtime concepts for HPPR browsing.

Implementation-specific shell behavior, local database layout, watch controls,
shadow workflows, diagnostics pages, and service management belong in
`../reference.md`.

## Home repo role

An HPPR browser uses a home repo for persistent local state.

Typical home-repo uses include:

- cached packets
- local user data
- route configuration
- trust configuration
- locally authored content

The storage backend and local process model are implementation-defined.

## Clients exposed to page code

- `window.home` targets the home repo and is always available.
- `window.route` targets the selected route repo when one is available.

`window.route` may be `null` when no route exists, no usable route identity is
available, or the current document has no route-backed source.

## Per-origin site identity

Browsers isolate home-repo access by origin.

HAVI uses one Ring1 identity per `//<group>/<app>/` origin. Other
implementations may use a different storage mechanism while preserving the same
origin isolation.

## Local persistence

Route fetches may be cached in the home repo.

Browsers may persist additional local state for history, credentials, authoring,
and shell integration. The format and storage location are implementation-
defined.

## Inline SVG runtime model

Inline `<svg>` participates in the browser's native style, layout, fragment,
and paint pipeline.

Required runtime behavior:

- inline SVG is not serialized to a temporary image URL for layout or first
  paint
- `use`, gradients, and clip paths resolve through the native SVG resource graph
- SVG text and `foreignObject` remain native fragment kinds even when feature
  coverage is partial
- external SVG image resources remain implementation-defined and may use a
  separate backend from inline SVG
