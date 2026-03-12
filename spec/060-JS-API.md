# JavaScript API

HAVI exposes HPPR APIs on `window`.

`window.home` and `window.route` are explicit low-level repo clients.
`window.resolve(input)` is the browser-owned high-level source resolver.

## Window globals

- `window.address`: current HPPR address object
- `window.home`: home repo client (always available)
- `window.route`: route repo client (nullable)
- `window.resolve(input)`: browser-owned document source resolver
- `window.ring0`: admin client (privileged schemes only)
- `window.H3`: crypto namespace (always available)

`window.route` is `null` when no route exists or no matching route auth key is
available.

For `hppr://` routed pages, route/deploy resolution happens before page JS runs.
If deploy metadata is missing or invalid, navigation fails with `HpprError` from
handler operations.

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

### connect() identity parameter

`connect()` accepts an optional identity string following `Signer::parse()`
format:

- omitted, `""`, or `"anyone"`: anyone
- `ring1:<name>#<password>`: Ring1 password-derived key
- `ring1:<name>#&.<b64a>.H3`: Ring1 explicit key
- `ring2:<group>#&.<b64a>.H3`: Ring2 explicit key
- `ring2:<group>/<user>#<password>`: Ring2 adhoc key

Invalid identity strings reject the returned promise with a TypeError.

### connectRing2Password()

`connectRing2Password(endpoint, group, username, password)` creates a remote
client with a Ring2 adhoc signer without requiring the caller to assemble a
signer string manually.

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

`window.resolve(input)` applies HAVI's built-in browser resolution policy and
returns a dedicated resolve result.

It is the high-level source API for browser-owned document resolution.
It is not a Fetch response and it does not expose fetch-style options.
It resolves relative input against the current document URL.

Result fields:

- `packet`: resolved `HpprPacket`
- `endpoint`: selected endpoint string
- `signer`: signer identity string when routed access is used
- `isRepo`: whether the resolved source came from the home repo path

`window.resolve()` is document resolve only in this pass.
Listing stays on `window.home.list()` or `window.route.list()`.

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

`window.address` is the exact HAVI address API.
Setting `window.address = url` or `window.address.href = url` navigates
(`PutForwards=href`).
Setting `scheme`, `endpoint`, `group`, `app`, or `location` recomputes the full
URL and navigates.

HAVI also installs a `window.location` and `document.location` compatibility
shim on `hppr*://` and `file://` pages.
The shim logs a warning on first use and projects common web fields onto HAVI
state:

- `hash` maps to JSONqa fragment (`{#:...}`)
- `search` maps to projected top-level JSONqa key/value pairs
- `pathname` maps to `/<app>/<location>`
- `origin` projects as `scheme://<group>` on `hppr*://`

Compatibility input is strict.
`window.location` does not accept raw JSONqa syntax.
Pages that need exact HAVI semantics use `window.address`.

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

## MediaRecorder and MediaSource status

`MediaRecorder` is exposed in HAVI with a strict part-2 camera path.

Available API surface:

- constructor: `new MediaRecorder(stream, options)`
- static: `MediaRecorder.isTypeSupported(mimeType)`
- attributes: `state`, `mimeType`, `stream`
- event handlers: `onstart`, `onstop`, `ondataavailable`, `onerror`
- methods: `start(timeslice?)`, `stop()`, `pause()`, `resume()`, `requestData()`

Part-2 behavior:

- constructor validates options and stores recorder state
- `isTypeSupported()` follows HAVI media policy checks
- `start(timeslice)` starts camera-backed AV1 encode and emits periodic
  `dataavailable` chunks (`Blob` payload in `event.data`)
- `stop()` emits a final chunk when available, then `stop`

Current supported execution path:

- exactly one live camera video track
- AV1-in-MP4 media policy mime (`video/mp4` + AV1 codecs)

Not-yet-implemented execution paths throw DOMException with messages containing
`NotYetImplemented` (NotSupportedError or InvalidStateError), including
audio-only streams, mixed audio/video streams, and explicit
pause/resume/requestData control paths.

`MediaSource` is now exposed on the append-backed MSE playback path.

Available API surface:

- constructor: `new MediaSource()`
- static: `MediaSource.isTypeSupported(mimeType)`
- attributes: `readyState`, `duration`, `sourceBuffers`, `activeSourceBuffers`
- methods: `addSourceBuffer(mimeType)`, `removeSourceBuffer(sourceBuffer)`,
  `endOfStream()`
- object URLs: `URL.createObjectURL(mediaSource)`, `URL.revokeObjectURL(url)`
- events: `sourceopen`, `sourceended`, `sourceclose`

`SourceBuffer` is exposed with:

- attributes: `updating`, `buffered`
- methods: `appendBuffer(data)`, `remove(start, end)`, `abort()`
- events: `updatestart`, `update`, `updateend`, `error`

Current attach/state model:

- supported attach path today is `video.src = URL.createObjectURL(mediaSource)`
- object-URL attach/detach is explicit in `MediaSource` ownership
- the code keeps the same `MediaSource` attach/detach state machine ready for a
  future `srcObject = mediaSource` path, but that surface is not exposed yet
- detached `SourceBuffer`s stay registered in `sourceBuffers` and leave
  `activeSourceBuffers` until reattached
- removed `SourceBuffer`s are explicitly invalid and reject further operations

Current limits:

- one attached `SourceBuffer` per `MediaSource`
- valid single-buffer MP4/fMP4 append can reach metadata, buffered ranges,
  and decode/present through HAVI's MSE playback session path
- `activeSourceBuffers` is still a temporary attached-buffer view, not final
  track-driven multi-buffer selection
- append/remove stay limited to MP4/fMP4 custom playback

Direct source-backed playback remains separate from MSE. For chunked recorder
playback, pages can use `MediaSource` through the append-backed path. Blob
handoff remains a page-level fallback.

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
- `status()` runtime/backend string (`external`, `self_exec`, or `inline`)
  - `external`: external-process runtime path
  - `self_exec`: pylon self-exec process runtime path
  - `inline`: in-process runtime path

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
