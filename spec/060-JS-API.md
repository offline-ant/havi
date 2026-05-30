# JavaScript API

HAVI exposes HPPR APIs on `window`.

`window.source` is the browser-owned ambient committed-source descriptor.
`window.resolve(input)` is the browser-owned high-level source resolver.

## Window globals

- `window.address`: current exact HAVI address object
- `window.source`: committed source descriptor for ordinary repo-backed pages
- `window.resolve(input)`: browser-owned document source resolver
- `window.H3`: crypto namespace (always available)

`window.source` is `null` on `file://` pages, helper pages, and non-HPPR pages.
For ordinary repo-backed pages it exposes exactly:

- `client`
- `authority`
- `kind`

`kind` is `"repo"` or `"remote"`.

Ordinary pages no longer expose `window.home`, `window.route`, or
`window.ring0`.

`window.source` is `null` on helper pages, `file://` pages, and non-HPPR pages.
Internal helper behavior is page-owned. Surviving `havi://` pages use their own
`fetch('havi:///.../api?...')` endpoints instead of a generic privileged JS
object.

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

## HpprSource

Ambient committed-source descriptor for the current document.

```webidl
[Exposed=Window, Pref="dom_hppr_enabled"]
interface HpprSource {
    readonly attribute HpprClient client;
    readonly attribute DOMString kind;
    readonly attribute DOMString? authority;
};
```

`client` is truthful for both committed remote sources and committed repo
sources. Repo-backed sources use a browser-owned local backend path, not fake
endpoint/signing metadata.

Current limitations:

- helper pages and `file://` pages get `window.source === null`
- local committed-source watch/stream paths are not exposed through a fake
  transport client; unsupported operations fail explicitly until the later
  browser-local facade grows them honestly

## HpprClient

Primary ordinary-page interface for HPPR commands.

```webidl
[Exposed=Window, Pref="dom_hppr_enabled"]
interface HpprClient {
    [NewObject, Throws] static Promise<HpprClient> named(DOMString name);

    [NewObject] EnvelopeHpprClient envelope();

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
    Promise<HpprGreeting> hello();
};
```

`HpprClient` is the truthful common surface.
It does not expose raw arbitrary connect, fake endpoint text, or transport-only
live primitives.

### named()

`named(name)` is the first explicit extra-capability path beyond
`window.source`.
It asks HAVI for a browser-mediated named client identified by stable
user-visible name.
The requesting page origin must already have a grant for that named client.
Otherwise the returned promise rejects.

The returned value is a normal `HpprClient`, but it is backed by a browser-owned
named-client backend instead of raw page-provided endpoint/signer material.
The page does not receive stored secrets.
Raw transport connect remains a separate privileged helper mechanism and is not
part of the ordinary named-client flow.

Common return types:

- `get()`: `HpprPacket`
- `list()/tips()/headers()/members()`: `string[]`
- `store()/add()`: `string[]` of stored hashes
- `detach()`: `void`
- `hello()`: `HpprGreeting`

`add()` sends headers/data and the repo builds packet layers.
`store()` sends full packet bytes unchanged.

### Chunk manifest transparency

`get()` auto-reassembles chunk manifests and returns rendered content.
Use `envelope()` for manifest-level response inspection and transport-oriented
operations.

## `window.resolve(input)`

`window.resolve(input)` applies the browser's built-in HPPR source resolution
policy and returns a dedicated resolve result.

It is the high-level source API for browser-owned document resolution.
It is not a Fetch response and it does not expose fetch-style options.
It resolves relative input against the current document URL.

Result fields:

- `packet`: resolved `HpprPacket`
- `kind`: resolved source kind (`"repo"` or `"remote"`)
- `contentAuthority`: resolved content-authority signer for the document, or `null`

For API-content URLs, `contentAuthority` comes from the API content pointer's
`Content-Authority`.
For direct sealed content, `contentAuthority` comes from packet `Seal-By`.
For unsigned content, it is `null`.

`kind` and `contentAuthority` are distinct:

- `kind` identifies whether browser-owned resolution selected the repo-backed or
  remote-backed source path
- `contentAuthority` identifies the signer that authorized the resolved content

`window.resolve()` is document resolve only.
Listing stays on `window.source.client.list()` for ordinary repo-backed pages.

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
`hppr://<group>/<api>//<key>` from the loaded packet.
For file documents it returns the stripped file URL without JSONqa view state.
For helper documents such as `havi:///diagnostics`, it returns the helper
document URL unchanged. On HPPR and file documents it warns on first access.

`document.documentURI` mirrors the same projected value as `document.URL` but
never warns.

Loaded HPPR documents also carry browser metadata for the resolved content
signer.
For API-content URLs this metadata comes from `Content-Authority`.
For direct sealed content it comes from `Seal-By`.
This content-authority metadata is distinct from any browser-local access
signer or routed request signer used to access the repo.

## HpprPacket

Packet fields include:

- identity: `hash`, `type`, `dataLength`
- coordinate: `group`, `api`, `key`, `tai`, `coordinate`
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
- `api`
- `key`
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
- `api`
- `key`

File live subtype field:

- `pathname`

On helper documents such as `havi:///diagnostics`, `window.address` returns the
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
- `pathname` maps to `/<api>//<key>`
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

## EnvelopeHpprClient

Transport-oriented wrapper around `HpprClient`.
It keeps raw remote connect plus the live transport primitives that are not
truthfully universal across committed repo sources and named-client backends.

```webidl
[Exposed=Window, Pref="dom_hppr_enabled"]
interface EnvelopeHpprClient {
    [NewObject, Throws] static Promise<EnvelopeHpprClient> connect(
        DOMString endpoint,
        optional DOMString identity
    );

    [NewObject] HpprClient unpack();

    readonly attribute DOMString? endpoint;
    readonly attribute DOMString? account;
    readonly attribute DOMString? group;

    Promise<HpprResult> get(USVString urc);
    Promise<HpprResult> list(USVString urc);
    Promise<HpprResult> headers(USVString urc);
    Promise<HpprResult> tips(USVString urc);
    Promise<HpprResult> members(USVString urc);
    Promise<HpprResult> store(HpprPacket packet);
    Promise<HpprResult> detach(DOMString hash);
    Promise<HpprResult> add(optional HpprAddOptions options = {});
    Promise<HpprResult> hello();
    WatchSocket watch(USVString urc);
    StreamPub streamPub(USVString prefix, optional StreamPubOptions options = {});
    StreamSub streamSub(USVString prefix);
};
```

### connect()

`EnvelopeHpprClient.connect(endpoint, identity?)` creates a remote transport
client.
`endpoint` uses HPPR via syntax from `../../hppr/spec/031-VIA-SYNTAX.md`.
Examples include `host`, `quib+host:4776`, `ws+host`, and
`unix+/absolute/path`.

`identity` follows the HPPRD identity text grammar:

- omitted, `""`, or `"anyone"`: anyone
- `ring1:<name>|<password>`: Ring1 password-derived key
- `ring1:<name>|&.<b64a>.H3`: Ring1 explicit key
- `ring2:<group>|&.<b64a>.H3`: Ring2 explicit key
- `ring2:<group>/<user>|<password>`: Ring2 adhoc key
- `ring2:/<user>|<password>`: Ring2 contextual adhoc key

Invalid identity strings reject the returned promise with a TypeError.
Raw connect is helper/privileged-only. Ordinary pages do not use it as their
ambient capability story.

`endpoint` is nullable because only true remote transport clients have a real
endpoint string. Committed repo-backed sources and browser-mediated named
clients expose `null` there.

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
