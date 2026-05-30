# havi-cli Reference

## Synopsis

```bash
havi-cli <command> [args]
havi-cli //<group>/<api>//<key> [--shadow] [--mount PATH]
```

Orchestrator for HAVI's command-line tools. Delegates to havi-makepad-cli
and havi-devtools-cli, and provides built-in workflow commands.

## Subcommand groups

- `makepad <cmd> ...` — delegates to havi-makepad-cli (Makepad UI control)
- `devtools <cmd> ...` — delegates to havi-devtools-cli (Servo webview control)

## Built-in commands

- `navigate <url>`
  - Navigate HAVI to a URL via havi-devtools-cli.
- `deploy <group> <api> <content-root> <content-authority>`
  - Write API content pointer packet for routed API resolution.
- `publish <coordinate> <file>`
  - Store a signed file and navigate.
- `publish-dir <coordinate> <dir>`
  - Retired command. Exits with an error that points to direct `hppr-fuse` or `hppr-nfs` workflows.
- `//<group>/<api>//<key> [--shadow] [--mount PATH]`
  - Open a routed API document directly. `--shadow` enters local shadow mode through the
    shell. `--mount` prints explicit `hppr-fuse` and `hppr-nfs` commands for the
    shadow root instead of mounting automatically.

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
havi-cli deploy u web //u/web// V.EXAMPLE.H3
havi-cli publish //u/web//index.html page.html
hppr-fuse --home "$HPPR_HOME" --signer "$HPPR_SIGNER" --root //u/site// --mount /mnt/hppr --rw --seal-with ring0
havi-cli //dev/hppr.forge//presentation/index.html --shadow --mount /mnt/presentation
```
