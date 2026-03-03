# Media

HAVI media support policy.

## Video

HAVI supports one video format: AV1 in MP4 containers.

Rejected formats:

- VP8, VP9, H.264, H.265 codecs
- WebM, Ogg, Matroska containers

### canPlayType

`HTMLMediaElement.canPlayType()` returns:

| Type | Result |
|------|--------|
| `video/mp4` | `maybe` |
| `video/mp4; codecs="av01..."` | `probably` |
| `video/mp4; codecs="av01..., opus"` | `probably` |
| `video/mp4; codecs="av01..., mp4a..."` | `probably` |
| `video/mp4; codecs="avc1..."` | (empty) |
| `video/mp4; codecs="hev1..."` | (empty) |
| `video/webm` | (empty) |
| `video/webm; codecs="vp8"` | (empty) |
| `video/webm; codecs="vp9"` | (empty) |
| `video/webm; codecs="av01..."` | (empty) |
| `video/ogg` | (empty) |

Bare `video/mp4` returns `maybe` because the container may or may not hold
AV1 content. With an explicit `av01` codec, the result is `probably`.

Any non-AV1 video codec in the codecs list causes rejection regardless of
container.

### Source element type filtering

`<source type="...">` attributes are checked against the same policy. A
source with `type="video/webm"` is skipped during resource selection.

### Runtime behavior

When a video source is loaded that the platform cannot decode (e.g. a
non-AV1 stream inside an MP4), the media element fires an `error` event
with `MEDIA_ERR_SRC_NOT_SUPPORTED` or `MEDIA_ERR_DECODE`.

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
