# HAVI Renderer

This document describes the active HAVI browser renderer after the Makepad
browser-scene rewrite.

Repository-wide development rules live in `../AGENTS.md`.

## Final layering

HAVI renders through one retained pipeline:

```text
Servo layout fragments
  -> HAVI direct builder (`havi/crates/render`)
  -> retained browser document (`makepad/browser_scene::MpDocument`)
  -> retained compositor browser scene lowering (`makepad/browser_scene::MpBrowserRenderer`)
  -> Makepad compositor execution (`makepad/compositor`)
  -> havishell widget presentation
```

There is no parallel legacy browser renderer.
There is no browser-scene-owned scratch surface system.
There is no texture-first transformed-content fallback for ordinary page content.

## Ownership split

### `havi/crates/render`

Owns lowering from Servo layout fragments into retained browser-scene data:

- spatial/reference-frame structure
- scroll and sticky nodes
- clip chains
- effect groups
- primitives
- embeds
- document-local glyph runs
- renderer-scoped font/image resource population

Important files:

- `havi/crates/render/src/lib.rs`
- `havi/crates/render/src/browser_scene_builder.rs`
- `havi/crates/render/src/browser_scene_primitives.rs`
- `havi/crates/render/src/reference_frame.rs`

### `makepad/browser_scene`

Owns retained browser document data and the lowering boundary into compositor
execution.

It does not own persistent font/image payload storage, runtime picture
allocation, filter passes, scratch surfaces, or per-glyph browser-local
drawing.

Important files:

- `makepad/browser_scene/src/scene.rs`
- `makepad/browser_scene/src/spatial.rs`
- `makepad/browser_scene/src/clip.rs`
- `makepad/browser_scene/src/effect.rs`
- `makepad/browser_scene/src/primitive.rs`
- `makepad/browser_scene/src/resource.rs`
- `makepad/browser_scene/src/renderer.rs`

### `makepad/compositor`

Owns retained execution data and execution policy for browser content:

- transform palette
- clip-chain execution
- retained primitive submission
- retained text submission
- picture nodes
- render-task graph
- picture/task cache entries
- picture output composition

Important files:

- `makepad/compositor/src/browser_primitives.rs`
- `makepad/compositor/src/browser_scene.rs`
- `makepad/compositor/src/quad.rs`
- `makepad/compositor/src/eval.rs`
- `makepad/compositor/src/scene.rs`

## Core rules

### 1. Ordinary browser content stays as primitives as long as possible

Solid rects, rounded rects, borders, gradients, images, repeating images, box
shadows, and ordinary text stay as retained compositor-owned content.

Transforms and clip chains are carried into compositor execution instead of
forcing browser-local rasterization.

### 2. Offscreen surfaces are semantic consequences

Offscreen work only exists for:

- picture boundaries
- render-task outputs
- cached picture/task outputs
- embeds/external surfaces

There is no generic `DirectChunk` ownership model and no transformed-group
scratch ownership in `browser_scene`.

Important distinction:

- arbitrary CSS subtree opacity requires isolated-picture semantics
- that does not mean every opacity case must allocate an offscreen surface
- trivial cases may be optimized later by folding opacity into direct draws
  when the result is provably equivalent
- the correct first implementation is still to isolate group opacity through the
  picture/task path, then optimize reducible cases later

Gecko / WebRender follows the same architectural direction: opacity is modeled
as picture/filter composition and may use simple surfaces for opacity, while
still allowing optimization policy above that layer.

### 3. One clip model and one picture/task model

Browser-scene lowers semantic clip chains.
The compositor executes them.

Browser-scene lowers semantic effect groups and embeds.
The compositor owns pictures and render tasks.

### 4. Text is retained compositor-owned execution

Browser text is no longer drawn through browser-scene per-glyph traversal.
Browser-scene lowers retained text-run data and font resources into compositor
text execution.

The compositor now owns a browser-only glyph residency cache with explicit
atlas pages and page textures. Browser scenes do not own glyph residency.
They get per-draw prepared batches built against the current glyph-cache
generation.

### 5. Explicit host geometry

`render_fragments_clipped()` takes `host_rect: Rect` explicitly from the HAVI
widget layer. The retained renderer does not read ambient turtle or draw-list
geometry. The widget resolves the real webview rect with
`self.draw_bg.area().rect(cx)` and passes it down.

### 6. Origin-space retained clips with draw-time evaluation

Retained clip data (in `MpPrimitiveClipChain`) is stored in origin space only.
The compositor evaluates clips at draw time using the explicit full basis:
`clip_from_origin = clip_from_world * world_from_scene * scene_from_origin`.

This ensures geometry and clipping always use the same transform chain,
regardless of where the webview is placed in the Makepad draw-list hierarchy.

### 7. Caching lives at picture/task level

Large retained content is cacheable only through explicit compositor picture/task
entries. Repeating backgrounds use a native repeating-image primitive instead of
CPU tile expansion loops.

## Runtime model

## Scroll ownership

Browser scroll now has one structural definition, one sampled runtime state,
and one commit path.

- `paint_api::scroll_tree::ScrollTree` is the structural scroll graph.
  It carries immutable scroll-node structure only: ids, parent chains,
  content rects, clip rects, sensitivities, and sticky metadata.
- `BrowserScrollController` in
  `havi/ports/havishell/src/browser_scroll.rs` owns sampled scroll offsets.
  It performs hit testing, ancestor handoff, and clamping for default browser
  scrolling.
- `SharedScrollState` is not render truth. HAVI uses it only as a minimal shell
  snapshot for the scroll indicator and as a one-time bootstrap fallback for the
  root committed scroll offset.
- The renderer consumes sampled offsets through
  `BrowserScrollController::render_scroll_state()` and applies them through the
  retained `BrowserDocumentScrollNodes` mapping.
- Committed offsets flow back to layout/script through `SetScrollStates`.

## Semantic document

`MpDocument` stays the retained browser unit:

```text
MpDocument
  - id
  - epoch
  - scene: MpScene
  - glyph_runs
  - child_documents
```

Fonts, images, and external image handles do not live in `MpDocument`.
They live in the persistent renderer-scoped `ResourceRegistry` owned by
`MpBrowserRenderer`.

`MpScene` stores semantic browser facts:

- `spatial_nodes`
- `clips`
- `clip_chains`
- `effects`
- `primitives`
- `embeds`
- `items`
- `hit_test_items`

## Compositor browser scene

`makepad/browser_scene/src/renderer.rs` lowers the semantic scene into
`makepad/compositor::MpBrowserScene`.

The compositor browser scene contains:

- retained primitive batches
- retained text runs
- picture nodes
- render tasks
- ordered interleaving of those items

Important retained execution types:

- `MpBrowserScene`
- `MpBrowserSceneItem`
- `MpBrowserPicture`
- `MpBrowserTask`
- `MpBrowserTaskKind`
- `MpBrowserPrimitiveScene`
- `MpBrowserTextRun`

Each retained lowered scene now carries one retained-scene id plus stable text-run
ids allocated during lowering. Those ids are the future prepared-text cache keys.
They are allocated once per retained lowered-scene tree and cover root direct
text, picture-fallback text, nested task scenes, and iframe child documents.

## Render-task graph

Current task kinds:

- `Scene` task: render one retained browser subscene into a task surface
- `Blur` task: filter another task output

This graph is explicit retained execution data, not hidden browser-scene logic.
`makepad/compositor/src/browser_scene.rs` is the runtime owner for this task
execution.

## Picture nodes

Pictures are the only browser composition boundaries. They are used for:

- isolated opacity/filter groups
- embeds
- nested effect subtrees
- cached content slices or tiles

A picture references a task output and describes how that result is composed back
into ordered browser content.

## Retained text path

Browser-scene lowers text runs into compositor text resources:

- font resources
- glyph instances
- metrics
- decoration data
- shadow data

The compositor executes those runs through an explicit browser text prepare
phase before draw.

Prepare owns:

- canonical glyph-key construction from raster-affecting state only
- glyph residency lookup in one global browser glyph cache
- page allocation in separate alpha, msdf, and color page pools
- page upload to explicit GPU textures
- retained prepared batches keyed by retained-scene id plus stable text-run id

Draw owns:

- decoration quads
- validated prepared glyph batch submission only

Browser draw does not call `font.rasterize_glyph()`.
Browser draw does not allocate atlas slots.
Browser draw does not mutate atlas textures.

Prepared batches now live on the retained lowered-scene owner, not in transient
per-draw traversal. Validation is cheap: compare the retained prepared-run key,
retained glyph-entry generations, and referenced atlas page generations. Only
invalid runs rebuild. Unchanged retained text runs skip glyph walking entirely.

Outline MSDF promotion is browser-owned and asynchronous. The first MSDF miss
publishes an alpha fallback entry immediately, queues browser-owned worker
promotion, then promotes the final MSDF entry before page upload on a later
frame. Promotion invalidates only prepared runs that reference the promoted
`BrowserGlyphKey`; ordinary atlas page allocation and promotion do not trigger a
global browser glyph-cache reset.

Current direct text execution uses the retained text-run path when the run can
stay on the local clip-rect fast path. Higher-level picture boundaries still
route that content through picture/task composition as needed.

## Retained cache model

The compositor owns cache entries by explicit task cache keys.

Current intended uses:

- retained picture slices for large repeated content
- reused task outputs across frames
- cached embed/effect outputs when stable

The current cache implementation is task-level and key-driven. It is explicit,
inspectable through compositor browser-scene frame stats, and does not reintroduce
generic browser-scene texture ownership.

## HAVI frame lifecycle

## 1. Havishell gathers widget state

`ServoWebView::draw_walk()` computes the widget rect, syncs the structural
scroll graph into `BrowserScrollController`, samples browser scroll state, and
builds selection overlays, then calls:

- `havi_render::render_fragments_clipped()`

File:

- `havi/ports/havishell/src/servo_web_view.rs`

## 2. Render crate fetches the fragment tree

`LayoutFragmentSource` reads the shared layout fragment payload from layout.
That payload is published through `SharedLayoutFragmentTree` as an immutable
`Arc<FragmentArenaGeneration>` built in `havi-types`.
The published arena carries fragment ids, explicit paint roots, out-of-flow
placement records, and derived side tables.
Unchanged reflows keep the same arena pointer.

File:

- `havi/crates/render/src/layout_adapter.rs`

## 3. Render crate reuses or rebuilds `MpDocument`

`FrameDrawListState` is now the retained browser-output owner. It caches one
entry that contains:

- the retained `MpDocument`
- the retained document scroll-node map
- the retained lowered `makepad_compositor::MpBrowserScene`
- retained-scene structural metadata and instrumentation inputs

Fast path:

- same published fragment source identity (`webview_id` + published generation)
- same viewport size
- same renderer resource-generation identity
- retained lowered-scene reuse without relowering

Scroll offsets update in place through retained scroll nodes on the retained
`MpDocument` layer and through source-driven retained patch metadata on the
retained lowered-scene layer. The retained lowered-scene entry stays keyed from
structural inputs plus resource identity only. Scroll is not a structural key.
Scroll-only frames patch retained transforms, clip chains, primitive rects,
text-run rects, picture rects, and nested task/iframe retained scenes without
structural relowering.

Layout republishes the arena when the fragment-tree generation changes or when
animated background/image content changes the published derived data.
Steady-state frames therefore hit the retained document cache instead of
rebuilding scene data every draw.

`ServoWebView` owns one browser-output surface cache on top of that retained
document. When the browser output stays stable across consecutive frames and
HAVI can present the cached result by exact physical-pixel copy, it promotes the
rendered page content into a dedicated offscreen pass texture and reuses that
texture on later unchanged frames. The key includes fragment source identity
(`webview_id` + published generation), final physical presentation rect, pass
DPI, renderer visual generation, scroll hash, and selection hash. Unsupported
outer clip or transform cases fall back to the live retained draw path instead
of reusing the surface. Dynamic image overrides disable this cache. Screenshot
capture also reads back this browser-owned surface instead of shell chrome
composition, but that capture path is separate from stable surface-cache reuse.
Set `HAVI_BROWSER_SURFACE_CACHE=0` to disable stable-surface reuse.

Files:

- `havi/crates/render/src/lib.rs`
- `havi/crates/render/src/browser_scene_builder.rs`

## 4. HAVI builds semantic browser-scene data

`try_build_browser_document()` lowers layout fragments directly into retained
browser-scene semantic data.

This step stays semantic. It does not allocate runtime surfaces.

Files:

- `havi/crates/render/src/browser_scene_builder.rs`
- `havi/crates/render/src/browser_scene_primitives.rs`

## 5. Browser-scene lowers into compositor execution

`MpBrowserRenderer::draw_document()` lowers semantic browser-scene content into
retained compositor execution:

- primitive batches
- text runs
- pictures
- render tasks

Then it calls:

- `makepad_compositor::MpRenderer::draw_browser_scene()`

File:

- `makepad/browser_scene/src/renderer.rs`

## 6. Compositor executes retained browser content

The compositor executes retained browser primitives directly, runs picture tasks,
reuses cached task outputs, and composites picture outputs in order.

Files:

- `makepad/compositor/src/browser_primitives.rs`
- `makepad/compositor/src/browser_scene.rs`
- `makepad/compositor/src/quad.rs`

## Executed browser feature set

Current retained primitive execution covers:

- solid rect
- rounded rect
- border
- image
- repeating image
- linear gradient
- radial gradient
- conic gradient
- box shadow
- retained text runs and decorations

Current retained clip execution covers:

- rect clips
- rounded-rect clips
- image-mask clips
- plane-set clips
- explicit scene-boundary viewport clips for document scenes and nested task scenes

Retained document scenes now keep viewport clipping as structural root
clip-chain data. Nested task scenes intersect explicit boundary clips into
emitted clip chains before task-local rebase. Placement remains explicit
through `host_rect`, and text, primitives, and pictures all evaluate clip state
against the same effective draw-time basis as their geometry.

Current retained picture/task execution covers:

- isolated opacity groups
- blur filters
- nested pictures
- embeds
- retained task caching

Current task kinds are `Scene` and `Blur`.

## HAVI builder behaviors that matter

### Repeating backgrounds

Repeated image backgrounds lower to `MpRepeatingImage` instead of expanding CPU
tile loops into many separate image primitives.

File:

- `havi/crates/render/src/browser_scene_primitives.rs`

### Text

Text lowers into retained glyph-run resources and text primitives.
Those become compositor text runs.

Files:

- `havi/crates/render/src/browser_scene_primitives.rs`
- `makepad/browser_scene/src/resource.rs`
- `makepad/compositor/src/browser_scene.rs`

### Effects

Opacity and supported filters lower into semantic effect nodes.
Browser-scene does not execute them locally.
They become compositor pictures and tasks.

For correctness, group opacity is treated as an isolated composition boundary.
That matches browser renderer architecture: the subtree must behave like one
composited image. This does not commit HAVI to always allocating a dedicated
offscreen surface for every opacity case forever. It states the semantic rule
first. Later work may optimize away a surface only for cases proven equivalent
to direct per-primitive opacity application.

Files:

- `havi/crates/render/src/browser_scene_builder.rs`
- `makepad/browser_scene/src/effect.rs`
- `makepad/browser_scene/src/renderer.rs`

### Embeds

Iframes lower as child documents plus `MpEmbed` attachment points.
The compositor picture/task model owns their runtime execution.

Files:

- `havi/crates/render/src/browser_scene_builder.rs`
- `makepad/browser_scene/src/embed.rs`
- `makepad/browser_scene/src/renderer.rs`

## Instrumentation

Set:

```bash
HAVI_RENDER_STATS=1
```

This logs browser-scene stats from the HAVI render crate.
It also logs shared-fragment publication reuse vs publish on the layout side,
browser-document cache hit vs miss, retained lowered-scene hit vs miss,
prepared-text rebuild counters, glyph residency hit vs miss, atlas page
allocation, and synchronous browser glyph generation on the render path.
`FrameDrawListState` is the single retained-scene cache owner at this seam.

Those stats now describe the retained compositor picture/task model rather than
legacy scratch ownership.

The current `MpRendererStats` field names remain continuity-oriented and still
include names such as:

- `direct_primitive_count`
- `isolated_boundary_count`
- `compositor_surface_count`
- scratch-surface counters

Interpret them as renderer telemetry for the retained compositor path, not as a
description of the old texture-first ownership model.

## Test coverage

Renderer integration coverage lives in:

- `havi/tests/havi/reftest/reftest.list`
- `havi/tests/havi/reftest/cases/filter-opacity-group.html`
- `havi/tests/havi/reftest/cases/repeating-background-image.html`
- `havi/tests/havi/reftest/cases/text-dense-retained.html`
- `havi/tests/havi/reftest/cases/long-scroll-snapshot.html`
- `havi/tests/havi/reftest/cases/embed-inline.html`
- `havi/tests/havi/benchmark/dense-text.html`
- `havi/tests/havi/benchmark/long-scroll.html`

Makepad proof environments live in:

- `makepad/experiments/browser-primitive-lab/`
- `makepad/experiments/browser-clip-lab/`
- `makepad/experiments/browser-task-lab/`
- `makepad/experiments/browser-text-lab/`
- `makepad/experiments/browser-cache-lab/`

## Document canvas background

CSS root/body background propagation is resolved at layout/publication time,
not in the renderer.

Layout publishes `DocumentCanvasBackground` as part of `FragmentArenaGeneration`.
This record carries the resolved source (root element or propagated body
element), the viewport paint rect, and the source fragment id.

The renderer paints the document canvas background as a dedicated pass before
ordinary fragment traversal. Ordinary box background painting for the
transferred source fragment is suppressed via per-fragment
`suppress_background_paint` metadata.

Suppression is background-only. Borders, box shadows, foreground content, and
hit testing on the source fragment are not affected.

When neither root nor body supplies a canvas background, no
`DocumentCanvasBackground` is published and normal fragment painting proceeds.

Files:

- `havi/crates/types/src/fragment_tree/arena.rs`
- `havi/components/layout/fragment_tree/fragment_tree.rs`
- `havi/crates/render/src/browser_scene_primitives/mod.rs`
- `havi/crates/render/src/browser_scene_primitives/box_background.rs`
- `havi/crates/render/src/browser_scene_builder/document.rs`

## Bottom line

HAVI now has one retained browser renderer architecture:

- HAVI owns semantic document construction
- browser-scene owns semantic retained data plus lowering
- the compositor owns retained runtime execution for primitives, text,
  pictures, tasks, and cache

If you are changing this renderer, keep ownership on those boundaries and keep
surfaces as picture/task consequences only.
