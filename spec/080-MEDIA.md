# Media

HAVI media support policy.

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

`MediaSource` is still not exposed.

Remote playback for stream-delivered recorder chunks can use standard
`<video>.srcObject = Blob` as a non-MSE runtime path.

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

Real-time communication uses HPPR StreamIn/StreamOut APIs instead.
See `060-JS-API.md` for StreamIn and StreamOut.
