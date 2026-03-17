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

Start HAVI with `./mach-havi run` on desktop. The Makepad event socket is
enabled automatically and the socket path is printed as
`HAVI_MAKEPAD_SOCKET=<path>`.

## Global options

- `-s, --socket PATH` — Unix event socket path (default: `$HAVI_MAKEPAD_SOCKET`)

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
  - Press a key. Accepts friendly names (`enter`, `tab`, `escape`, `f5`,
    `a`-`z`, `0`-`9`) or Makepad variant names (`ReturnKey`, `ArrowUp`).
    Case-insensitive. Key codes are sent as integer indices matching
    Makepad's `KEYCODE_VARIANTS` serialization.
- `touch <id> <phase> <x> <y>`
  - Touch event at window coordinates.
- `sleep <ms>`
  - Wait for milliseconds (for sequencing in scripts).
- `control <id> <op> [arg...]`
  - Invoke a widget control operation by widget id.
  - Use `dump` or `query` to discover ids and advertised control ops.
  - HAVI semantic controls:
    - `nav_control get|set|focus|go|edit`
    - `watch_control get|set|next`
    - `shadow_control get|set|enter|exit`
    - `dock_control get|set|toggle`
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

`mach-havi run` relays between the socket and HAVI's stdin/stdout on desktop.
Multiple clients can connect simultaneously.

## Examples

```bash
# Take a screenshot
havi-makepad-cli -s $HAVI_MAKEPAD_SOCKET screenshot /tmp/out.png

# Click the address bar
havi-makepad-cli -s $HAVI_MAKEPAD_SOCKET click 400 55

# Widget tree
havi-makepad-cli -s $HAVI_MAKEPAD_SOCKET dump
havi-makepad-cli -s $HAVI_MAKEPAD_SOCKET query id:nav_control

# Semantic chrome controls
havi-makepad-cli -s $HAVI_MAKEPAD_SOCKET control nav_control go hppr://u/web/index.html
havi-makepad-cli -s $HAVI_MAKEPAD_SOCKET control watch_control set auto
havi-makepad-cli -s $HAVI_MAKEPAD_SOCKET control shadow_control enter
havi-makepad-cli -s $HAVI_MAKEPAD_SOCKET control dock_control set bottom

# Pipe mode
echo -e "control watch_control set notify\ncontrol nav_control go havi:///services\nscreenshot /tmp/out.png" | \
  havi-makepad-cli -s $HAVI_MAKEPAD_SOCKET pipe
```
