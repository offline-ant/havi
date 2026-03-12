# Video chat status (2026-03-06)

## Current status

HAVI now ships:

- `getUserMedia()` camera acquisition
- `video.srcObject` local preview
- `StreamPub` / `StreamSub` incremental byte transport
- `MediaRecorder` part-2 camera chunk path
- `video-chat.html` explicit sender contract with remote playback path:
  recorder sender when supported, NYI receive-only fallback when not

## MediaRecorder part-2 scope

Implemented:

- constructor + options validation
- `MediaRecorder.isTypeSupported()` via HAVI media policy
- `state`, `mimeType`, `stream`
- `onstart`, `onstop`, `ondataavailable`, `onerror`
- `start(timeslice)` camera-backed periodic chunk emission
- `stop()` final chunk (when available) then stop event

Current supported path:

- one live camera video track
- AV1-in-MP4 mime policy

Pending:

- audio-only recorder
- mixed audio/video recorder
- pause/resume/requestData explicit control path

Pending paths throw NotSupportedError / InvalidStateError with
`NotYetImplemented` in the message.

## Chat page contract (`/video-chat.html`)

Sender branch behavior is explicit:

- `MediaRecorder.start(200)` succeeds: page runs normal sender + receiver flow.
- `MediaRecorder.start(200)` throws NYI for non-encoder-friendly camera format:
  page enters receive-only mode, shows a clear user-visible status, and remains
  responsive (no hanging start state).

Receiver byte-stream framing is always active.

Playback path selection is explicit:

- `MediaSource` / `SourceBuffer` is now available for the primary receiver path
- pages can append framed MP4 chunks through `SourceBuffer`
- Blob handoff remains a page-level fallback strategy when a page chooses not
  to use MSE

If neither path is available, page stays receive-only and reports
`NotYetImplemented` instead of hanging.

## Current MSE scope

- `new MediaSource()`
- `URL.createObjectURL(mediaSource)`
- one `addSourceBuffer()` per source
- `appendBuffer()` / `remove()` / `abort()` / `endOfStream()`
- valid single-buffer fMP4 receiver playback through the shared custom session
  path below the browser transport boundary
