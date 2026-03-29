# HAVI Reference

HAVI-specific tools, runtime details, and operator surfaces live here.

The `havi/spec/` directory defines the behavior needed to implement a generic
HPPR-native browser. This document covers HAVI's extra tooling and current
browser shell behavior.

## Internal schemes

HAVI provides browser-owned helper schemes in addition to `hppr://` and
`file://`.

### `hppr-setup://`

Route setup flow for first-contact endpoint approval and local route storage.

### `hppr-join://`

HAVI join and login flow for Ring2-backed groups.

### `hppr-sandbox://`

Untrusted preview mode.

### `hppr-browse://`

Read-only directory explorer for coordinate trees.

### `hppr-editor://`

Local packet editor.

### `havi://`

Internal HAVI administration pages.

Current pages include:

- `havi:///overview`
- `havi:///diagnostics`
- `havi:///services`

These pages are implementation-defined UI. They are not part of the generic
HPPR browser spec.

## Local runtime and configuration

Environment variables:

- `HAVI_HOME` — remote hpprd endpoint override
- `HAVI_CONFIG` — config directory override
- `HAVI_URL` — startup URL override
- `HAVI_SCREENSHOT` — internal screenshot output path set by
  `--screenshot <output.png>`; screenshot mode bypasses single-instance reuse

Screenshot mode waits for the active page to reach load-complete and then for
active-page visual updates to go quiet. Shell chrome redraws and generic event
loop wakeups do not extend screenshot settling. The final PNG is captured from
the browser-owned page output surface, not from shell chrome composition.

When `HAVI_HOME` is unset, HAVI runs with a local repo under the config
location.

### Public network routing

For `hppr://<group>/<app>/...` without a local route, HAVI may resolve the
public network for an upstream endpoint.

Current HAVI behavior:

- public-network lookup is used for the current navigation only
- HAVI does not auto-install a local route packet from public-network discovery
- routed resolution decisions are printed to stderr with the selected source,
  endpoint, and public-network details
- failed public-network resolution for a public name is a navigation failure,
  not a silent home-repo fallback
- `hppr-join://` is used only for routed `UNAUTHORIZED not a member` failures
- missing Ring2 setup on the target repo is shown as a route setup error, not a
  join flow

## Pylon integration

HAVI runs through pylon.

HAVI exposes pylon management through `havi:///services`.

The shell pylon indicator is status-first. It opens a compact status panel and
links to `havi:///services` for service management.

### Pylon indicator

The toolbar indicator uses current HAVI shell glyphs:

- Booting: orange diamond
- hpprd running or external: green circle
- hpprd starting: orange triangle
- hpprd stopped or failed: red square

These visuals are shell UI details, not protocol semantics.

## Shell behavior

Current HAVI shell behavior:

- the address bar is single-line and strips `\r`, `\n`, and `\t`
- the main toolbar shows browser-first controls only: pylon status, back,
  forward, URL input, reload, and an overflow button
- advanced shell actions such as share, edit, watch, shadow, home, dock, and
  services live in the overflow panel
- `Ctrl+T` on Linux and Windows opens a new tab
- `Command+T` on macOS opens a new tab
- middle click on a tab closes it

## Watch mode

Watch mode is a HAVI tab feature for live reload and change indication.
Two settings control behavior:

**Watch scope**:

- `None` — no watching
- `Page` — watch the current page coordinate only
- `App` — watch the entire app

**Navigate**:

- Off: changes produce a notification badge on the tab
- On: changes trigger automatic page reload

Wire protocol values:

- `none`
- `page`
- `app`
- `page+navigate`
- `app+navigate`

## Shadow mode

Shadow mode is a HAVI local-authoring feature.

When enabled for `//<group>/<app>/`, HAVI resolves routed page loads against a
local shadow root:

`//~<group>/<app>/...`

Current HAVI behavior:

- shadow state is persisted locally
- enabling shadow mode sets tab watch to scope=App, navigate=on

Shadow mode is a HAVI workflow feature, not a generic HPPR browser requirement.

## Renderer architecture

HAVI renders through one retained browser pipeline.
Layout fragments are lowered into retained `makepad-browser-scene` semantic data
and executed by the Makepad compositor.

Current behavior:

- ordinary page content renders through the retained browser-scene/compositor path
- there is no parallel legacy fragment-to-surface renderer
- browser-scene builder and draw failures are logged; HAVI does not switch to a second renderer path
- current retained coverage includes rounded solid boxes, uniform rounded borders,
  exact rounded background clips for retained gradients and images, box shadows,
  retained text, clipped images, and iframe/embed child documents on the retained path
- browser text uses an explicit prepare phase before draw; the compositor owns one global glyph residency cache and explicit per-page GPU textures for browser text
- prepared browser text batches are per-draw snapshots only; they are invalid after any glyph-cache generation or page-generation change
- layout publishes one immutable `FragmentArenaGeneration` per visible generation through `havi-types`
- `paint_api::scroll_tree::ScrollTree` is the immutable structural scroll graph for browser content
- `BrowserScrollController` in havishell owns sampled scroll offsets, hit testing, ancestor handoff, and clamping for default scrolling
- `SharedScrollState` is only a shell snapshot for diagnostics/indicator UI and controller bootstrap; it is not the live render scroll authority
- committed offsets flow back to layout/script through `SetScrollStates`
- unchanged fragment trees with unchanged scroll state reuse the last retained browser document
- `HAVI_BROWSER_SURFACE_CACHE=0` disables the browser-owned retained output-surface cache; by default HAVI promotes stable page content into an offscreen texture and reuses it on unchanged frames

This keeps renderer ownership on the retained path while coverage work continues.

## Inline SVG rollout status

Inline `<svg>` now renders on the native DOM -> layout -> fragment arena ->
browser-scene path. HAVI no longer serializes inline SVG subtrees to data URLs
for layout or first paint.

Current architectural model:

- native fragment kinds: `SVGViewport`, `SVGContainer`, `SVGLeaf`
- container kinds: `Group`, `ForeignObject`
- leaf payload kinds: `Path`, `Text`, `Image`
- all paintable SVG leaves publish shared `SVGBounds`, `SVGPaintStyle`, and
  `SVGEffectState`
- SVG text publishes native `SVGTextPayload { runs, chunks, addressing }`
  instead of HTML text fragments
- native resource kinds: paint servers (`Gradient`, `Pattern`), clip paths,
  masks, filters, markers, and `use` instance sources
- explicit layout subsystem ownership in `components/layout/svg/`
- standalone `image/svg+xml` navigation now creates a real SVG document and
  renders through the native SVG pipeline
- external SVG images (`<img src="foo.svg">`, CSS image URLs) now flow through
  renderer-backed SVG image producers instead of the old layout-triggered
  rasterization path

Current first-cut coverage:

- tags: `svg`, `g`, `path`, `rect`, `circle`, `ellipse`, `line`, `polyline`,
  `polygon`, `defs`, `use`, `linearGradient`, `radialGradient`, `stop`,
  `clipPath`, `foreignObject`, `image`, `text`, `tspan`
- properties: transforms, `viewBox`, `preserveAspectRatio`, solid fill and
  stroke, gradients, `currentColor`, `display`, `visibility`, leaf `opacity`,
  `fill-opacity`, `pointer-events`, `fill-rule`, `clip-rule`,
  `vector-effect: non-scaling-stroke`, placeholder publication for
  `paint-order`, stroke dash data, masks, filters, markers, and patterns,
  basic text positioning attributes
- native resource behavior: `use` instance expansion, paint-server publication,
  clip-path geometry publication, and simple clip-chain lowering from clip-path
  bounds
- renderer behavior: all SVG leaves now traverse one shared leaf builder path;
  retained text is used only for simple solid-fill SVG text and complex SVG text
  lowers through the vector path boundary
- query behavior: shape hit testing now uses SVG path geometry and SVG text run
  bounds rather than only axis-aligned fragment bounds

Still deferred:

- full filters and masks
- markers and patterns beyond publication
- browser-grade SVG text shaping, `textLength`, and `textPath`
- full `foreignObject` HTML formatting-context embedding
- full DOM API parity

## Diagnostics

`havi:///diagnostics` exposes HAVI-specific inspection and test controls.

Current API commands:

- `inspect`
- `join_fixture_get`
- `join_fixture_set`

Join fixture states:

- `none`
- `pending`
- `approved`

## DevTools actors

HAVI exposes chrome-level DevTools actors for shell integration.

### `watch`

Controls tab watch mode.

Operations:

- `getMode`
- `setMode`

### `shell`

Controls HAVI shell navigation and tab activation.

Operations:

- `setUrl`
- `activateTab`

These actors are HAVI shell tooling, not part of the web platform surface.

## Site accounts and route keys

HAVI creates per-origin site Ring1 identities using:

`site:<group>#<app>`

It also maintains per-group route keys for authenticated remote operations.

## Notes on scope

This document describes current HAVI implementation details and supported shell
surfaces.

Normative browser behavior belongs in `havi/spec/`.
