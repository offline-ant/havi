# Runtime

This spec defines browser-visible runtime concepts for HPPR browsing.

Implementation-specific shell behavior, local database layout, watch controls,
shadow workflows, diagnostics pages, and service management belong in
`../reference.md`.

## Browser-local state role

An HPPR browser keeps browser-local packet and policy state for persistent local
behavior.

Typical browser-local uses include:

- cached packets
- local route configuration
- local trust configuration
- browser-owned policy state
- optionally locally authored content

The storage backend and local process model are implementation-defined.

## Clients exposed to page code

Current committed-source behavior:

- `window.source` is the ambient committed-source descriptor for ordinary
  repo-backed documents
- `window.source.client` targets the committed source actually used to load the
  document
- `window.source.kind` is `"repo"` or `"remote"`
- `window.source.authority` is the committed content authority when one exists
- `window.source` is `null` on `file://` pages, helper pages, and non-HPPR pages

Legacy ordinary-page ambient repo clients are no longer part of the public
runtime model:

- ordinary pages do not expose `window.home`
- ordinary pages do not expose `window.route`

Helper-only privileged access remains separate and explicit through the
internal helper capability root at `window.havi` on internal helper pages.

## Origin-scoped capability isolation

Browsers isolate browser-granted repo capability by origin.

HAVI ordinary pages do not receive hidden per-origin Ring1 credentials.
Ambient repo power comes from the committed `window.source` descriptor, and
extra repo power comes from explicit origin-scoped named-client grants. Other
implementations may use a different storage mechanism while preserving the same
origin isolation.

## Local persistence

Route fetches may be cached in browser-local packet storage.

Browsers may persist additional local state for history, grants, authoring,
and shell integration. The format and storage location are implementation-
defined.

## SVG runtime model

Inline `<svg>` and standalone `image/svg+xml` documents participate in the
browser's native style, layout, fragment, and paint pipeline.

Required runtime behavior:

- inline SVG is not serialized to a temporary image URL for layout or first
  paint
- standalone SVG navigation uses the same native SVG DOM/layout/resource
  pipeline as inline SVG
- the fragment arena publishes native SVG viewport/container/leaf objects rather
  than HTML text fragments or renderer-specific SVG fragment kinds
- `use`, paint servers, and clip paths resolve through the native SVG resource
  graph
- SVG text and `foreignObject` remain native SVG payload kinds even when feature
  coverage is partial
- external SVG image resources use the same native SVG parse/layout/fragment/
  paint pipeline as inline and standalone SVG for the supported SVG subset
- external SVG image resources do not need to instantiate a page SVG DOM when
  used as image resources

Current HAVI entry-path SVG runtime state:

- inline, standalone, and external SVG images share one native SVG
  layout/resource/paint pipeline in the current tree
- external SVG images do not instantiate a page SVG DOM and they do not expose
  the SVG query surface

Remaining additive SVG backlog is tracked in `../svg-missing.md`.

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
