# HAVI

HAVI is an HPPR-native browser distribution with bundled HPPR tools.

It can run with an embedded home repo or connect to an external `hpprd`.

## Documentation

Reference docs for command-line tools live in `docs/reference/<tool>.md`. SDK
docs live in `docs/sdk/<lang>/`.

- `docs/reference/` — command references (`havi-cli`, `havi-makepad-cli`, `havi-devtools-cli`, HPPR tools)
- `docs/sdk/` — JS and Python client docs

In packaged artifacts, specs are under `docs/spec/havi/` and `docs/spec/hppr/`.

## Core tools

- `havi` — browser runtime
- `havi-cli` — Orchestrator (publish, trust, navigate)
- `havi-makepad-cli` — Makepad UI control via Unix socket
- `havi-devtools-cli` — DevTools protocol client
- `hppr`, `hpprd`, `mkpac`, `ckpac`, `dir-pac`, `pac-dir` — HPPR tooling

## Start

```bash
./bin/havi
```

Use external repo:

```bash
HAVI_REPO=tcp+127.0.0.1:4777 ./bin/havi
```

For behavior details, read `docs/spec/havi/` and `docs/spec/hppr/`.
