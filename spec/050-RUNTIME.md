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
| Dev    | Reload on any event under `//group/app/`. |

Mode labels: `W:Off`, `W:Note`, `W:Auto`, `W:Dev`.

Cycling order: Off → Notify → Auto → Dev → Off.

### Event matching

Notify and Auto match when the WATCH event line contains the tab's exact
coordinate (e.g. `//group/app/path`).

Dev matches any event received on the `//group/app/` connection.

### Watch actions

Event matching produces one of:

- `None`: no matching event
- `ChangeDetected`: Notify mode sets an indicator flag
- `Reload`: Auto and Dev modes trigger page reload

When multiple events arrive in one poll, the highest-priority action wins
(Reload > ChangeDetected > None).

## Watch Pool

Connections are shared per `//group/app/` prefix. Each connection runs HPPR
`🖧WATCH` ([050-RING1](../../hppr/spec/050-RING1.md)) on the prefix with a
trailing slash.

Pool behavior:

- keyed by `//group/app/` string
- created lazily on first non-Off watch mode use
- connections are reference-counted; dropped when no tabs use them
- dead entries are cleaned on next access
- subscribers receive events via channels; a wake function triggers UI updates

A tab acquires a connection when its mode is not Off and its URL has a valid
group and app. Changing URL releases the old connection and acquires a new one
if the `//group/app/` prefix differs.

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

Valid mode values: `off`, `notify`, `auto`, `dev`.

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
