# Offline Behavior

HAVI uses a home repo for persistent local state.

Packets fetched earlier stay available when route repos are unreachable.

## Home repo role

Home repo stores:

- fetched packets
- user data under app `user/` paths
- route and trust config
- locally created content

Data persists across browser restarts.

## `window.home` and `window.route`

- `window.home` targets the local home repo and is always available.
- `window.route` targets the resolved route endpoint when one is available and
  may fail offline.
- an effective local route answer, including a terminal local exact-app
  bootstrap, still counts as route-backed

Use `window.home` for persistence and offline reads.
Use `window.route` for fresh remote reads when available.

## Caching behavior

Route fetches are cached automatically in the home repo.

When `window.route.get()` succeeds, HAVI stores returned packets locally before
returning them to page code.

Chunk data fetched for manifest reassembly is also cached.

## Connectivity detection

HAVI has no dedicated online/offline API.

Practical checks:

- `window.route === null`: no route-backed source exists or no usable route endpoint exists after effective resolution
- route operation throws fatal error: route is unreachable or session failed
- absence of local route auth falls back to `anyone`; it does not by itself make
  `window.route` null

## WATCH while offline

Route watches require an active remote connection.

Disconnects end watch streams. Reconnect with backoff when continuous updates
are required.

