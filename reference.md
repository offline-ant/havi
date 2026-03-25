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
loop wakeups do not extend screenshot settling.

When `HAVI_HOME` is unset, HAVI runs with a local repo under the config
location.

### Bootstrap routing

For `hppr://<group>/<app>/...` without a local route, HAVI may query the
bootstrap index for an upstream endpoint.

Current HAVI behavior:

- bootstrap lookup is used for the current navigation only
- HAVI does not auto-install a local route packet from bootstrap discovery
- routed resolution decisions are printed to stderr with the selected source and
  endpoint
- `hppr-join://` is used only for routed `UNAUTHORIZED not a member` failures
- missing Ring2 setup on the target repo is shown as a route setup error, not a
  join flow

## Pylon integration

HAVI runs through pylon.

HAVI exposes pylon controls through `havi:///services`.

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
- unchanged fragment trees with unchanged scroll state reuse the last retained browser document

This keeps renderer ownership on the retained path while coverage work continues.

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
