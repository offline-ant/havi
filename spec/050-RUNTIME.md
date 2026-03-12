# Runtime

Credentials, configuration, and watch behavior.

## Credentials

Local admin credentials are stored in SQLite at:

`<config-dir>/havi.sqlite` table `credentials`

Columns:

1. `repo_vkey`
2. `ring1_name`
3. `token`

Startup flow:

1. show native splash screen with logo and status
2. start pylon/hpprd bootstrap in background
3. HELLO to get home repo key
4. load credentials for that key
5. if missing, bootstrap with `ring0/init`
6. hide splash screen, open first tab on `havi:///`

Site credentials for route operations are cached in memory per group/app.

## Configuration

HAVI local state is stored in:

`<config-dir>/havi.sqlite`

`hppr-join` fixture state (`none|pending|approved`) is process-local runtime
state and is not persisted. It resets when HAVI restarts.

Environment variables:

- `HAVI_HOME` — remote hpprd endpoint. When set, pylon runs in remote mode.
  Formats: `tcp+<host>[:<port>]`, `unix+<path>`, `path:<dir>`.
- `HAVI_CONFIG` — config directory override (default: `~/.config/HAVI`).

When `HAVI_HOME` is unset, pylon runs in local mode with `<config-dir>/repo`.

Host integration runtime policy:

- desktop targets (mac/linux/windows): HAVI uses `self_exec` host startup by
  spawning itself as `havi pylon ...`.
- android target: HAVI uses `inline` host startup by starting pylon in an
  internal thread host.
- services can run with daemon runtime selectors `external`, `self_exec`, or
  `inline` (subject to platform/feature support).

All runtime paths keep the same pylon TCP control protocol and service control
line/event shapes.

## Pylon Indicator

The toolbar pylon indicator uses redundant shape and color cues.

Health mapping:

- Booting: orange diamond
- hpprd running or external: green circle
- hpprd starting: orange triangle
- hpprd stopped or failed: red square

The pylon dropdown uses matching shape glyphs for per-service state.

## History

Top-level URL changes are persisted to SQLite table `history`.

Columns:

- `id` (autoincrement)
- `ts_unix`
- `url`
- `title`

## Watch Modes

Per-tab watch mode controls live-reload behavior. Modes:

| Mode   | Behavior |
|--------|----------|
| Off    | No watching. Default. |
| Notify | Flag change on exact coordinate match. No reload. |
| Auto   | Reload on exact coordinate match. |
| Tree   | Reload on any event under the backing app root. |

Mode labels: `W:Off`, `W:Note`, `W:Auto`, `W:Tree`.

Cycling order: Off → Notify → Auto → Tree → Off.

### Event matching

Notify and Auto match when the WATCH event line contains the tab's exact
coordinate (e.g. `//group/app/path`).

Tree matches any event received on the current backing-root connection.
When shadow mode is active, this is the local shadow root `//~<group>/<app>/`.

### Watch actions

Event matching produces one of:

- `None`: no matching event
- `ChangeDetected`: Notify mode sets an indicator flag
- `Reload`: Auto and Tree modes trigger page reload

When multiple events arrive in one poll, the highest-priority action wins
(Reload > ChangeDetected > None).

## Shadow Mode

Shadow mode is separate from watch mode.

When enabled for `//<group>/<app>/`, HAVI resolves routed `hppr://<group>/<app>/...`
page loads against the local shadow root:

- `//~<group>/<app>/...`

Shadow mode state is persisted in local HAVI SQLite state.

Entering shadow mode creates or reuses a persistent local shadow signing key for
that origin, seeds the current page into the shadow tree when resolution
succeeds, enables the local shadow override, and switches the tab watch mode to
`Tree`.

The shadow signing key is local draft identity. Publishing may re-seal content
with a different target signer.

## Watch Pool

Connections are shared per backing-root prefix. Each connection runs HPPR
`🖧WATCH` ([050-RING1](../../hppr/spec/050-RING1.md)) on the prefix with a
trailing slash.

Pool behavior:

- keyed by backing-root string such as `//group/app/` or `//~group/app/`
- created lazily on first non-Off watch mode use
- connections are reference-counted; dropped when no tabs use them
- dead entries are cleaned on next access
- subscribers receive events via channels; a wake function triggers UI updates

A tab acquires a connection when its mode is not Off and its URL has a valid
group and app. Changing URL releases the old connection and acquires a new one
if the resolved backing-root prefix differs.

## DevTools Watch Actor

The watch actor exposes watch mode control via the DevTools protocol.

Actor name: `watch`.

### getMode

Request:

```json
{"to": "watch", "type": "getMode"}
```

Optional `browserId` field targets a specific tab. When omitted, targets the
active tab.

Response:

```json
{"from": "watch", "mode": "off"}
```

### setMode

Request:

```json
{"to": "watch", "type": "setMode", "mode": "auto"}
```

Valid mode values: `off`, `notify`, `auto`, `tree`.

Optional `browserId` field targets a specific tab. When omitted, targets the
active tab.

Response:

```json
{"from": "watch", "mode": "auto"}
```

### Errors

Invalid `browserId`:

```json
{"from": "watch", "error": "unknown browserId"}
```

Invalid mode value:

```json
{"from": "watch", "error": "invalid mode"}
```

Timeout (5 second deadline):

```json
{"from": "watch", "error": "watch timeout"}
```

## DevTools Shell Actor

The shell actor exposes HAVI chrome-level controls via DevTools.

Actor name: `shell`.

### setUrl

Request:

```json
{"to": "shell", "type": "setUrl", "url": "hppr://u/web/index.html"}
```

Optional `browserId` field targets a specific tab. When omitted, targets the
active tab.

Response:

```json
{"from": "shell", "ok": true, "url": "hppr://u/web/index.html"}
```

### activateTab

Request:

```json
{"to": "shell", "type": "activateTab", "browserId": 1}
```

Response:

```json
{"from": "shell", "ok": true}
```

### Errors

Unknown browser id:

```json
{"from": "shell", "error": "unknown browserId"}
```

Timeout (5 second deadline):

```json
{"from": "shell", "error": "shell timeout"}
```
