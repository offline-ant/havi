# havi Reference

## Synopsis

```bash
havi [URL]
```

Launches the HAVI browser.

- `URL` may be any HAVI-supported scheme (`hppr://`, `hppr-setup://`,
  `hppr-sandbox://`, `hppr-browse://`, `hppr-editor://`, `havi://`).
- If `URL` is omitted, HAVI opens `havi:///overview`.

If another HAVI instance is already running, sends the URL to it via IPC
and exits.

## Runtime Configuration

Environment variables:

- `HAVI_HOME` — home directory for HAVI state (default: `~/.config/HAVI`)
- `HAVI_HOME` — repo endpoint override, bypasses pylon
  - `tcp+<host>[:<port>]`
  - `unix+<path>`
  - `path:<dir>`
- `HAVI_URL` — override startup URL
- `HAVI_DEVTOOLS` — enable DevTools server (`<port>` or `<host>:<port>`)
- `HAVI_MAKEPAD_EVENTS` — enable Makepad event injection (JSON lines over stdin/stdout)

## Repository Connection

When `HAVI_HOME` is set, HAVI connects directly to that endpoint.

Otherwise, HAVI uses [pylon](pylon.md):

1. Connect to running pylon (via `~/.config/pylon/pylon.port`)
2. If no pylon found, spawn `pylon` as a subprocess
3. Request hpprd start via pylon, poll for hpprd port
4. Hold pylon connection open for app lifetime

## Single-Instance

HAVI listens on `~/.config/HAVI/havi.sock` (Unix socket). A second `havi`
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
# Start with pylon-managed hpprd
havi

# Open a specific page
havi hppr://u/showcase/index.html

# Connect to external repo
HAVI_HOME=tcp+127.0.0.1:4777 havi

# Enable DevTools
HAVI_DEVTOOLS=6080 havi
```
