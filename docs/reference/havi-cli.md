# havi-cli Reference

## Synopsis

```bash
havi-cli <command> [args]
```

Orchestrator for HAVI's command-line tools. Delegates to havi-makepad-cli
and havi-devtools-cli, and provides built-in workflow commands.

## Subcommand groups

- `makepad <cmd> ...` — delegates to havi-makepad-cli (Makepad UI control)
- `devtools <cmd> ...` — delegates to havi-devtools-cli (Servo webview control)

## Built-in commands

- `navigate <url>`
  - Navigate HAVI to a URL via havi-devtools-cli.
- `trust <group> <app>`
  - Ensure site-trust exists for an origin. Idempotent.
- `publish <coordinate> <file>`
  - Store a signed file, ensure trust, navigate.
- `publish-dir <coordinate> <dir>`
  - Store a directory tree, ensure trust, navigate.

## Environment

- `HAVI_CONTROL` — Makepad control socket path (used by makepad subgroup)
- `HAVI_DEVTOOLS` — DevTools port (used by devtools subgroup and built-in commands)

## Examples

```bash
havi-cli makepad screenshot /tmp/out.png
havi-cli makepad click 300 400
havi-cli devtools eval 'document.title'
havi-cli publish //u/web/index.html page.html
havi-cli trust mysite www
```
