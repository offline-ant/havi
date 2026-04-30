# Offline Behavior

HAVI keeps browser-owned local state and packet history across restarts.

Packets fetched earlier may remain available when remote upstreams are
unreachable, but ordinary pages do not program against a separate ambient
`window.home` client anymore.

## Browser-owned local role

The browser-owned local runtime stores:

- fetched packets needed for current behavior
- route and trust config
- history and browser settings
- future capability-management state such as named clients and grants

Current HAVI keeps packet-native local state in the browser-owned packet store
and keeps non-packet history/settings/grants state in `havi.sqlite`.

This runtime stays browser-owned implementation state.
It is not itself an ordinary page API.

## `window.source`

Ordinary repo-backed pages program against `window.source`.

- `window.source.kind === "repo"` means the committed document source is local
  to the browser-owned runtime path
- `window.source.kind === "remote"` means the committed document source depends
  on a remote upstream path and may fail offline
- `window.source === null` on `file://` pages, helper pages, and non-HPPR pages

Use `window.source.client` for ordinary page reads and writes that match the
committed source capability.
Use explicit named clients when the app needs extra repo power beyond that
ambient source.

## Caching behavior

Remote fetches may still be cached through the browser-owned runtime.
Media and subresource fetches reuse the committed source snapshot for the
current document instead of re-deriving a separate ambient repo story.

## Connectivity detection

HAVI has no dedicated online/offline API.

Practical checks:

- `window.source === null`: no ordinary repo-backed source exists for the page
- `window.source.kind === "remote"` plus fatal repo failure: remote source is
  unreachable or session failed
- `window.source.kind === "repo"`: the committed source is already local to the
  browser-owned runtime path

## WATCH while offline

Remote watch and stream behavior still depends on the backend the current client
actually has.
Browser-owned local committed-source and named-client watch/stream paths that do
not yet exist fail explicitly instead of pretending to be transport-backed.

