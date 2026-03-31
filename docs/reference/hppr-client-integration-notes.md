# HPPR Client Integration Notes (HAVI)

This note tracks alignment with hppr-client ergonomics improvements.

## Applied in HAVI

- WATCH endpoint parsing uses `hppr_client::parse_via`
  (`tcp+host:port` and `host:port`).
  - Module: `libhavi::hppr::watch`
- WATCH events use `hppr_client::parse_watch_event`
  instead of raw-line matching.
  - Module: `libhavi::hppr::watch`
- Sandbox fetch resolves hostnames via `ToSocketAddrs`.
  - Module: `libhavi::pages::hppr_sandbox`

## Not currently relevant in HAVI runtime

- `ring2_members_tip_urc` is not consumed yet.
- HAVI runtime reads routes/content and streams watch data;
  it does not materialize Ring2 membership packets.
- When HAVI adds Ring2 membership flows, use
  `hppr_client::ring2_members_tip_urc` instead of
  local string formatting.
