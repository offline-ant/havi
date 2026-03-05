# Video chat status (2026-03-05)

## Current status

HAVI now ships:

- `getUserMedia()` camera acquisition
- `video.srcObject` local preview
- `StreamIn` / `StreamOut` incremental byte transport
- `MediaRecorder` part-2 camera chunk path

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

## Still not exposed

- `MediaSource` / `SourceBuffer`
