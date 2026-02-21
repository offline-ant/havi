# JSONqa: Query Attributes

JSONqa carries client-side parameters that are outside packet identity.

HPPR coordinates do not use HTTP query strings.

## Motivation

A coordinate identifies stored data.

Apps still need transient view state, such as:

- page number
- filters
- sort keys
- fragment targets

JSONqa appends structured metadata for this state.

## Form

Example:

`//docs/manual/chapter-3{page:5,#:results}`

`{...}` is client-side metadata only.
It is excluded from repository coordinate lookup.

## Syntax examples

```text
{page:5}
{#:section}
{tags:[a,b,c]}
{opts:{dark:1}}
{draft}
{name:"hello world"}
{key:a\:b}
```

Escaping applies to:

`{ } [ ] , : \ " '`

## JavaScript usage

```javascript
const urc = new URC("//docs/manual/page{#:section,page:5}");
urc.fragment;  // "section"
urc.qa;        // {"#": "section", page: "5"}

urc.qa = {"#": "intro", page: "10"};
urc.href;      // "//docs/manual/page{#:intro,page:10}"
```

`Address` delegates JSONqa access to its inner `URC`.
See `060-JS-API.md`.

## Mapping from HTTP patterns

- `?page=5` -> `{page:5}`
- `?a=1&b=2` -> `{a:1,b:2}`
- `#section` -> `{#:section}`
- repeated keys -> arrays
- bracket objects -> nested objects
