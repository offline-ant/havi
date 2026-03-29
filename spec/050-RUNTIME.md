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
- the fragment arena publishes native SVG viewport/container/leaf objects rather
  than HTML text fragments or renderer-specific SVG fragment kinds
- `use`, paint servers, and clip paths resolve through the native SVG resource
  graph
- SVG text and `foreignObject` remain native SVG payload kinds even when feature
  coverage is partial
- external SVG image resources remain implementation-defined and may use a
  separate backend from inline SVG

Current HAVI Phase 1 browser-visible SVG DOM state:

- the basic/current SVG DOM interface graph is structurally in place for the
  current subset, including the `SVGGradientElement` and
  `SVGTextContentElement` / `SVGTextPositioningElement` chains
- constructor coverage exists for `g`, `pattern`, `filter`, `marker`, and
  `textPath`
- the first wrapper-backed SVG value-object family now exists with required
  `[SameObject]` caching on the Phase 1 surfaces in scope
- SVG factory and query APIs that would otherwise collide with legacy SVG alias
  names use `DOMPoint`, `DOMRect`, and `DOMMatrix` bridge objects instead
- geometry/text query semantics and long-tail list mutation semantics remain
  partial in this phase; placeholder-backed methods and explicit
  `NotSupported` behavior are acceptable until later SVG phases complete
