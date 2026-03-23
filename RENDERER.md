# HAVI Renderer

This document is the entry point for the current HAVI rendering architecture.
It is written first around design principles, then around the concrete code
path. Read this as internal renderer documentation, not browser spec
behavior. Repository-wide development rules live in `../AGENTS.md`.

## Core design principles

These principles matter more than the individual files.

### 1. No parallel or fallback renderer architectures

HAVI has one active renderer architecture:

- semantic fragments from layout
- direct build into retained browser-scene documents
- Makepad browser-scene execution
- Makepad compositor only as a lower composition stage

The code should not drift back toward parallel renderer architectures,
page-level fallback renderers, compatibility bridges, or a split between an old
HAVI renderer and a new Makepad renderer.

### 2. One scene model for browser content

HAVI does not build an intermediate HAVI-specific render scene and then lower
that again into Makepad. The active builder emits `MpDocument` directly.

Relevant files:

- `havi/crates/render/src/lib.rs`
- `havi/crates/render/src/browser_scene_builder.rs`
- `makepad/browser_scene/src/scene.rs`

### 3. One placement story for webview content

Browser content is attached to the shell through one retained scene/document
boundary. Transforms, clips, scroll offsets, sticky offsets, effects, and child
embeds are expressed as scene facts. The renderer decides which parts can draw
straight into a pass and which parts need compositor surfaces.

The important boundary is not "widget draws fragments". The widget presents an
already built retained document.

Relevant files:

- `havi/ports/havishell/src/servo_web_view.rs`
- `havi/crates/render/src/lib.rs`
- `makepad/browser_scene/src/renderer.rs`

### 4. Semantic facts first, execution choices downstream

The builder records browser facts in the retained scene:

- spatial/reference-frame structure
- scroll and sticky state
- clip chains
- effect groups
- primitives
- embeds

Execution choices happen later in the renderer:

- direct draw vs surface composition
- isolated group allocation
- transformed-group composition
- scratch-surface reuse

The builder should not pre-commit ordinary content to surface boundaries just to
make execution easier.

Relevant files:

- `havi/crates/render/src/browser_scene_builder.rs`
- `makepad/browser_scene/src/scene.rs`
- `makepad/browser_scene/src/renderer.rs`

### 5. Direct paint for ordinary content, compositor only for real boundaries

The Makepad browser-scene renderer draws flat ordinary content directly with
Makepad draw primitives:

- solid rects
- rounded rects
- borders
- gradients
- box shadows
- text runs
- images

It uses surfaces and `makepad/compositor` only where the current execution model
needs an isolated boundary:

- transformed subtrees
- isolated effect groups
- embedded child documents
- masked rounded-clip groups
- host-space reattachment when the document viewport is not rooted at `(0, 0)`

Relevant files:

- `makepad/browser_scene/src/renderer.rs`
- `makepad/compositor/src/`

### 6. Retained resources, not per-draw uploads

Images, fonts, glyph runs, and child documents live in the retained document.
The builder reuses the previous document's resource store when rebuilding.
Scroll-only updates mutate scroll spatial nodes in place without rebuilding the
whole document.

Relevant files:

- `havi/crates/render/src/browser_scene_builder.rs`
- `havi/crates/render/src/lib.rs`
- `makepad/browser_scene/src/resource.rs`
- `makepad/browser_scene/src/transaction.rs`

### 7. Unsupported features fail by omission, not by architecture fork

The builder and renderer log unsupported cases once and skip them. They do not
switch the entire page to another renderer.

This keeps the architecture singular. It also means missing features are visible
as missing content, not hidden by a fallback path.

Relevant files:

- `havi/crates/render/src/browser_scene_builder.rs`
- `makepad/browser_scene/src/renderer.rs`

## Renderer summary

HAVI renders from Servo layout fragments into a retained Makepad-native browser
scene.

The active path is:

```text
Servo layout publication
  -> shared fragment tree (`havi-fragment-semantics`)
  -> HAVI direct builder (`havi/crates/render`)
  -> retained browser document (`makepad/browser_scene::MpDocument`)
  -> Makepad browser-scene renderer
      -> direct Makepad drawing for ordinary content
      -> Makepad compositor surfaces for transform/effect/embed boundaries
  -> widget presentation in havishell
```

There is no separate legacy renderer in the active path.
Unsupported content is skipped with one-time logging instead of falling back to
an older fragment-to-surface renderer.

## Top-level code map

### HAVI shell attachment

- `havi/ports/havishell/src/servo_web_view.rs`
  - webview widget draw entry
  - gathers viewport rect, scroll state, selection overlay inputs
  - calls `havi_render::render_fragments_clipped()`

### HAVI render crate

- `havi/crates/render/src/lib.rs`
  - render entry point
  - `FrameDrawListState`
  - retained document cache
  - scroll-only cache update path
  - renderer stats logging
- `havi/crates/render/src/layout_adapter.rs`
  - reads the shared fragment tree from layout publication
- `havi/crates/render/src/browser_scene_builder.rs`
  - fragment-tree to `MpDocument` builder
  - spatial tree, clip chains, effects, embeds, scroll-node bookkeeping
- `havi/crates/render/src/browser_scene_primitives.rs`
  - lowers backgrounds, borders, outlines, shadows, gradients, text, images
  - creates retained resources
- `havi/crates/render/src/reference_frame.rs`
  - transform/reference-frame semantics for the builder
- `havi/crates/render/src/transform.rs`
  - CSS transform and perspective helpers

Files present in `havi/crates/render/src/` but not part of the active path are
not declared from `lib.rs`. At time of writing that includes:

- `clip_tree.rs`
- `frame_tree.rs`

### Makepad retained browser-scene crate

- `makepad/browser_scene/src/lib.rs`
  - public crate boundary
- `makepad/browser_scene/src/scene.rs`
  - `MpDocument`, `MpScene`, ordering, spatial/clip resolution helpers
- `makepad/browser_scene/src/spatial.rs`
  - spatial node types
- `makepad/browser_scene/src/clip.rs`
  - clips and clip chains
- `makepad/browser_scene/src/effect.rs`
  - effect nodes and filter/blend/mask data model
- `makepad/browser_scene/src/primitive.rs`
  - retained primitive types
- `makepad/browser_scene/src/embed.rs`
  - child document embedding
- `makepad/browser_scene/src/resource.rs`
  - images, fonts, glyph runs, external images
- `makepad/browser_scene/src/renderer.rs`
  - execution planner and renderer
- `makepad/browser_scene/src/hit_test.rs`
  - retained hit-test helper
- `makepad/browser_scene/src/transaction.rs`
  - document transaction API

### Makepad compositor

- `makepad/compositor/src/scene.rs`
- `makepad/compositor/src/eval.rs`
- `makepad/compositor/src/surface.rs`
- `makepad/compositor/src/quad.rs`

The browser-scene renderer uses the compositor as a lower composition stage. It
is not the browser scene model.

## Frame lifecycle

## 1. Havishell gathers widget state

`ServoWebView::draw_walk()` computes:

- the webview widget rect
- viewport top and bottom from shell scroll state
- per-element scroll offsets
- optional selection highlight rectangles

It then calls:

- `havi_render::render_fragments_clipped()`

File:

- `havi/ports/havishell/src/servo_web_view.rs`

## 2. Render crate fetches the current fragment tree

`LayoutFragmentSource` reads the shared fragment tree published by layout:

- `shared_layout_fragment_tree_for(webview_id)`

The active render path consumes `Arc<Vec<Fragment>>` from
`havi-fragment-semantics`.

File:

- `havi/crates/render/src/layout_adapter.rs`

## 3. Render crate reuses or rebuilds the retained document

`FrameDrawListState` keeps:

- `MpBrowserRenderer`
- cached `MpDocument`
- scroll-node mapping for that document
- instrumentation counters

Cache key inputs are:

- fragment tree pointer identity
- viewport size
- scroll-state hash

If the fragment tree pointer and viewport size match, the renderer can reuse the
cached document. If only scroll offsets changed, it updates scroll-frame spatial
nodes in place with `update_scroll_offsets()`.

Relevant code:

- `BrowserDocumentCacheEntry`
- `update_cached_browser_document_scroll_offsets()`
- `hash_scroll_state()`

File:

- `havi/crates/render/src/lib.rs`

## 4. HAVI builds `MpDocument` directly when needed

`try_build_browser_document()` constructs:

- `MpScene`
- retained resources
- child documents for iframes
- mapping from DOM node ids to scroll spatial ids

The builder walks the fragment tree recursively. It does not build a separate
HAVI render scene first.

File:

- `havi/crates/render/src/browser_scene_builder.rs`

## 5. Makepad browser-scene renderer executes the document

`MpBrowserRenderer::draw_document()` takes:

- a retained `MpDocument`
- its `MpResourceStore`
- the host viewport rect in widget space

It first analyzes the scene into a render plan, then executes either:

- direct flat drawing into the current pass, or
- compositor-backed surface composition, or
- a mix of both inside the same retained plan

File:

- `makepad/browser_scene/src/renderer.rs`

## 6. Shell overlays draw after document presentation

Selection highlight rectangles are drawn after the browser document. The shell's
scrollbar thumb is drawn separately by `ServoWebView`.

Files:

- `havi/crates/render/src/lib.rs`
- `havi/ports/havishell/src/servo_web_view.rs`

## Retained scene model

`MpDocument` is the retained browser rendering unit.

```text
MpDocument
  - id
  - epoch
  - scene: MpScene
  - resources: MpResourceStore
  - child_documents: Vec<MpChildDocument>
```

`MpScene` contains the retained browser scene itself.

```text
MpScene
  - spatial_nodes
  - clips
  - clip_chains
  - effects
  - primitives
  - embeds
  - items   // stable paint order across primitives and embeds
  - hit_test_items
```

File:

- `makepad/browser_scene/src/scene.rs`

### Spatial nodes

Current spatial node kinds:

- `ReferenceFrame`
- `ScrollFrame`
- `StickyFrame`
- `EmbedRoot`

A spatial node defines local placement semantics. The builder emits these from
fragment semantics rather than flattening everything into root coordinates.

File:

- `makepad/browser_scene/src/spatial.rs`

### Clips and clip chains

Clip nodes are stored separately from spatial nodes and referenced through clip
chains.

Current clip kinds in the scene model:

- `Rect`
- `RoundedRect`
- `ImageMask`
- `PlaneSet`

The current renderer executes rect clips directly and executes rounded-clip
chains through a masked-surface helper. `ImageMask` and `PlaneSet` exist in the
scene model but are not part of the currently executed subset.

Files:

- `makepad/browser_scene/src/clip.rs`
- `makepad/browser_scene/src/scene.rs`
- `makepad/browser_scene/src/renderer.rs`

### Effects

`MpEffectNode` carries semantic effect-group data:

- `opacity`
- `filters`
- `blend_mode`
- `isolation`
- `mask`

The scene model is broader than the currently executed subset.

Current HAVI builder emits effect nodes for:

- opacity
- CSS blur filter
- CSS opacity filter
- non-normal mix-blend metadata

Current renderer execution applies:

- opacity
- blur

The broader fields are kept in the scene model, but not all are executed yet.
In particular, mask execution and full blend-mode execution are not wired as a
complete path yet.

Files:

- `havi/crates/render/src/browser_scene_builder.rs`
- `makepad/browser_scene/src/effect.rs`
- `makepad/browser_scene/src/renderer.rs`

### Primitives

The retained primitive model includes:

- solid rect
- rounded rect
- border
- text run
- image
- repeating image
- linear gradient
- radial gradient
- conic gradient
- box shadow
- line decoration

HAVI currently emits a subset centered on ordinary page content:

- background color
- rounded backgrounds
- borders and outlines
- gradients
- box shadows
- text runs with retained glyph resources
- images
- iframe embeds

Files:

- `makepad/browser_scene/src/primitive.rs`
- `havi/crates/render/src/browser_scene_primitives.rs`

### Resources

Retained resources are document-owned:

- images
- fonts
- glyph runs
- external images

HAVI's builder populates the resource store while lowering primitives.
On rebuild it starts from the previous document's resource store, so identical
resources are retained across rebuilds.

Files:

- `makepad/browser_scene/src/resource.rs`
- `havi/crates/render/src/browser_scene_builder.rs`
- `havi/crates/render/src/browser_scene_primitives.rs`

### Child documents and embeds

Iframes are lowered as child documents, not flattened paint output.

The builder:

- allocates a `MpPipelineId`
- recursively builds a child `MpDocument`
- emits `MpEmbed` in the parent scene
- stores the child document in `document.child_documents`

The renderer draws child documents into textures as needed and reattaches them
at the embed bounds.

Files:

- `havi/crates/render/src/browser_scene_builder.rs`
- `makepad/browser_scene/src/embed.rs`
- `makepad/browser_scene/src/renderer.rs`

## HAVI builder details

## Fragment traversal

`build_fragment_list()` and `build_fragment()` recursively traverse the fragment
structure from `havi-fragment-semantics`.

Current fragment cases on the active path:

- `Box`
- `Float`
- `Text`
- `Image`
- `Positioning`
- `AbsoluteOrFixedPositioned`
- `IFrame`

File:

- `havi/crates/render/src/browser_scene_builder.rs`

## Reference frames and transforms

`reference_frame_semantics()` decides whether a box fragment becomes a browser
reference frame. It computes:

- placement origin
- local transform matrix
- descendant perspective matrix
- used transform style
- flattening flag
- backface visibility

The builder converts that into an `MpSpatialNode::ReferenceFrame`.

Files:

- `havi/crates/render/src/reference_frame.rs`
- `havi/crates/render/src/transform.rs`
- `havi/crates/render/src/browser_scene_builder.rs`

## Scroll and sticky

The builder emits:

- `MpScrollFrame` for overflow-scrolling content
- `MpStickyFrame` for sticky positioning

It also records a map from DOM node id to scroll spatial id so cached documents
can update scroll offsets without full rebuild.

Files:

- `havi/crates/render/src/browser_scene_builder.rs`
- `makepad/browser_scene/src/spatial.rs`

## Clips

The builder creates clip chains for:

- absolute-position CSS `clip`
- overflow clips
- rounded overflow clips
- background-layer clips when needed

Clip nodes stay attached to the retained scene instead of being pre-rasterized
into standalone surfaces by the builder.

Files:

- `havi/crates/render/src/browser_scene_builder.rs`
- `havi/crates/render/src/browser_scene_primitives.rs`

## Primitive lowering

`paint_run_item_to_primitives()` lowers fragment paint payload into retained
primitives and retained resources.

Important behaviors:

- background images and gradients become retained image/gradient primitives
- tiled background image repetition is expanded to multiple image primitives
- text becomes a retained glyph-run resource plus a `TextRun` primitive
- images become retained image resources plus `Image` primitives
- owner node ids become `MpHitTestTag` values where appropriate

File:

- `havi/crates/render/src/browser_scene_primitives.rs`

## Unsupported builder cases

Builder skips currently unsupported cases and logs once. Known examples in the
code include:

- CSS `clip-path` masks
- filters outside the currently lowered subset
- unsupported paint-run shapes
- unshaped text
- missing font handles

File:

- `havi/crates/render/src/browser_scene_builder.rs`
- `havi/crates/render/src/browser_scene_primitives.rs`

## Makepad browser-scene renderer details

## Planner

`build_plan(scene)` partitions `scene.items` into execution groups:

- direct items
- direct chunks that must be reattached through compositor surfaces
- isolated effect groups
- transformed groups

The plan computes `MpRendererStats` such as:

- direct primitive count
- isolated boundary count
- compositor surface count
- total offscreen pixel area
- scratch-surface allocation and reuse counts

File:

- `makepad/browser_scene/src/renderer.rs`

## Direct flat drawing

When the plan has no compositor surfaces and the viewport is rooted at `(0, 0)`,
`draw_scene_items()` draws directly into the current pass.

Primitive execution in `draw_primitive_flat()` uses Makepad draw shaders:

- `DrawColor`
- `DrawRoundedColor`
- `DrawBorderColor`
- `DrawGradient`
- `DrawBoxShadow`
- `DrawText`
- `DrawImage`

Rounded clip chains are handled separately through `draw_masked_group()` and
`draw_masked_surface()`.

File:

- `makepad/browser_scene/src/renderer.rs`

## Compositor-backed execution

When the plan needs host-space attachment or isolated boundaries, the renderer
creates a `makepad_compositor::MpScene` with:

- a root reference frame for the webview host rect
- surface nodes for direct chunks
- effect nodes plus surface nodes for isolated groups
- reference-frame nodes plus surface nodes for transformed groups

This is the point where the browser-scene renderer hands work to
`makepad/compositor`.

File:

- `makepad/browser_scene/src/renderer.rs`

## Scratch surfaces

Offscreen surfaces are reused through a scratch-surface pool.

Current scratch-surface behavior:

- surfaces are allocated lazily
- each frame resets a scratch cursor
- reused slots are resized as needed
- total offscreen pixel area is tracked for stats

File:

- `makepad/browser_scene/src/renderer.rs`

## Filters

Current effect-filter execution is texture-based:

- blur is applied through `DrawFilteredTexture`
- opacity filters are folded into effect opacity
- named/other filters are logged unsupported

File:

- `makepad/browser_scene/src/renderer.rs`

## Clips in execution

The renderer resolves clip state from the retained scene.

Current execution behavior:

- rect clips become ordinary draw clips
- rounded clip chains become masked texture composition
- up to four rounded clips are supported by the current masked-texture shader
- unsupported clip kinds are skipped with one-time logging

Files:

- `makepad/browser_scene/src/scene.rs`
- `makepad/browser_scene/src/renderer.rs`

## Embeds in execution

Embeds are rendered by drawing the child `MpDocument` into a texture and then
composing that texture back into the parent scene at the embed bounds.

File:

- `makepad/browser_scene/src/renderer.rs`

## What Makepad provides

The current renderer uses two distinct Makepad layers.

### 1. Makepad draw system

Used for ordinary flat primitive drawing:

- solid color
- rounded boxes
- borders
- gradients
- text
- images
- box shadows
- masked texture composition for rounded clips
- filtered texture passes

Relevant areas:

- `makepad/draw/`
- `makepad/platform/`

### 2. Makepad compositor

Used for explicit retained composition boundaries:

- transformed groups
- isolated effect groups
- host-space surface attachment

Relevant areas:

- `makepad/compositor/src/scene.rs`
- `makepad/compositor/src/eval.rs`
- `makepad/compositor/src/quad.rs`
- `makepad/compositor/src/surface.rs`

The important architectural split is:

- `makepad/browser_scene` is the browser scene model and browser renderer
- `makepad/compositor` is the lower compositing engine used by that renderer

## Caching and invalidation

Current retained behavior is simple and explicit.

### Cached at the HAVI boundary

`FrameDrawListState` caches the last built `MpDocument` plus scroll-node ids.

Reuse works when:

- the fragment tree pointer is unchanged
- viewport size is unchanged

### Scroll-only update path

If only scroll offsets changed, HAVI mutates the cached document's
`MpScrollFrame.scroll_offset` values in place before drawing.

### Resource reuse

New documents start from the previous document's `MpResourceStore`, so the
builder can keep retained resources stable across rebuilds.

### Transactions

`makepad/browser_scene` exposes a transaction API, but the current HAVI path is
not driven by a separate transaction queue. HAVI currently rebuilds whole
`MpDocument` objects when structure changes and uses targeted in-place scroll
updates for the common scroll-only case.

Files:

- `havi/crates/render/src/lib.rs`
- `havi/crates/render/src/browser_scene_builder.rs`
- `makepad/browser_scene/src/transaction.rs`

## Hit testing

The retained scene model includes hit-test tags and a simple retained hit-test
helper.

Current builder behavior:

- primitives and embeds receive `MpHitTestTag` values derived from DOM owner ids

Current helper behavior:

- iterates scene items back-to-front
- resolves retained bounds
- performs simple point-in-rect tests

File:

- `makepad/browser_scene/src/hit_test.rs`

This is part of the retained scene model. It is not the whole shell input story.
For rendering work, treat it as scene-local retained hit-test support.

## Instrumentation and logging

### Render stats

Set:

```bash
HAVI_RENDER_STATS=1
```

This logs per-frame browser-scene stats from `log_browser_scene_stats()`.

File:

- `havi/crates/render/src/lib.rs`

### Unsupported content logging

Both the builder and the renderer log unsupported cases once per reason.

Files:

- `havi/crates/render/src/browser_scene_builder.rs`
- `makepad/browser_scene/src/renderer.rs`

## Current executed subset and current gaps

The public retained scene model is broader than the current executed subset.
The active HAVI path is strongest for ordinary page content.

Current strengths in the active code:

- retained scene-level rendering
- retained resources
- direct ordinary-content drawing
- transform/reference-frame lowering
- scroll and sticky spatial nodes
- overflow and rounded clips
- gradients and background images
- text with retained glyph resources
- iframe child documents
- scratch-surface reuse and renderer stats

Known gaps visible in the current code:

- CSS `clip-path` mask lowering is not implemented
- filter support is partial
- effect masks are not a complete execution path
- full blend-mode execution is not a complete execution path
- scene-model clip kinds such as `ImageMask` and `PlaneSet` are not in the
  currently executed subset
- unsupported builder or renderer cases are skipped, not recovered through a
  second renderer

For exact current behavior, read the match arms in:

- `havi/crates/render/src/browser_scene_builder.rs`
- `havi/crates/render/src/browser_scene_primitives.rs`
- `makepad/browser_scene/src/renderer.rs`
- `makepad/browser_scene/src/scene.rs`

## Recommended reading order

If you are new to this code, read in this order:

1. `havi/RENDERER.md`
2. `havi/ports/havishell/src/servo_web_view.rs`
3. `havi/crates/render/src/lib.rs`
4. `havi/crates/render/src/layout_adapter.rs`
5. `havi/crates/render/src/browser_scene_builder.rs`
6. `havi/crates/render/src/browser_scene_primitives.rs`
7. `havi/crates/render/src/reference_frame.rs`
8. `makepad/browser_scene/src/lib.rs`
9. `makepad/browser_scene/src/scene.rs`
10. `makepad/browser_scene/src/spatial.rs`
11. `makepad/browser_scene/src/effect.rs`
12. `makepad/browser_scene/src/primitive.rs`
13. `makepad/browser_scene/src/resource.rs`
14. `makepad/browser_scene/src/renderer.rs`
15. `makepad/compositor/src/`

## External reference code

These trees are useful reference material for renderer structure. They are not
HAVI code and they are not a source of direct API compatibility, but they are
useful when reasoning about scene graphs, property trees, and composition.

### Gecko / WebRender

- `/mnt/llm/gecko-dev/gfx/wr/webrender/src/render_api.rs`
- `/mnt/llm/gecko-dev/gfx/wr/webrender/src/scene_building.rs`
- `/mnt/llm/gecko-dev/gfx/wr/webrender/src/frame_builder.rs`
- `/mnt/llm/gecko-dev/gfx/wr/webrender/src/composite.rs`
- `/mnt/llm/gecko-dev/gfx/wr/webrender/src/picture_textures.rs`

### Chromium

- `/mnt/llm/chromium/cc/trees/property_tree_builder.cc`
- `/mnt/llm/chromium/cc/paint/display_item_list.h`
- related code under `/mnt/llm/chromium/cc/trees/`

### Servo mainline reference shape

- `experiment/servo-mainline/components/layout/display_list/mod.rs`
- `experiment/servo-mainline/components/layout/display_list/stacking_context.rs`
- `experiment/servo-mainline/components/paint/painter.rs`

These references are most useful for:

- property-tree separation
- transform/clip/effect ownership
- downstream surface decisions instead of early semantic surface splitting
- embed and hit-test structure

## Bottom line

The active HAVI renderer is a retained browser-scene pipeline.

HAVI builds `MpDocument` directly from semantic fragments.
Makepad executes that document with a split between:

- direct draw for ordinary content
- compositor surfaces for actual composition boundaries

If you are changing renderer architecture, preserve that split and keep the
scene model singular.