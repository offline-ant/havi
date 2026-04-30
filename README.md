# HAVI

HAVI is an HPPR-native browser distribution with bundled HPPR tools.

Default HAVI browsing now uses a browser-owned local packet store. On desktop,
when `HAVI_HOME` is unset, packet-native browser-local state lives in
`~/.config/HAVI/havi-packets.sqlite` and non-packet browser state lives in
`~/.config/HAVI/havi.sqlite`.

HAVI can also connect to an explicit external `hpprd` via `HAVI_HOME`. Pylon is
operator/service machinery, not part of the default browsing path.

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
