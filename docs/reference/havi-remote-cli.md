# havi-remote-cli Reference

## Synopsis

```bash
havi-remote-cli [--socket PATH] <command> [args]
```

Controls a running HAVI instance through the Makepad UI layer via a Unix
domain socket. Operates on the native widget tree (tab bar, address bar,
webview container), not the web content inside it.

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
  - Click at logical pixel coordinates (mousedown + mouseup).
- `mousedown <x> <y>`
  - Press mouse button at coordinates.
- `mouseup <x> <y>`
  - Release mouse button at coordinates.
- `mousemove <x> <y>`
  - Move mouse to coordinates.
- `type <text>`
  - Send text input.
- `key <name>`
  - Press a key (`enter`, `tab`, `escape`, `backspace`, `a`-`z`, etc.).
- `touch <x> <y>`
  - Touch event at coordinates.
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
# Start HAVI with control mode
./mach-havi run --control
# prints: HAVI_CONTROL=/tmp/havi-control-12345.sock

# Take a screenshot
havi-remote-cli -s /tmp/havi-control-12345.sock screenshot /tmp/out.png

# Select text by dragging
havi-remote-cli -s $HAVI_CONTROL mousedown 200 120
havi-remote-cli -s $HAVI_CONTROL mousemove 500 120
havi-remote-cli -s $HAVI_CONTROL mouseup 500 120
havi-remote-cli -s $HAVI_CONTROL screenshot /tmp/selected.png

# Widget tree
havi-remote-cli -s $HAVI_CONTROL dump
havi-remote-cli -s $HAVI_CONTROL query id:address_bar

# Pipe mode
echo -e "click 200 120\nsleep 500\nscreenshot /tmp/out.png" | \
  havi-remote-cli -s $HAVI_CONTROL pipe
```
