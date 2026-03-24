# The `<x>` Element

`<x>` embeds HPPR content in a nested browsing context.

## Purpose

`<x>` is HPPR-native embedding.

Compared with HTTP iframe assumptions, `<x>` supports coordinate-aware loading,
content-signer-aware policy selection, watch reloads, and packet access.

## Basic usage

```html
<x src="//chess/game/board.html" width="800" height="600"></x>
<x src="//app/widgets/sidebar.html" policy="isolated"></x>
<x src="components/header.html"></x>
<x src="hppr-sandbox://preview?url=//app/demo/test.html"></x>
```

## Attributes

- `src`: URC or relative coordinate
- `policy`: `auto`, `isolated`, or `strict`
- `width`, `height`: display size
- `watch`: enable automatic reload on coordinate changes

Default `policy` is `auto`.

## Properties

- `packet`: loaded `HpprPacket`, `null` before load
- `contentSigner`: resolved child content signer, or `null`
- `embedMode`: `inherited`, `isolated`, `strict`, or `sandbox-preview`
- `contentDocument`: available only in `inherited` mode under ordinary
  same-origin rules
- `contentWindow`: `WindowProxy`; restricted outside `inherited` mode

## Events

- `load`
- `error`

## Content-signer comparison

`<x>` compares parent and child content signer by exact verification-key
equality.

Comparison source:

- app-content URLs use `Content-Signer`
- direct explicit Seal URLs use resolved `Seal-By`
- unsealed direct content has no content signer
- `hppr-sandbox://` has no content signer

Matching `//<group>/<app>` is not enough.
Matching route or repo endpoint is not enough.

## Embed modes

`<x>` has four embed modes:

- `inherited`
- `isolated`
- `strict`
- `sandbox-preview`

### inherited

Conditions:

- `policy="auto"`
- child content signer equals parent content signer

Behavior:

- scripts enabled
- ordinary app-origin behavior
- ordinary same-origin DOM access rules
- `contentDocument` available only when ordinary same-origin rules allow it
- same-publisher composition behaves like a normal nested app document

### isolated

Conditions:

- `policy="isolated"`, or
- `policy="auto"` with signer mismatch, or
- `policy="auto"` with no child content signer

Behavior:

- scripts enabled
- isolated sandboxed origin
- no parent-child DOM access
- no privilege inheritance from parent
- no top navigation escape
- no popup escape
- `contentDocument` is inaccessible
- `contentWindow` is restricted to cross-origin-safe `WindowProxy` behavior

Cross-signer default is `isolated`.

### strict

Conditions:

- `policy="strict"`

Behavior:

- scripts disabled
- isolated sandboxed origin
- stronger sandbox restrictions than `isolated`
- render-only or near-render-only embedding
- `contentDocument` is inaccessible
- `contentWindow` is restricted

### sandbox-preview

Conditions:

- `src` uses `hppr-sandbox://`

Behavior:

- explicit preview mode
- anonymous fetch
- current sandbox preview CSP
- no content signer
- DOM access behaves like `isolated`

## Policy selection

Policy selection happens before final child sandbox or origin mode is fixed.
Post-load correction is insufficient.

Mode selection rules:

1. `hppr-sandbox://` -> `sandbox-preview`
2. `policy="strict"` -> `strict`
3. `policy="isolated"` -> `isolated`
4. `policy="auto"`:
   - same content signer -> `inherited`
   - different content signer -> `isolated`
   - missing child content signer -> `isolated`

`policy="auto"` never overrides a signer mismatch.
There is no `trustParent` attribute.

## DOM access rules

### inherited

Current same-origin rules apply.

### isolated

- `contentDocument` is inaccessible
- `contentWindow` remains a restricted cross-origin-safe `WindowProxy`
- parent and child cannot access each other's DOM even if app origin matches

### strict

- same DOM-access restrictions as `isolated`
- scripts disabled in the child

### sandbox-preview

- same DOM-access restrictions as `isolated`

## Watch behavior

`watch` enables automatic content reload when new packets arrive at the
element's coordinate.

```html
<x src="//chess/game/board.html" watch></x>
<x src="//app/widgets/sidebar.html" watch="//app/widgets/"></x>
```

Default (`watch` with no value): watches at `//group/app/` level derived from
`src`. Multiple elements sharing the same app share one WatchSocket connection.

Explicit (`watch="<prefix>"`): watches at the specified prefix.

On `+` event matching the element's coordinate, the element reloads its content.
`-` events are ignored.

Every watch-triggered reload re-runs content resolution and embed-mode
selection. If the resolved content signer changes, the element updates
`contentSigner` and `embedMode` accordingly.

Elements sharing a watch prefix share a single WatchSocket connection per
document, managed by a refcounted pool. The connection closes when the last
element using that prefix is removed or stops watching.

`watch` is a no-op when `src` is not an HPPR coordinate.

No auto-reconnect on WatchSocket error. An `error` event fires on the element.
