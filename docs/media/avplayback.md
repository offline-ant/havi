# HAVI A/V playback

This document covers current `HTMLMediaElement` playback behavior in HAVI.

## Architecture split

HAVI keeps ordinary playback and custom MSE playback as different systems.
Do not merge them in API or implementation design.

- Native playback path: for ordinary baked audio/video.
- Custom MSE path: for `MediaSource` append workflows.

See `havi/spec/080-MEDIA.md` for the normative split.

## Native playback path

Use the native path for ordinary `<audio src>` / `<video src>` playback.

Responsibilities:

- browser side resolves the source and applies HPPR policy
- platform/native media code owns decode, playout, and timing
- HAVI shell presents the resulting audio/video through the native player path

This is the right path for normal baked media files.
It is not a fallback form of MSE.

## Custom MSE path

`MediaSource` playback uses the controller operations in
`components/media/media-thread/controller.rs`:

- `PrepareMsePlayback`
- `MseAddSourceBuffer`
- `MseAppendData`
- `MseRemove`
- `MseEndOfStream`
- track-selection updates and parse/decode events

This path owns append semantics itself.
It is used where native delegated playback cannot express the required
buffering behavior.

## Source-backed browser/media handoff

The browser still owns source resolution before playback starts:

- route/auth resolution
- chunk-manifest handling
- byte-range reads from resolved assets

The media layer receives a resolved asset boundary, not a second network stack.
This keeps transport policy in browser code and playback policy in media code.

## Video policy

Current HAVI video policy matches `havi/spec/080-MEDIA.md`.

Supported video containers/codecs:

- `video/mp4`
- `video/x-m4v`
- AV1 video (`av01`)
- H.264 video (`avc1`, `avc3`)

Rejected video families include:

- WebM
- Ogg
- Matroska
- H.265 / HEVC
- VP8 / VP9

`canPlayType()` policy and codec parsing live in
`components/media/media-thread/controller.rs`.

Audio format support remains delegated to platform capability.

## Presentation path

HAVI does not render video through WebRender.
Current high-level flow is:

```text
HTMLMediaElement / MediaSource
  -> media controller
  -> havishell media bridge
  -> texture or native presentation surface
  -> Makepad/compositor frame composition
```

The exact retained composition rules live in `havi/RENDERER.md`.
The important constraint here is architectural: media output enters the active
Makepad/compositor renderer, not a parallel legacy renderer.

## What this document does not claim

- It does not claim WebRTC support.
- It does not claim generic media-capture support.
- It does not claim WebRender output.

## Relevant tests

- `havi/tests/havi/media-policy-test.sh`
- `havi/tests/havi/media-source-basic-test.sh`
- `havi/tests/havi/media-source-playback-test.sh`
- `havi/tests/havi/media-baked-hppr-test.sh`
