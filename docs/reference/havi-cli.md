# havi-cli Reference

## Synopsis

```bash
havi-cli <command> [args]
havi-cli //<group>/<app>[/location] [--shadow] [--mount PATH]
```

Orchestrator for HAVI's command-line tools. Delegates to havi-makepad-cli
and havi-devtools-cli, and provides built-in workflow commands.

## Subcommand groups

- `makepad <cmd> ...` — delegates to havi-makepad-cli (Makepad UI control)
- `devtools <cmd> ...` — delegates to havi-devtools-cli (Servo webview control)

## Built-in commands

- `navigate <url>`
  - Navigate HAVI to a URL via havi-devtools-cli.
- `deploy <group> <app> <content-root> <content-authority>`
  - Write app content pointer packet for routed app resolution.
- `publish <coordinate> <file>`
  - Store a signed file and navigate.
- `publish-dir <coordinate> <dir>`
  - Store a directory tree and navigate.
- `//<group>/<app>[/location] [--shadow] [--mount PATH]`
  - Open a routed app directly. `--shadow` enters local shadow mode through the
    shell. `--mount` mounts the shadow root with the persistent shadow signing
    key.

## Environment

- `HAVI_MAKEPAD_SOCKET` — Makepad event socket path (used by makepad subgroup,
  and required for `--shadow` shell control)
- `HAVI_DEVTOOLS` — DevTools port
  (used by devtools subgroup and built-in commands)

## Examples

```bash
havi-cli makepad screenshot /tmp/out.png
havi-cli makepad click 300 400
havi-cli devtools eval 'document.title'
havi-cli deploy u web //u/web V.EXAMPLE.H3
havi-cli publish //u/web/index.html page.html
havi-cli //dev/hppr.forge/presentation/index.html --shadow --mount /mnt/presentation
```
