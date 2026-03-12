# Media

HAVI media support policy.

## Architecture split

HAVI keeps browser transport and playback core responsibilities separate.

Browser-owned transport stays in HAVI code:

- shared source resolution through `havi-protocols::resolve`
- HPPR route and auth policy
- chunk-manifest detection and traversal
- source-reference-based byte reads
- media asset resolution into a byte source

Shared playback contracts live at the HAVI/media boundary:

- page loading and media loading use the same browser resolver module
- resolved media is handed to the media layer as a resolved asset with metadata
  and a blocking random-access `MediaByteSource`
- `MediaByteSource` is a byte-range contract, not a URL contract
- reads happen off the script thread on playback/session workers through the
  shared resolve `ReadBytes` path

Playback code stays below that boundary and is split into two ingress models:

- direct source-backed playback over `ResolvedMediaAsset`
  - container probing
  - indexing and seek mapping
  - byte-region caching
  - demux/decode progression
- MSE append playback over `MediaSource` / `SourceBuffer`
  - append/remove/eos handling
  - append-time demux/decode progression
- shared lower playback responsibilities
  - PCM playout
  - clocking
  - scheduling
  - media-session policy

Platform-native delegated playback remains a separate path for ordinary native
URL/file sources. HPPR-specific concerns do not cross into platform backends.
No HTTP loopback relay is part of the final architecture.

## Video

HAVI supports two video codecs in MP4 containers: AV1 and H.264.

Rejected formats:

- VP8, VP9, H.265 codecs
- WebM, Ogg, Matroska containers

### canPlayType

`HTMLMediaElement.canPlayType()` returns:

| Type | Result |
|------|--------|
| `video/mp4` | `maybe` |
| `video/mp4; codecs="av01..."` | `probably` |
| `video/mp4; codecs="av01..., opus"` | `probably` |
| `video/mp4; codecs="av01..., mp4a..."` | `probably` |
| `video/mp4; codecs="avc1..."` | `probably` |
| `video/mp4; codecs="avc1..., mp4a..."` | `probably` |
| `video/mp4; codecs="hev1..."` | (empty) |
| `video/webm` | (empty) |
| `video/webm; codecs="vp8"` | (empty) |
| `video/webm; codecs="vp9"` | (empty) |
| `video/webm; codecs="av01..."` | (empty) |
| `video/ogg` | (empty) |

Bare `video/mp4` returns `maybe` because the container may hold any codec.
With an explicit `av01` or `avc1` codec, the result is `probably`.

Any unsupported video codec (VP8, VP9, H.265) in the codecs list causes
rejection regardless of container.

### Source element type filtering

`<source type="...">` attributes are checked against the same policy. A
source with `type="video/webm"` is skipped during resource selection.

### Runtime behavior

When a video source is loaded that the platform cannot decode (e.g. a
VP9 stream inside an MP4), the media element fires an `error` event
with `MEDIA_ERR_SRC_NOT_SUPPORTED` or `MEDIA_ERR_DECODE`.

## Recorder/MSE status

`MediaRecorder` is exposed with a strict part-2 camera path.

Implemented in part 2:

- constructor + option validation
- `MediaRecorder.isTypeSupported()` wired to HAVI media policy checks
- `state`/`mimeType`/`stream` attributes
- camera-backed `start(timeslice)` periodic chunk generation
- `dataavailable` events carrying Blob chunks (`event.data`)
- `stop()` final-chunk + `stop` ordering

Not implemented in part 2:

- audio-only recorder path
- mixed audio/video recorder path
- pause/resume/requestData control path

Not-yet-implemented paths throw NotSupportedError or InvalidStateError with
`NotYetImplemented` in the error message.

`MediaSource` and `SourceBuffer` are exposed on the append-backed MSE path.
This stays separate from native delegated playback and from direct
source-backed playback.

Current MSE scope:

- `new MediaSource()`
- `URL.createObjectURL(mediaSource)`
- `HTMLMediaElement.srcObject = mediaSource`
- `readyState`
- `duration`
- `sourceBuffers` / `activeSourceBuffers`
- `MediaSource.isTypeSupported()` for HAVI MP4 policy
- multiple `addSourceBuffer()` calls per `MediaSource`
- `removeSourceBuffer()`
- `SourceBuffer.appendBuffer()` / `remove()` / `abort()`
- `endOfStream()`
- `sourceopen` / `sourceended` / `sourceclose`
- `updatestart` / `update` / `updateend` / `error`

Current attach/state model:

- object-URL attachment and `srcObject = mediaSource` attachment are supported
- `MediaSource` owns attach/detach state for both attach paths
- removed `SourceBuffer`s become invalid immediately
- detached-but-still-registered `SourceBuffer`s remain in `sourceBuffers` and
  drop out of `activeSourceBuffers` until reattachment

Current limits:

- one `MediaSource` now owns one playback session with multiple logical
  `SourceBuffer` append inputs beneath it
- append/remove completion and error routing are per input
- each `SourceBuffer` currently allows one in-flight append/remove operation at
  a time; same-buffer overlap is rejected at the DOM surface
- incomplete fMP4 append tails may be completed by later append data on the
  same input
- `HTMLMediaElement` audio/video track selection now feeds MSE session and
  active-buffer coordination for parsed MSE track metadata
- the concrete decode/present path supported today remains one muxed MP4/fMP4
  append input; split audio/video playout is not complete yet
- `activeSourceBuffers` now begins to follow parsed track metadata and current
  DOM track selection, but final multi-track coordination is not complete yet
- append/remove stay limited to MP4/fMP4 custom playback

Remote playback for stream-delivered recorder chunks can use `MediaSource`
through the MSE append path. The older Blob handoff remains a fallback
page-level strategy, not the browser media architecture.

## Audio

Audio types delegate to platform capabilities. Common supported formats:

- Opus (in WebM or MP4)
- FLAC
- AAC (platform-native)
- Vorbis (in Ogg, platform-dependent)
- MP3

Audio codec support varies by platform and installed decoders.

## Rationale

AV1 is royalty-free, has best-in-class compression, and is natively
supported by OS video APIs on all target platforms (macOS 13+, iOS 16+,
Windows 10+, Android 10+, Linux via GStreamer). Bundled dav1d software
fallback covers older devices.

Restricting to one codec simplifies testing and ensures consistent
behavior. MP4 is the standard container for AV1 distribution.

## WebRTC

HAVI does not implement WebRTC (`RTCPeerConnection`, `RTCDataChannel`, etc.).

Real-time communication uses HPPR StreamPub/StreamSub APIs instead.
See `060-JS-API.md` for StreamPub and StreamSub.
