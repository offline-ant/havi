# HAVI media overview

These documents describe current HAVI media behavior.
They do not describe generic upstream Servo media architecture.

## Source of truth

- Spec: `havi/spec/080-MEDIA.md`
- Renderer path: `havi/RENDERER.md`
- Runtime boundary: `havi/components/media/media-thread/controller.rs`

## Two playback paths

HAVI keeps two media paths separate.

### 1. Native playback path

Ordinary baked audio/video playback delegates media control to the platform
player/widget path.

Use this path for normal `<audio>` / `<video>` playback where the platform can
already provide decode, playout, and timing.

Rules:

- browser code resolves the source and policy
- platform/native playback owns decode and timing
- HPPR-specific routing does not leak into platform backends
- this path stays separate from custom append-buffer playback

### 2. Custom MSE path

`MediaSource` / `SourceBuffer` use HAVI's custom append path.

This path owns:

- append-buffer parsing
- init/media segment handling
- decode orchestration
- buffered-range tracking
- playout state and timing
- track selection events back to script

Use this path only when native delegated playback cannot provide the required
behavior.

## Browser/media boundary

The browser owns transport and resolution work:

- HPPR route and auth policy
- committed-source resolution
- chunk-manifest traversal
- byte-range access to resolved media assets

The media layer receives resolved assets and playback commands.
It does not own HPPR routing.
It does not fetch through an HTTP loopback shim.

## Renderer integration

HAVI does not use WebRender.
Video presentation flows through the active Makepad/compositor renderer stack.
At a high level:

```text
HTMLMediaElement / MediaSource
  -> media controller (`components/media/media-thread/controller.rs`)
  -> havishell media bridge
  -> Makepad texture / widget presentation
  -> HAVI retained renderer + compositor
```

For full renderer ownership and composition rules, see `havi/RENDERER.md`.

## Current limits

- No WebRTC claim.
- No generic Media Capture claim in these docs.
- No WebRender claim.
- Video policy is the one in `080-MEDIA.md`: MP4 container, AV1/H.264 video.
- Audio format support depends on platform-native capabilities.

## Relevant tests

- `havi/tests/havi/media-policy-test.sh`
- `havi/tests/havi/media-source-basic-test.sh`
- `havi/tests/havi/media-source-playback-test.sh`
- `havi/tests/havi/media-baked-hppr-test.sh`
- `havi/tests/havi/mediarecorder-part1-test.sh`
- `havi/tests/havi/mediarecorder-part2-test.sh`
