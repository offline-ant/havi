# Video chat implementation decision (2026-03-05)

## Decision

Ship the **byte-stream chat path vertical slice** now. Do not ship a fake
MediaRecorder/MediaSource surface.

## Why

Current runtime has:

- `getUserMedia()` camera acquisition
- `video.srcObject` camera preview
- `StreamIn`/`StreamOut` incremental byte transport

Current runtime does not have:

- `MediaRecorder` DOM interface or encoder plumbing for chunk emission
- `MediaSource` / `SourceBuffer` DOM interfaces for append-stream playback

Implementing those APIs without real encode/decode plumbing would create a
misleading contract and unstable behavior.

## Shipped scope in this change

1. StreamOut forwards incremental bytes immediately; packet-finalization events
   are not on the media hot path.
2. JS API/media specs now match runtime truth:
   - transparent incremental StreamIn/StreamOut bytes
   - recorder/MSE explicitly not exposed yet
3. Focused path test validates the shippable contract:
   - `getUserMedia` + `srcObject`
   - framed bytes over StreamIn/StreamOut with split/coalesce recovery

## Deferred for next vertical slice

- Real `MediaRecorder` subset backed by encoder output
- Real `MediaSource`/`SourceBuffer` append path for remote `<video>` playback
