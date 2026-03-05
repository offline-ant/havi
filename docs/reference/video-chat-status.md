# Video chat status (2026-03-05)

## Current status

HAVI now ships:

- `getUserMedia()` camera acquisition
- `video.srcObject` local preview
- `StreamIn` / `StreamOut` incremental byte transport
- `MediaRecorder` part-1 DOM surface (state + lifecycle scaffolding)

## MediaRecorder part-1 scope

Implemented:

- constructor + options validation
- `MediaRecorder.isTypeSupported()` via HAVI media policy
- `state`, `mimeType`, `stream`
- `onstart`, `onstop`, `ondataavailable`, `onerror`
- `start()` / `stop()` state transitions and event dispatch scaffolding

Pending:

- encoder-backed chunk emission (`dataavailable` payload path)
- `pause()` / `resume()` / `requestData()` execution path

Pending paths throw NotSupportedError / InvalidStateError with
`NotYetImplemented` in the message.

## Still not exposed

- `MediaSource` / `SourceBuffer`

## Next vertical slice

- Real recorder chunk production
- Real MSE append playback path
