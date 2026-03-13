# Media

HAVI media support policy.

## Architecture split

HAVI keeps browser transport and playback core responsibilities separate.

Browser-owned transport stays in browser code:

- shared source resolution through browser resolver logic
- HPPR route and auth policy
- chunk-manifest detection and traversal
- source-reference-based byte reads
- media asset resolution into a byte source

Shared playback contracts live at the browser/media boundary:

- page loading and media loading use the same browser resolver module
- resolved media is handed to the media layer as a resolved asset with metadata
  and a random-access byte source
- the byte source contract is a byte-range contract, not a URL contract
- reads happen off the script thread on playback/session workers

Playback code stays below that boundary and is split into two ingress models:

- direct source-backed playback over a resolved media asset
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
No HTTP loopback relay is part of the architecture.

## Video policy

HAVI supports MP4 video with these video codecs:

- AV1
- H.264

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

Any unsupported video codec in the codecs list causes rejection regardless of
container.

### Source element type filtering

`<source type="...">` attributes are checked against the same policy.
A source with `type="video/webm"` is skipped during resource selection.

### Runtime behavior

When a video source is loaded that the platform cannot decode, the media element
fires an `error` event with `MEDIA_ERR_SRC_NOT_SUPPORTED` or
`MEDIA_ERR_DECODE`.

## Audio policy

Audio types delegate to platform capabilities. Common supported formats:

- Opus
- FLAC
- AAC
- Vorbis
- MP3

Audio codec support varies by platform and installed decoders.

## Media APIs

When media APIs are exposed to page code:

- `MediaRecorder` records according to browser media policy
- `MediaSource` / `SourceBuffer` provide append-backed playback
- direct source-backed playback remains separate from MSE

Exact implementation status, rollout phase, and shell-specific constraints are
reference material, not media policy.

## Rationale

AV1 is royalty-free, has strong compression, and fits the target-platform media
strategy. MP4 is the standard container for AV1 distribution.

Restricting supported formats keeps testing and playback behavior predictable.

## WebRTC

HAVI does not implement WebRTC (`RTCPeerConnection`, `RTCDataChannel`, etc.).

Real-time communication uses HPPR `StreamPub` and `StreamSub` APIs instead.
See `060-JS-API.md`.
