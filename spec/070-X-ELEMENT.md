# The `<x>` Element

`<x>` embeds HPPR content in a nested browsing context.

## Purpose

`<x>` is HPPR-native embedding.

Compared with HTTP iframe assumptions, `<x>` supports coordinate-aware loading,
trust-policy integration, and packet access.

## Basic usage

```html
<x src="//chess/game/board.html" width="800" height="600"></x>
<x src="//app/widgets/sidebar.html" trustParent></x>
<x src="components/header.html"></x>
```

## Attributes

- `src`: URC or relative coordinate
- `trustParent`: inherit parent trust keys
- `width`, `height`: display size
- `watch`: enable automatic reload on coordinate changes

## Properties

- `packet`: loaded `HpprPacket`, `null` before load
- `contentDocument`: same-origin only, otherwise `null`
- `contentWindow`: always `WindowProxy`, cross-origin restricted

## Events

- `load`
- `error`

## Trust behavior

Default mode evaluates trust independently for the embedded origin.

`trustParent` makes embedded content use the parent trusted signer set.
Use `trustParent` for same-app widget composition.

## Cross-origin rules

Origin boundary: `//<group>/<app>/`.

Cross-origin behavior matches iframe rules:

- `contentDocument` is inaccessible (`null`)
- restricted `contentWindow` access throws `SecurityError`

See `020-SECURITY-MODEL.md` for sandbox boundaries.

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

Elements sharing a watch prefix share a single WatchSocket connection per
document, managed by a refcounted pool. The connection closes when the last
element using that prefix is removed or stops watching.

`watch` is a no-op when `src` is not an HPPR coordinate.

No auto-reconnect on WatchSocket error. An `error` event fires on the element.
