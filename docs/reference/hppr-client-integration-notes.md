# HPPR Client Integration Notes (HAVI)

This note tracks alignment with hppr-client ergonomics improvements.

## Applied in HAVI

- Endpoint parsing for WATCH now uses `hppr_client::parse_via` semantics (supports `tcp+host:port` and `host:port`).
  - File: `havi-protocols/src/watch.rs`
- WATCH event handling now uses structured parser `hppr_client::parse_watch_event` instead of implicit raw-line matching.
  - File: `havi-protocols/src/watch.rs`
- Socket address resolution in sandbox fetch path now resolves hostnames via `ToSocketAddrs`.
  - File: `havi-protocols/src/pages/hppr_sandbox.rs`

## Not currently relevant in HAVI runtime

- Ring2 membership tip helper (`ring2_members_tip_urc`) is not consumed yet in HAVI protocol handlers.
- Current HAVI runtime paths here do not materialize/update Ring2 membership packets; they mainly read routes/content and stream watch data.
- When HAVI adds Ring2 membership authoring/moderation flows, use `hppr_client::ring2_members_tip_urc` instead of local string formatting.
