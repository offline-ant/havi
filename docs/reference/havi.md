# havi Reference

## Synopsis

```bash
havi [OPTIONS] [URL]
```

Launches the HAVI browser.

- `URL` may be any HAVI-supported scheme (`hppr://`, `hppr-setup://`,
  `hppr-sandbox://`, `hppr-browse://`, `hppr-editor://`, `havi://`).
- If `URL` is omitted, HAVI opens `havi:///overview`.

## Runtime configuration

Environment variables:

- `HAVI_HOME` — home directory for HAVI state (default: `~/.config/HAVI`)
- `HAVI_REPO` — repo endpoint override
  - `tcp+<host>[:<port>]`
  - `unix+<path>`
  - `path:<dir>`

When `HAVI_REPO` is unset, HAVI runs an embedded repo at
`$HAVI_HOME/repo`.

## Common options

- `--devtools <port>`: enable DevTools server on the given port.

For full shell options inherited from the underlying browser shell, run:

```bash
havi --help
```

## Examples

Start with embedded home repo:

```bash
./bin/havi
```

Start against external repo:

```bash
HAVI_REPO=tcp+127.0.0.1:4777 ./bin/havi
```

Start with DevTools and open a page:

```bash
./bin/havi --devtools 6000 hppr://u/showcase/index.html
```
