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
- `window.route` targets configured route endpoints and may fail offline.

Use `window.home` for persistence and offline reads.
Use `window.route` for fresh remote reads when available.

## Caching behavior

Route fetches are cached automatically in the home repo.

When `window.route.get()` succeeds, HAVI stores returned packets locally before
returning them to page code.

Chunk data fetched for manifest reassembly is also cached.

## Recommended access patterns

- local-first with remote fallback
- remote-first with local fallback
- background refresh with cache comparison

Treat route failures as normal runtime states.

## Connectivity detection

HAVI has no dedicated online/offline API.

Practical checks:

- `window.route === null`: no route exists or no matching route auth key exists
- route operation throws fatal error: route is unreachable or session failed

## WATCH while offline

Route watches require an active remote connection.

Disconnects end watch streams. Reconnect with backoff when continuous updates
are required.

## Offline-first app guidance

1. write local state to `user/`
2. keep cached copies of required remote content
3. handle route failures gracefully
4. sync queued local changes after reconnection
5. resolve divergence explicitly
