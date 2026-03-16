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

Behavior:

- shows local setup UI for `group/app`
- can preview remote content through `hppr-sandbox://`
- may expose privileged local admin access needed to store approved route state

### `hppr-join://`

HAVI join and login flow for Ring2-backed groups.

Behavior:

- local HTML UI for membership login or join request
- may use route credentials for the selected upstream and group
- supports deterministic join fixtures for tests

### `hppr-sandbox://`

Untrusted preview mode.

Behavior:

- anonymous access only
- strict CSP
- JavaScript and active features blocked

### `hppr-browse://`

Read-only directory explorer for coordinate trees.

### `hppr-editor://`

Local packet editor.

Behavior:

- targets local context only
- can edit headers and packet data
- can save through local privileged APIs

### `havi://`

Internal HAVI administration pages.

Current pages include:

- `havi:///overview`
- `havi:///diagnostics`
- `havi:///services`

These pages are implementation-defined UI. They are not part of the generic
HPPR browser spec.

## Diagnostics and fixtures

`havi:///diagnostics` exposes HAVI-specific inspection and test controls.

Current API commands:

- `inspect`
- `join_fixture_get`
- `join_fixture_set`

Join fixture states:

- `none`
- `pending`
- `approved`

Fixture state is process-local and resets on restart.

## Local runtime and configuration

HAVI stores local configuration in:

- `<config-dir>/havi.sqlite`

Current uses include:

- admin credentials
- local history
- shadow-mode state
- other browser-local settings

Environment variables:

- `HAVI_HOME` — remote hpprd endpoint override
- `HAVI_CONFIG` — config directory override

When `HAVI_HOME` is unset, HAVI runs with a local repo under the config
location.

## Pylon integration

HAVI always runs through pylon.

Runtime paths:

- `external`
- `self_exec`
- `inline`

Current host policy:

- desktop targets prefer `self_exec`
- android uses `inline`

HAVI also exposes pylon controls through `havi:///services`.

### Pylon indicator

The toolbar indicator uses current HAVI shell glyphs:

- Booting: orange diamond
- hpprd running or external: green circle
- hpprd starting: orange triangle
- hpprd stopped or failed: red square

These visuals are shell UI details, not protocol semantics.

## Watch mode

Watch mode is a HAVI tab feature for live reload and change indication.
Two orthogonal settings control behavior:

**Watch scope** (cycled by the watch button):

- `None` — no watching
- `Page` — watch the current page coordinate only
- `App` — watch the entire app (all changes under backing root)

**Navigate** (boolean toggle):

- Off: changes produce a notification badge on the tab
- On: changes trigger automatic page reload

Labels: `W:None`, `W:Page`, `W:App`. When navigate is on: `W:Page↻`, `W:App↻`.

Cycling order: `None → Page → App → None`

Wire protocol values: `none`, `page`, `app`, `page+navigate`, `app+navigate`.

Backward compatibility: `off`→none, `notify`→page, `auto`→page+navigate,
`tree`/`dev`→app+navigate.

Connections are pooled by backing-root prefix and shared across tabs.

## Shadow mode

Shadow mode is a HAVI local-authoring feature.

When enabled for `//<group>/<app>/`, HAVI resolves routed page loads against a
local shadow root:

`//~<group>/<app>/...`

Current HAVI behavior:

- shadow state is persisted in local HAVI state
- entering shadow mode creates or reuses a persistent local shadow signing key
- the current page is seeded into the shadow tree when resolution succeeds
- tab watch is set to scope=App, navigate=on
- publishing may re-seal content with a different target signer

Shadow mode is a HAVI workflow feature, not a generic HPPR browser requirement.

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

Legacy `HAVI-site:<group>#<app>` naming may still appear in compatibility code.
`site:<group>#<app>` is the current form.

## Rendering architecture

HAVI webview creation is viewport-first.

Current shell model:

- HAVI creates webviews from explicit physical viewport size and hidpi scale
- HAVI may attach a legacy `RenderingContext` only for compatibility backends
- webview identity and viewport updates do not depend on rendering-context registration
- page rendering remains direct fragment rendering through `havi-render`

This is phase 1 of the rendering cleanup. Optional GPU attachment remains a
separate concern and is not part of mandatory webview creation.

## Notes on scope

This document may describe current HAVI implementation details that change over
time.

Normative browser behavior belongs in `havi/spec/`.
