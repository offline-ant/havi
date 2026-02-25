# Runtime

Credentials, configuration, and watch behavior.

## Credentials

Local admin credentials are stored at:

`~/.config/HAVI/credentials/<home-repo-key>`

Format:

1. ring1 name
2. token

Startup flow:

1. HELLO to get home repo key
2. load credentials for that key
3. if missing, bootstrap with `ring0/init`

Site credentials for route operations are cached in memory per group/app.

## Configuration

Environment variables:

- `HAVI_HOME` — remote hpprd endpoint. When set, pylon runs in remote mode.
  Formats: `tcp+<host>[:<port>]`, `unix+<path>`, `path:<dir>`.
- `HAVI_CONFIG` — config directory override (default: `~/.config/HAVI`).

When `HAVI_HOME` is unset, pylon runs in local mode with `<config-dir>/repo`.

Host integration runtime policy:

- desktop targets (mac/linux/windows): HAVI uses the **Self-Exec Process Runtime**
  host path by spawning itself as `havi pylon ...`.
- android target: HAVI uses the **In-Process Embedded Runtime** host path by
  starting pylon in an internal thread host.
- service dispatch remains protocol-compatible with **External Process Runtime**
  execution paths.

All runtime paths keep the same pylon TCP control protocol contract.

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
