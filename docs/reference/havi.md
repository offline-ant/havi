# havi Reference

## Synopsis

```bash
havi [--path <dir>] [--home <via>] [--screenshot <output.png>] [URL]
```

Launches the HAVI browser.

- `URL` may be any HAVI-supported scheme (`hppr://`, `hppr-setup://`,
  `hppr-sandbox://`, `hppr-browse://`, `hppr-editor://`, `havi://`).
- If `URL` is omitted, HAVI opens `havi:///`.

If another HAVI instance from the same build is already running, sends the
URL to it via IPC and exits. If a different HAVI build is already running,
HAVI refuses to start.

## CLI Flags

- `--path <dir>` — config directory (default: `~/.config/HAVI`)
- `--home <via>` — remote hpprd endpoint (starts pylon in remote mode)
- `--screenshot <output.png>` — launch normally using `HAVI_URL`, capture the
  rendered webview content as PNG, write it, and exit

## Runtime Configuration

Environment variables:

- `HAVI_CONFIG` — config directory override (default: `~/.config/HAVI`).
  CLI `--path` takes precedence.
- `HAVI_HOME` — remote hpprd endpoint. When set, pylon starts in remote
  mode. CLI `--home` takes precedence. Formats:
  - `tcp+<host>[:<port>]`
  - `unix+<path>`
  - `path:<dir>`
- `HAVI_URL` — override startup URL
- `HAVI_SCREENSHOT` — internal env used by `--screenshot <output.png>`
- `HAVI_DEVTOOLS` — enable DevTools server (`<port>` or `<host>:<port>`)
- `HAVI_MAKEPAD_EVENTS` — enable Makepad event injection
  (JSON lines over stdin/stdout)

## Local State

HAVI stores local configuration, credentials, history, and persistent shadow
signing keys in:

`<config-dir>/havi.sqlite`

## Repository Connection

HAVI always goes through [pylon](pylon.md).

HAVI links pylon with `embedded-services` enabled by default.

Runtime vocabulary used by HAVI + pylon integration:

- `external`
- `self_exec`
- `inline`

**Local mode** (default): Pylon owns hpprd. HAVI finds or starts pylon for
`<config-dir>/repo/`.

**Remote mode** (`HAVI_HOME` or `--home` set): Pylon connects to an external
hpprd. HAVI starts pylon with `--home <via>`. Pylon manages satellites
(NFS, FUSE, lokid, unlokid) against the external hpprd but does not manage
hpprd itself.

Startup sequence:

1. Connect to running pylon (via `<config-dir>/repo/pylon.pid`)
2. If no pylon found:
   - desktop (macOS/Linux/Windows): use `self_exec` host startup by spawning
     self as `havi pylon ...`
   - Android: use `inline` host startup by starting pylon in an internal
     thread host
3. If remote mode, pass `--home <via>` to pylon
4. Hold pylon connection open for app lifetime

`havi pylon ...` command compatibility is preserved, including
`havi pylon exec hpprd ...`.

When the binary is invoked via a service self-name (argv0 rename), HAVI routes
that invocation into pylon dispatch so service CLI behavior stays consistent.

## Single-Instance

HAVI uses a build-specific single-instance endpoint plus a stable active
pointer in the config/runtime directory. A second `havi` invocation reuses the
running instance only when it matches the same build identity. The running
instance opens the URL in a new tab.

If the active instance belongs to a different HAVI build, startup is denied.
This prevents different binaries from silently sharing the same local state,
files, or database.

## Startup Output

HAVI prints eval-compatible environment on stderr:

```
HPPRD_REPO=/path/to/repo
PYLON=127.0.0.1:4850
HAVI_URL=havi:///overview
HAVI_DEVTOOLS=127.0.0.1:6080
```

## Examples

```bash
# Start with pylon-managed hpprd (local mode)
havi

# Open a specific page
havi hppr://u/showcase/index.html

# Connect to external repo (remote mode)
havi --home tcp+127.0.0.1:4777

# Same via environment
HAVI_HOME=tcp+127.0.0.1:4777 havi

# Custom config directory
havi --path /data/havi-config

# Enable DevTools
HAVI_DEVTOOLS=6080 havi
```

Shadow mode is entered from HAVI shell chrome or through `havi-cli //<group>/<app>
--shadow`. Shadow resolution is local and uses a persistent per-origin shadow
signing key stored in `havi.sqlite`.
