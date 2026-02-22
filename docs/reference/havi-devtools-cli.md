# havi-devtools-cli Reference

## Synopsis

```bash
havi-devtools-cli [--port N] [--timeout SEC] [--text] <command> [args]
```

Connects to a running HAVI instance through the DevTools protocol.
Operates at the Servo webview level (DOM, JS, page coordinates).
For Makepad UI input (mouse, keyboard, touch), use havi-makepad-cli.

Default target: `localhost:${HAVI_DEBUG_PORT:-${SERVO_DEBUG_PORT:-6000}}`.

## Global options

- `-p, --port N` — DevTools port
- `--timeout SEC` — command timeout (default: `10`)
- `--text` — human-readable output (default output is JSON)

## Commands

- `tabs`
  - List open tabs (`index`, `url`, `title`).
- `eval [js] [--await]`
  - Evaluate JavaScript in the first tab.
  - Reads JS from stdin when `js` is omitted.
  - `--await` waits for Promise resolution.
- `navigate <url>`
  - Set `window.address.href` and wait for load completion.
- `wait-for <expr> [--interval MS]`
  - Poll expression until truthy.
- `events`
  - Stream navigation / page error / console events as JSONL.
- `repl`
  - Read JS lines from stdin and emit eval result events as JSONL.
- `screenshot [file]`
  - Capture viewport PNG.
  - Writes to stdout when `file` is omitted.

## Exit codes

- `0` success
- `2` usage error
- `3` connection error
- `4` timeout
- `5` JavaScript exception

## Examples

```bash
havi-devtools-cli -p 6000 eval 'document.title'
havi-devtools-cli -p 6000 eval --await \
  'window.home.get("//u/demo/msg.txt").then(p => p.text())'
havi-devtools-cli -p 6000 navigate 'hppr://u/showcase/index.html'
havi-devtools-cli -p 6000 screenshot /tmp/havi.png
```
