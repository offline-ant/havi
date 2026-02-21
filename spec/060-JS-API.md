# JavaScript API

HAVI exposes HPPR APIs on `window`.

These APIs replace HTTP-style fetch patterns with signed packet operations.

## Window globals

- `window.address`: current HPPR address object
- `window.home`: home repo client (always available)
- `window.route`: route repo client (nullable)
- `window.ring0`: admin client (privileged schemes only)
- `window.H3`: crypto namespace (always available)

`window.route` is `null` when no route exists or no matching route auth key is
available.

## H3

Crypto namespace for H3 format operations (BLAKE3 + HSB3).

```webidl
dictionary HpprKeyPair {
    required DOMString signingKey;
    required DOMString verifyingKey;
};

[Exposed=Window, Pref="dom_hppr_enabled"]
namespace H3 {
    [Throws] HpprKeyPair deriveKeyPair(
        DOMString password,
        DOMString name,
        DOMString domainKey,
        optional DOMString phc
    );
    [Throws] DOMString hash((ArrayBuffer or ArrayBufferView or USVString) data);
    [Throws] DOMString sign(DOMString hash, DOMString signingKey);
    [Throws] boolean verify(
        DOMString hash,
        DOMString signature,
        DOMString verifyingKey
    );
    [Throws] HpprKeyPair generateKey();
};
```

### deriveKeyPair

Derive a signing/verifying key pair from a password, context name, and domain
key.

Parameters:

- `password`: the secret string
- `name`: context name (ring1 name, group name, etc)
- `domainKey`: scoping verification key, e.g. `V.xxx.H3`
- `phc`: optional Argon2id parameters string (default:
  `$argon2id$v=19$m=12288,t=3,p=1$`)

Returns `HpprKeyPair` with `signingKey` (`&.xxx.H3`) and `verifyingKey`
(`V.xxx.H3`).

Throws `TypeError` on empty password or invalid parameters.

Example:

```javascript
const { signingKey, verifyingKey } = H3.deriveKeyPair(
  "my-password", "alice", repoKey
);
```

## HpprClient

Primary interface for HPPR commands.

```webidl
[Exposed=Window, Pref="dom_hppr_enabled"]
interface HpprClient {
    [NewObject, Throws]
    static Promise<HpprClient> home(optional HpprRepoOptions options = {});

    [NewObject, Throws]
    static Promise<HpprClient> connect(DOMString endpoint, optional DOMString identity);

    [NewObject] EnvelopeHpprClient envelope();

    readonly attribute DOMString endpoint;
    readonly attribute DOMString? account;
    readonly attribute DOMString? group;
    readonly attribute DOMString? ring1Name;

    Promise<any> get(USVString urc);
    Promise<any> list(USVString urc);
    Promise<any> headers(USVString urc);
    Promise<any> tips(USVString urc);
    Promise<any> members(USVString urc);
    Promise<any> store(HpprPacket packet);
    Promise<any> detach(DOMString hash);
    Promise<any> add(optional HpprAddOptions options = {});
    Promise<any> hello();
    WatchSocket watch(USVString urc);

    StreamIn streamIn(
        USVString prefix,
        optional StreamInOptions options = {}
    );

    StreamOut streamOut(USVString prefix);

    readonly attribute HpprRepoInfo? repo;
};
```

### connect() identity parameter

`connect()` accepts an optional identity string following `Signer::parse()`
format:

- omitted or `""`: anyone (no authentication)
- `!ring1/token`: Ring1 with adhoc token
- `!ring1#&.<b64a>.H3`: Ring1 with explicit key
- `@group#&.<b64a>.H3`: Ring2 with explicit key

Invalid identity strings reject the returned promise with a TypeError.

Common return types:
Common return types:

- `get()`: `HpprPacket`
- `list()/tips()/headers()/members()`: `string[]`
- `store()/add()`: `string[]` of stored hashes
- `watch()`: `WatchSocket`
- `streamIn()`: `StreamIn`
- `streamOut()`: `StreamOut`

`add()` sends headers/data and the repo builds packet layers.
`store()` sends full packet bytes unchanged.

### Chunk manifest transparency

`get()` auto-reassembles chunk manifests and returns rendered content.
Use `envelope()` for manifest-level response inspection.

## Relative resolution

HTML attributes use standard RFC 3986 resolution (href mode).

## Document.packet

For `hppr://` documents, `document.packet` returns the source `HpprPacket`.
For non-HPPR pages, it returns `null`.

## HpprPacket

Packet fields include:

- identity: `hash`, `type`, `dataLength`
- coordinate: `group`, `app`, `location`, `tai`, `coordinate`
- signature: `sealBy`
- header APIs: `getHeader`, `getHeaders`, `headers`, `customHeaders`
- data APIs: `arrayBuffer`, `blob`, `text`, `json`, `raw`

## URC and Address

`URC` models coordinate syntax.
`Address` wraps scheme/endpoint and an inner `URC`. For `hppr://...{via:...}`,
`endpoint` returns the raw `via` value (`host`, `host:port`, or keywords like
`repo`).

`window.address` replaces `window.location` semantics for HAVI pages.
Setting `window.address = url` or `window.address.href = url` navigates
(`PutForwards=href`).
Setting `scheme`, `endpoint`, `group`, `app`, or `location` recomputes the full
URL and navigates.

`qa` and `fragment` exist on `URC` and are delegated through `Address`.
See `075-JSONQA.md`.

## WatchSocket

Watch API for repo watch updates.

`readyState` values:

- `0`: CONNECTING
- `1`: OPEN
- `2`: CLOSING
- `3`: CLOSED

Message payload format:

- `+ <coord>` for store
- `- <coord>` for detach


`<x watch>` elements share WatchSocket connections through a per-document pool
keyed by watch prefix. See `070-X-ELEMENT.md`.

## StreamIn

Live publisher API.

Modes:

- raw mode: write trailer bytes directly
- publisher mode: provide signing key and auto-segment output

Key options:

- `key`
- `headers`
- `maxSegmentSize`
- `flushSeq`

Events:

- `onopen`
- `onerror`
- `onclose`
- `onpacket` (segment hash)

`write()` resolves when queued, not when TCP flush completes.

## StreamOut

Live subscriber API.

Exposes `ReadableStream<Uint8Array>` through `stream`.

Lifecycle events:

- `onopen`
- `onerror`
- `onclose`

## Errors

HPPR methods throw `HpprError`.

Fields:

- `type`
- `detail`
- `fatal`

Common `type` values:

- `NOT_FOUND`
- `FORBIDDEN`
- `UNAUTHORIZED`
- `INVALID`
- `HELLO_REQUIRED`
- `TOO_LARGE`
- `INTERNAL`

When `fatal` is `true`, create a new client connection.

## HpprRepoInfo

Available on `window.ring0.repo`.

Methods:

- `port()`
- `repoPath()`
- `status()` (`embedded` or `external`)

## EnvelopeHpprClient

Debug wrapper that returns both operation value and signed response envelope.

Streaming methods return normal streaming types and are not envelope-wrapped.

## H3 Low-Level Primitives

These methods expose raw BLAKE3 hashing and HSB3 signing. Most sites use
higher-level APIs (`HpprClient`, `connect`, `add`, `store`) and rarely need
direct hash/sign/verify.

### hash

```javascript
const h = H3.hash("hello world");       // raw b64a, 43 chars
const h2 = H3.hash(new Uint8Array([1, 2, 3]));
const h3 = H3.hash(arrayBuffer);
```

BLAKE3-256 hash of data. Accepts `ArrayBuffer`, `ArrayBufferView`, or string.
Strings are hashed as UTF-8 bytes. Returns raw b64a (43 chars), not
`B.<b64a>.H3`.

### sign

```javascript
const sig = H3.sign(hash, "&.xxx.H3");  // raw b64a, 86 chars
```

HSB3 signature of a raw b64a hash (43 chars) using a signing key (`&.xxx.H3`).
Returns raw b64a signature (86 chars).

### verify

```javascript
const ok = H3.verify(hash, sig, "V.xxx.H3");  // boolean
```

Verify an HSB3 signature. Takes raw b64a hash, raw b64a signature, and a
verifying key (`V.xxx.H3`). Returns `true` if valid.

### generateKey

```javascript
const { signingKey, verifyingKey } = H3.generateKey();
```

Generate a fresh random key pair. Returns `HpprKeyPair` with `signingKey`
(`&.xxx.H3`) and `verifyingKey` (`V.xxx.H3`).
Streaming methods return normal streaming types and are not envelope-wrapped.
