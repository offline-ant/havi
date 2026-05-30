# HAVI Reference

HAVI-specific tools, runtime details, and operator surfaces live here.

The `havi/spec/` directory defines the behavior needed to implement a generic
HPPR-native browser. This document covers HAVI's extra tooling and current
browser shell behavior.

## Internal schemes

HAVI provides browser-owned helper schemes in addition to `hppr://` and
`file://`.

### `hppr-sandbox://`

Untrusted preview mode.

### `hppr-browse://`

Read-only directory explorer for coordinate trees.

### `havi://`

Internal HAVI administration pages.

Current surviving internal pages include:

- `havi:///home-repo` — repo status, name, and named-client grant management
- `havi:///diagnostics`

Deleted in this cut:

- `havi:///overview`
- `havi:///services`

These pages are implementation-defined UI. They are not part of the generic
HPPR browser spec.

## Committed page source and helper metadata

Current HAVI page classes are explicit:

- ordinary `hppr://` documents commit `hppr_source` as the page source of truth
  at document commit time
- ordinary `hppr://` documents derive endpoint and signer from that committed
  source snapshot; they do not receive ambient site/home credential blobs
- ordinary repo-backed documents now expose `window.source` with committed
  `client`, `authority`, and `kind`
- committed repo-backed `window.source.client` uses a browser-owned local
  backend path instead of fake endpoint/signing metadata
- ordinary pages no longer expose `window.home` or `window.route`
- raw `HpprClient.connect*()` is helper/privileged-only
- ordinary routed media resolution reuses the committed document source snapshot
  through the originating `pipeline_id`
- `file://` documents do not carry `hppr_source`, endpoint, signer, or admin
  credentials, and `window.source === null`
- helper pages do not carry `hppr_source` by default; `window.source === null`
  there, and helper-only privileged behavior stays page-owned through explicit
  `havi:///.../api?...` handlers instead of document transport metadata or a
  generic JS helper object
- non-HPPR pages carry no HPPR source metadata and no helper credential
  metadata

## Local runtime and configuration

Environment variables:

- `HAVI_HOME` — remote hpprd endpoint override
- `HAVI_CONFIG` — config directory override
- `HAVI_URL` — startup URL override
- `HAVI_SCREENSHOT` — internal screenshot output path set by
  `--screenshot <output.png>`; screenshot mode bypasses single-instance reuse
- `HAVI_BROWSER_SURFACE_CACHE` — set to `0` to disable the browser-owned
  retained output-surface cache; by default HAVI promotes only exact-present
  page output into an offscreen texture and reuses it on unchanged frames when
  physical placement, clip state, and renderer visual generation still match

Desktop HAVI embeds its browser-owned runtime resources and its Makepad package
resources by default through `mach-havi` using embedded-resource builds with the
small-font profile. It does not require an adjacent Makepad `makepad/`
resource tree or a browser `resources/` directory for the browser's built-in
HSTS list, error pages, broken-image placeholder, or DevTools helper script.

The default small-font build does not guarantee full emoji or CJK coverage.
HAVI does not currently rely on system emoji fallback as a supported runtime
font path.

Screenshot mode waits for the active page to reach load-complete and then for
active-page visual updates to go quiet. Shell chrome redraws and generic event
loop wakeups do not extend screenshot settling. The final PNG is captured from
the browser-owned page output surface, not from shell chrome composition.
Stable surface-cache reuse is separate from this capture path.

When `HAVI_HOME` is unset, default HAVI browsing uses the browser-owned packet
store at `<config_dir>/havi-packets.sqlite`. Browser-local non-repo state stays
in `<config_dir>/havi.sqlite`.

Default browsing does not start `hpprd` just to back ordinary
`window.source.client` access. Browser-local packets are stored in
`<config_dir>/havi-packets.sqlite`.

### Public network routing

For `hppr://<group>/<api>//...` without a local route, HAVI may resolve the
public network for an upstream endpoint.

Current HAVI behavior:

- public-network lookup is used for the current navigation only
- HPPR error pages include a collapsed `Lookup details` section showing the
  committed HPPR lookup trace when available
- HAVI does not auto-install a local route packet from public-network discovery
- routed resolution decisions are printed to stderr with the selected source,
  endpoint, and public-network details
- failed canonical public lookup for a public name is a navigation failure
  unless local exact-group or terminal local exact-API records supply the
  effective route answer
- HAVI does not silently fall back to generic browser-local repo content for that case
- routed `UNAUTHORIZED not a member` failures now stay explicit error pages
- missing Ring2 setup on the target repo is shown as a route setup error

## Shell behavior

Current HAVI shell behavior:

- the address bar is single-line and strips `\r`, `\n`, and `\t`
- a bare public group name like `eu` or `lab.eu` resolves to that group's
  landing page as `hppr://<group>/<home-api>//index.html`; when no effective
  `Home-API` is configured, HAVI uses `home`
- desktop HAVI disables the stock Makepad caption bar and uses empty tab-strip
  space as the draggable caption region
- the main toolbar shows browser-first controls only: back, forward, URL input,
  an info button, reload, and an overflow button
- the info button opens a right-side current-tab inspector panel
- the inspector panel shows committed page source fields, packet info, and the
  current HPPR lookup trace when available
- inspector v1 actions are copy lookup trace, open `havi:///diagnostics`, and
  open the final resolved target when the trace has one
- advanced shell actions such as share, watch, shadow, home, and dock live in
  the overflow panel
- `Ctrl+T` on Linux and Windows opens a new tab
- `Command+T` on macOS opens a new tab
- middle click on a tab closes it

## Watch mode

Watch mode is a HAVI tab feature for live reload and change indication.
Two settings control behavior:

**Watch scope**:

- `None` — no watching
- `Page` — watch the current page coordinate only
- `App` — watch the entire API

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

When enabled for `//<group>/<api>//`, HAVI resolves routed page loads against a
local shadow root:

`//~<group>/<api>//...`

Current HAVI behavior:

- shadow state is persisted locally
- enabling shadow mode sets tab watch to scope=App, navigate=on

Shadow mode is a HAVI workflow feature, not a generic HPPR browser requirement.



## Diagnostics

`havi:///diagnostics` exposes HAVI-specific inspection and test controls.

The toolbar info panel is the fast current-page inspector. `havi:///diagnostics`
remains the deeper active probe tool.

Current API commands:

- `inspect`

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

## Route auth

HAVI no longer grants ordinary pages per-origin site Ring1 identities or
ambient `window.home` credentials.

Current browser-owned auth writes are limited to local route auth records under
`//repo/route/auth/` as part of HPPR route-scheme behavior.
Exact-API auth overrides group-default auth.
When no local route auth record exists, routed access falls back to `anyone`.
Join/setup pages are HAVI UI on top of that general HPPR mechanism.

## Notes on scope

This document describes HAVI-specific tools, shell behavior, and operator
surfaces.

Normative browser behavior belongs in `havi/spec/`.
Renderer architecture and internal design decisions belong in `RENDERER.md`.
