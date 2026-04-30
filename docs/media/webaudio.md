# HAVI WebAudio

This document covers the current WebAudio implementation surface in
`havi/components/media/audio/`.

## Scope

WebAudio is its own audio-graph system.
It is not the same path as native `HTMLMediaElement` playback, and it is not
part of HAVI's custom MSE append pipeline.

Current implementation anchors:

- `audio/context.rs`
- `audio/render_thread.rs`
- `audio/node.rs`
- backend sink integration under `components/media/backends/`

## Execution model

The code follows the usual WebAudio split between:

- control-thread work: graph creation and author-driven mutations
- render-thread work: block-by-block audio processing

The render thread processes 128-sample quanta and pushes mixed output to an
`AudioSink`.

Supported context families in code:

- real-time audio contexts
- offline audio contexts

## Relationship to the rest of HAVI media

Keep these boundaries clear:

- WebAudio is an audio-graph engine.
- Native HTML media playback delegates ordinary baked playback to native media
  control.
- Custom MSE owns append/decode/timing for append-buffer playback.

These systems may meet at audio output, but they are not one shared playback
abstraction.

## What this document does not claim

- no WebRTC support claim
- no Media Capture support claim
- no WebRender audio/video integration claim

## Pointers

- Normative media policy: `havi/spec/080-MEDIA.md`
- Renderer ownership: `havi/RENDERER.md`
- Media controller boundary: `havi/components/media/media-thread/controller.rs`
