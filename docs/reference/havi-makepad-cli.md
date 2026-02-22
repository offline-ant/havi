# havi-makepad-cli Reference

## Synopsis

```bash
havi-makepad-cli [--socket PATH] <command> [args]
```

Controls a running HAVI instance through the Makepad UI layer via a Unix
domain socket. Operates on the native widget tree (tab bar, address bar,
webview container), not the web content inside it.

All coordinates are in Makepad window space (0,0 = top-left of window).
For webview-level interaction (JS eval, DOM, page coordinates), use
havi-devtools-cli.

Start HAVI with `./mach-havi run --control` to enable control mode. The
socket path is printed as `HAVI_CONTROL=<path>`.

## Global options

- `-s, --socket PATH` — Unix control socket path (default: `$HAVI_CONTROL`)

## Commands

- `screenshot <file>`
  - Capture the full window as PNG.
- `dump`
  - Print the Makepad widget tree.
- `query <pattern>`
  - Search the widget tree. Patterns: `id:name`, `type:name`, or substring.
- `click <x> <y>`
  - Click at window coordinates (mousedown + mouseup).
- `mousedown <x> <y>`
  - Press mouse button at window coordinates.
- `mouseup [<x> <y>]`
  - Release mouse button. Defaults to last position if omitted.
- `mousemove <x> <y>`
  - Move mouse to window coordinates.
- `type <text>`
  - Send text input.
- `key <name>`
  - Press a key (`enter`, `tab`, `escape`, `backspace`, `a`-`z`, etc.).
- `touch <id> <phase> <x> <y>`
  - Touch event at window coordinates.
- `sleep <ms>`
  - Wait for milliseconds (for sequencing in scripts).
- `send <json>`
  - Send a raw JSON StudioToApp message.
- `pipe`
  - Read newline-delimited commands from stdin. Each line is
    `<command> [args...]`, same as CLI arguments.
- `builds`
  - Print connection status.

## Protocol

Communication uses Makepad's `StudioToApp` / `AppToStudio` JSON
serialization over the Unix socket. Each message is one JSON line.

`mach-havi --control` relays between the socket and HAVI's stdin/stdout.
Multiple clients can connect simultaneously.

## Examples

```bash
# Take a screenshot
havi-makepad-cli -s $HAVI_CONTROL screenshot /tmp/out.png

# Click the address bar
havi-makepad-cli -s $HAVI_CONTROL click 400 55

# Widget tree
havi-makepad-cli -s $HAVI_CONTROL dump
havi-makepad-cli -s $HAVI_CONTROL query id:address_bar

# Pipe mode
echo -e "click 200 120\nsleep 500\nscreenshot /tmp/out.png" | \
  havi-makepad-cli -s $HAVI_CONTROL pipe
```
