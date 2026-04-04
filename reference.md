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
- `HAVI_BROWSER_SURFACE_CACHE` — set to `0` to disable the browser-owned
  retained output-surface cache; by default HAVI promotes stable page content
  into an offscreen texture and reuses it on unchanged frames

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

When `HAVI_HOME` is unset, HAVI runs with a local repo under the config
location.

### Public network routing

For `hppr://<group>/<app>/...` without a local route, HAVI may resolve the
public network for an upstream endpoint.

Current HAVI behavior:

- public-network lookup is used for the current navigation only
- HPPR error pages include a collapsed `Lookup details` section showing the
  committed HPPR lookup trace when available
- HAVI does not auto-install a local route packet from public-network discovery
- routed resolution decisions are printed to stderr with the selected source,
  endpoint, and public-network details
- failed canonical public lookup for a public name is a navigation failure
  unless local exact-group or terminal local exact-app records supply the
  effective route answer
- HAVI does not silently fall back to generic home-repo content for that case
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
- a bare public group name like `eu` or `lab.eu` resolves to that group's
  landing page as `hppr://<group>/<home-app>/index.html`; when no effective
  `Home-App` is configured, HAVI uses `home`
- desktop HAVI disables the stock Makepad caption bar and uses empty tab-strip
  space as the draggable caption region
- the main toolbar shows browser-first controls only: pylon status, back,
  forward, URL input, an info button, reload, and an overflow button
- the info button opens a right-side current-tab inspector panel
- the inspector panel shows committed page source fields, packet info, and the
  current HPPR lookup trace when available
- inspector v1 actions are copy lookup trace, open `havi:///diagnostics`, and
  open the final resolved target when the trace has one
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



## Diagnostics

`havi:///diagnostics` exposes HAVI-specific inspection and test controls.

The toolbar info panel is the fast current-page inspector. `havi:///diagnostics`
remains the deeper active probe tool.

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

## Site accounts and route auth

HAVI creates per-origin site Ring1 identities using:

`site:<group>#<app>`

HAVI also reads and writes local route auth records under `//repo/route/auth/`
as part of HPPR route-scheme behavior.
Exact-app auth overrides group-default auth.
When no local route auth record exists, routed access falls back to `anyone`.
Join/setup pages are HAVI UI on top of that general HPPR mechanism.

## Notes on scope

This document describes HAVI-specific tools, shell behavior, and operator
surfaces.

Normative browser behavior belongs in `havi/spec/`.
Renderer architecture and internal design decisions belong in `RENDERER.md`.
