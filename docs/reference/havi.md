# havi Reference

## Synopsis

```bash
havi [--path <dir>] [--home <via>] [URL]
```

Launches the HAVI browser.

- `URL` may be any HAVI-supported scheme (`hppr://`, `hppr-setup://`,
  `hppr-sandbox://`, `hppr-browse://`, `hppr-editor://`, `havi://`).
- If `URL` is omitted, HAVI opens `havi:///overview`.

If another HAVI instance is already running, sends the URL to it via IPC
and exits.

## CLI Flags

- `--path <dir>` — config directory (default: `~/.config/HAVI`)
- `--home <via>` — remote hpprd endpoint (starts pylon in remote mode)

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
- `HAVI_DEVTOOLS` — enable DevTools server (`<port>` or `<host>:<port>`)
- `HAVI_MAKEPAD_EVENTS` — enable Makepad event injection (JSON lines over stdin/stdout)

## Repository Connection

HAVI always goes through [pylon](pylon.md). Two modes:

**Local mode** (default): Pylon owns hpprd. HAVI finds or spawns pylon for
`<config-dir>/repo/`. Pylon auto-starts hpprd as a child process.

**Remote mode** (`HAVI_HOME` or `--home` set): Pylon connects to an external
hpprd. HAVI spawns pylon with `--home <via>`. Pylon manages satellites
(NFS, FUSE, lokid, unlokid) against the external hpprd but does not manage
hpprd itself.

Startup sequence:

1. Connect to running pylon (via `<config-dir>/repo/pylon.pid`)
2. If no pylon found, spawn `pylon` as a subprocess
3. If remote mode, pass `--home <via>` to pylon
4. Hold pylon connection open for app lifetime

## Single-Instance

HAVI listens on `<config-dir>/havi.sock` (Unix socket). A second `havi`
invocation detects the running instance, sends the URL, and exits. The
running instance opens a new tab.

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
