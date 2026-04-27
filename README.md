# HAVI

HAVI is an HPPR-native browser distribution with bundled HPPR tools.

Current HAVI runtime still starts through pylon and `hpprd`. On desktop, when
`HAVI_HOME` is unset, that compatibility repo path defaults to
`~/.config/HAVI/repo`. Browser-local non-repo state stays in
`~/.config/HAVI/havi.sqlite`. HAVI can also connect to an external `hpprd` via
`HAVI_HOME`.

This runtime plumbing is not the ordinary page capability model. Ordinary pages
use the committed `window.source` descriptor plus explicit named clients, not
`window.home` / `window.route` ambient handles.

## Documentation

Reference docs for command-line tools live in `docs/reference/<tool>.md`.
Practical workflows live in `docs/guide/`. SDK docs live in
`docs/sdk/<lang>/`.

- `docs/reference/` — command references (`havi-cli`,
  `havi-makepad-cli`, `havi-devtools-cli`, HPPR tools)
- `docs/guide/` — task guides (publishing, access patterns)
- `docs/sdk/` — JS and Python client docs

In packaged artifacts, specs are under `docs/spec/havi/` and `docs/spec/hppr/`.

## Core tools

- `havi` — browser runtime
- `havi-cli` — Orchestrator (publish, trust, navigate)
- `havi-makepad-cli` — Makepad UI control via Unix socket
- `havi-devtools-cli` — DevTools protocol client
- `hppr`, `hpprd`, `mkpac`, `ckpac`, `hppr-nfs` — HPPR tooling

## Start

```bash
./bin/havi
```

Use external repo:

```bash
HAVI_HOME=tcp+127.0.0.1:4777 ./bin/havi
```

For behavior details, read `docs/spec/havi/` and `docs/spec/hppr/`.
