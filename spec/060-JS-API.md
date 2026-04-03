# JavaScript API

HAVI exposes HPPR APIs on `window`.

`window.home` and `window.route` are explicit low-level repo clients.
`window.resolve(input)` is the browser-owned high-level source resolver.

## Window globals

- `window.address`: current exact HAVI address object
- `window.home`: home repo client (always available)
- `window.route`: route repo client (nullable)
- `window.resolve(input)`: browser-owned document source resolver
- `window.ring0`: admin client (privileged implementations only)
- `window.H3`: crypto namespace (always available)

`window.route` is `null` when no route exists or no usable route endpoint is
available after effective resolution.
An effective local route answer, including a terminal local exact-app
bootstrap, still exposes `window.route`.
Absence of local route auth falls back to `anyone`.

For `hppr://` routed pages, route/content-pointer resolution happens before
page JS runs. If content-pointer metadata is missing or invalid, navigation
fails with `HpprError` from handler operations.

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

    [NewObject, Throws]
    static Promise<HpprClient> connectRing2Password(
        DOMString endpoint,
        DOMString group,
        DOMString username,
        DOMString password
    );

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

    StreamPub streamPub(
        USVString prefix,
        optional StreamPubOptions options = {}
    );

    StreamSub streamSub(USVString prefix);

    readonly attribute HpprRepoInfo? repo;
};
```

### connect() endpoint parameter

`connect()` takes an HPPR via string as `endpoint`.
Via syntax is defined by `../../hppr/spec/031-VIA-SYNTAX.md`.
Examples include `host`, `quib+host:4776`, `ws+host`, and
`unix+/absolute/path`.

### connect() identity parameter

`connect()` accepts an optional identity string following the HPPR route/auth
identity text format:

- omitted, `""`, or `"anyone"`: anyone
- `ring1:<name>|<password>`: Ring1 password-derived key
- `ring1:<name>|&.<b64a>.H3`: Ring1 explicit key
- `ring2:<group>|&.<b64a>.H3`: Ring2 explicit key
- `ring2:<group>/<user>|<password>`: Ring2 adhoc key
- `ring2:/<user>|<password>`: Ring2 contextual adhoc key

Invalid identity strings reject the returned promise with a TypeError.
The identity grammar itself is defined by the HPPR route scheme.

### connectRing2Password()

`connectRing2Password(endpoint, group, username, password)` creates a remote
client with a Ring2 adhoc signer without requiring the caller to assemble a
signer string manually.
This is a convenience for local auth selection. Routed pages without stored
local route auth still default to `anyone`.

`username` follows one `Location` segment's constraints.
The derived key remains client-side.
It depends only on group, username, and password.
The repo sees only the resulting Ring2 member verification key.

Common return types:

- `get()`: `HpprPacket`
- `list()/tips()/headers()/members()`: `string[]`
- `store()/add()`: `string[]` of stored hashes
- `watch()`: `WatchSocket`
- `streamPub()`: `StreamPub`
- `streamSub()`: `StreamSub`

`add()` sends headers/data and the repo builds packet layers.
`store()` sends full packet bytes unchanged.

### Chunk manifest transparency

`get()` auto-reassembles chunk manifests and returns rendered content.
Use `envelope()` for manifest-level response inspection.

## `window.resolve(input)`

`window.resolve(input)` applies the browser's built-in HPPR source resolution
policy and returns a dedicated resolve result.

It is the high-level source API for browser-owned document resolution.
It is not a Fetch response and it does not expose fetch-style options.
It resolves relative input against the current document URL.

Result fields:

- `packet`: resolved `HpprPacket`
- `endpoint`: selected endpoint string
- `signer`: signer identity string used to access the repo when routed access is
  used
- `contentAuthority`: resolved content-authority signer for the document, or `null`
- `isRepo`: whether the resolved source came from the home repo path

`isRepo` is `true` for browser-home-selected sources such as `repo` and other
non-route-backed home-repo resolution paths.

For app-content URLs, `contentAuthority` comes from the app content pointer's
`Content-Authority`.
For direct sealed content, `contentAuthority` comes from packet `Seal-By`.
For unsigned content, it is `null`.

`signer` and `contentAuthority` are distinct:

- `signer` identifies the route or repo capability used for access
- `contentAuthority` identifies the signer that authorized the resolved content

`window.resolve()` is document resolve only.
Listing stays on `window.home.list()` or `window.route.list()`.

## Relative resolution

HTML attributes use standard RFC 3986 resolution (href mode).

## Document.packet

For `hppr://` documents, `document.packet` returns the source `HpprPacket`.
For non-HPPR pages, it returns `null`.

`document.URC` is the exact HPPR packet identity surface.
It is derived from the loaded packet, not from the navigable URL.
When the loaded packet is a Plex or Seal with coordinate identity, it returns the
full exact versioned coordinate including hash. This includes direct-hash HPPR
pages that loaded a Plex or Seal packet. Packetless pages, helper schemes, file
pages, and Blob-only pages return `null`.

`document.URL` is a native legacy projected document URL.
For HPPR-backed documents it strips `/|/...` exact selectors and JSONqa state.
For direct-hash HPPR documents that loaded a Plex or Seal packet, it projects to
`hppr://<group>/<app>/<location>` from the loaded packet.
For file documents it returns the stripped file URL without JSONqa view state.
For helper documents such as `havi:///overview`, it returns the helper document
URL unchanged. On HPPR and file documents it warns on first access.

`document.documentURI` mirrors the same projected value as `document.URL` but
never warns.

Loaded HPPR documents also carry browser metadata for the resolved content
signer.
For app-content URLs this metadata comes from `Content-Authority`.
For direct sealed content it comes from `Seal-By`.
This content-authority metadata is distinct from the route signer or home-repo
signer used to access the repo.

## HpprPacket

Packet fields include:

- identity: `hash`, `type`, `dataLength`
- coordinate: `group`, `app`, `location`, `tai`, `coordinate`
- signature: `sealBy`
- header APIs: `getHeader`, `getHeaders`, `headers`, `customHeaders`
- data APIs: `arrayBuffer`, `blob`, `text`, `json`, `raw`

## URC and Address

`URC` models coordinate syntax.

`Address` is a detached parsed HPPR-family value object.
`new Address(...)` parses an exact HPPR-family address and does not navigate the
current page. It exposes:

- `scheme`
- `href`
- `coordinate`
- `urc`
- `group`
- `app`
- `location`
- `qa`
- `fragment`
- `isListing`

Exact explicit routing syntax remains in JSONqa.
For `hppr://...{via:...}`, use `address.qa['via']`.
`via` still follows HPPR via syntax from `../../hppr/spec/031-VIA-SYNTAX.md`.
Browser-defined shorthands such as `repo` remain HAVI address-layer values,
not HPPR core via syntax.

`window.address` is the browser-owned live exact address API.
Setting `window.address = url` or `window.address.href = url` navigates
(`PutForwards=href`).
It is a stable `[SameObject]` live view over the current exact address state.
Same-document updates keep the same object and update it in place.

Shared live fields:

- `scheme`
- `href`
- `qa`
- `fragment`
- `isListing`

HPPR live subtype fields:

- `coordinate`
- `urc`
- `group`
- `app`
- `location`

File live subtype field:

- `pathname`

On helper documents such as `havi:///overview`, `window.address` returns the
shared live exact-address surface with helper `href` and `scheme`, `qa === null`,
and `document.URC === null`.
On unsupported non-HAVI pages, `window.address` is `null`.

HAVI also installs a `window.location` and `document.location` compatibility
shim on `hppr://` and `file://` pages.
The shim logs a warning on first use and projects common web fields onto HAVI
state. It derives compatibility state from `window.address`, not from
`document.URL`:

- `hash` maps to JSONqa fragment (`{#:...}`)
- `search` maps to projected top-level JSONqa key/value pairs
- `pathname` maps to `/<app>/<location>`
- `origin` projects as `scheme://<group>` on `hppr://`

Compatibility input is strict.
`window.location` does not accept raw JSONqa syntax.
Pages that need exact HAVI semantics use `window.address`.

`qa` and `fragment` exist on `URC` and are delegated through detached
`Address`. `window.address.urc` is a live `[SameObject]` view on HPPR pages.
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

`<x watch>` elements may share WatchSocket connections through a per-document
pool keyed by watch prefix. See `070-X-ELEMENT.md`.

## StreamPub

Live publisher API.

`streamPub(prefix, options)` is payload-oriented.
Callers write payload bytes.
The client signs and frames those bytes into trailer-format Seal packets
internally before sending them to the repo.

Key options:

- `key` (required)
- `headers`
- `maxSegmentSize`
- `flushSeq`

Calls without `key` are invalid.
HAVI does not expose raw trailer passthrough as part of the normal JS stream API.

Methods:

- `write(bytes)`
- `flush()`
- `close()`

`write()` resolves when bytes are queued, not when TCP flush completes.

### Incremental byte-stream semantics

StreamPub and StreamSub are transparent byte pipes for the primary media path.
Applications are expected to frame payloads at the application layer (for
example, length-prefixed chunks) and parse incrementally from `streamSub()`.

`onpacket` is optional and is not required for media playback.
When used, packet events carry parsed `HpprPacket` objects.

## StreamSub

Live subscriber API.

`streamSub(prefix)` is payload-oriented.
The repo still relays trailer-format bytes on the wire.
The client parses them internally and:

- `stream` yields payload bytes
- optional `onpacket` receives parsed completed `HpprPacket` objects from the
  same parser state that produced those payload bytes

Lifecycle events:

- `onopen`
- `onerror`
- `onclose`

Byte delivery is incremental and order-preserving for the received payload
stream.

## Media APIs

When exposed, browser media APIs follow the media policy in `080-MEDIA.md`.
Implementation status and HAVI-specific rollout details belong in
`../reference.md` or other reference docs.

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

Available on `window.ring0.repo` when privileged repo integration is exposed.

Methods:

- `port()`
- `repoPath()`
- `status()` runtime/backend string

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
