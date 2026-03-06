# Media Source Extensions (MSE) Implementation Investigation

Investigation for implementing `window.MediaSource` + `SourceBuffer` in HAVI
to support incremental MP4 chunk playback (AV1/MP4 per HAVI media policy).

## 1. Current Architecture

### 1.1 Data Flow Overview

```
JS / DOM                         media-thread             havishell (Makepad)         Platform backend
─────────────────────────────────────────────────────────────────────────────────────────────────────
HTMLMediaElement                  MediaController          App::drain_video_ops()      GStreamer / AVPlayer /
  ↓ resource_fetch_algorithm()      ↓ send_op(VideoOp)       ↓ cx.prepare_video_*()    MediaFoundation
  → resolve_media_source()        VideoOp channel ───→       ↓ playbin/AVPlayerItem
  → create_media_player()                                    ↓ platform decode
      ↓                          MediaEvent channel ←────  Event::VideoTextureUpdated
      handle_makepad_event()                                Event::VideoPlaybackPrepared
      ↓ fire DOM events                                     etc.
```

### 1.2 Key Files

| Layer | File | Role |
|-------|------|------|
| DOM bindings | `havi/components/script_bindings/webidls/HTMLMediaElement.webidl` | WebIDL for HTMLMediaElement; `MediaProvider` typedef (currently `MediaStream or Blob`) |
| DOM impl | `havi/components/script/dom/html/htmlmediaelement.rs` | 3585 lines. Resource selection, load algorithm, ready state, events |
| DOM media dir | `havi/components/script/dom/media/mod.rs` | Module hub for media DOM types |
| Media controller | `havi/components/media/media-thread/controller.rs` | `MediaController`, `VideoOp`, `MediaEvent`, `MediaSource` (not the MSE type — name collision) |
| Havishell bridge | `havi/ports/havishell/src/app.rs` | `drain_video_ops()`, `handle_video_event()`: translates between `VideoOp`/`MediaEvent` and Makepad `Cx` ops |
| Makepad API | `makepad/platform/src/cx_api.rs` | `prepare_video_playback()`, `begin_video_playback()`, etc. |
| Linux backend | `makepad/platform/src/os/linux/linux_video_playback.rs` | GStreamer `playbin` + `appsink` pipeline |
| macOS backend | `makepad/platform/src/os/apple/apple_video_playback.rs` | AVPlayer + CVMetalTextureCache |
| Windows backend | `makepad/platform/src/os/windows/windows_video_playback.rs` | IMFMediaEngine |
| Software AV1 | `makepad-media/makepad-media/src/software_av1.rs` | dav1d-based MP4 demux → AV1 decode (read-all-at-once) |
| MP4 demux | `makepad-media/makepad-media/src/demux.rs` | Minimal ISOBMFF parser; requires seekable `Read+Seek` |
| GStreamer FFI | `makepad/platform/src/os/linux/gstreamer_sys.rs` | Dynamic dlopen of libgstreamer, libgstapp; `appsink` only — no `appsrc` |
| Media plugin | `makepad/platform/src/media_plugin.rs` | `MediaPlugin` trait, `MediaSoftwareVideoPlayer` trait |
| Video events | `makepad/platform/src/event/video_playback.rs` | `VideoSource`, `VideoPlaybackPreparedEvent`, etc. |

### 1.3 Current Media Source Resolution

`HTMLMediaElement::resolve_media_source()` (line 1890) maps `Resource` to
`media::controller::MediaSource` (an enum with `InMemory`, `Network`,
`Filesystem` variants — unrelated to MSE `MediaSource`). This then becomes
a Makepad `VideoSource` and gets passed to the platform for single-source
playback.

All platform backends expect a complete URI or in-memory blob. There is no
incremental push path. `InMemory` data is written to a temp file on all
platforms.

### 1.4 srcObject Path

`HTMLMediaElement.srcObject` currently accepts `MediaStream | Blob` (the
`/*or MediaSource */` is commented out in the WebIDL typedef on line 8).

`SrcObject` enum (line 134) has `MediaStream` and `Blob` variants. No
`MediaSource` variant exists.

## 2. What Is Missing for MSE

### 2.1 WebIDL Definitions

No WebIDL files exist for:

- `MediaSource.webidl`
- `SourceBuffer.webidl`
- `SourceBufferList.webidl`

### 2.2 DOM Implementation Files

No Rust files exist for:

- `mediasource.rs`
- `sourcebuffer.rs`
- `sourcebufferlist.rs`

### 2.3 Union Type Update

`HTMLMediaElement.webidl` line 8:
```
typedef (MediaStream /*or MediaSource */ or Blob) MediaProvider;
```
`MediaSource` is commented out. The generated Rust union is `MediaStreamOrBlob`.

### 2.4 HTMLMediaElement Integration Points

Specific TODO/missing spots:

| Location | Line | What's missing |
|----------|------|----------------|
| `htmlmediaelement.rs` | 134 | `SrcObject` enum lacks `MediaSource` variant |
| `htmlmediaelement.rs` | 139 | `From<MediaStreamOrBlob>` needs `From<MediaStreamOrMediaSourceOrBlob>` |
| `htmlmediaelement.rs` | 760 | `resource_selection_algorithm_sync` step 6: `src_object` mode doesn't distinguish MediaSource from Blob |
| `htmlmediaelement.rs` | 821 | `load_from_src_object()`: no MediaSource attach logic |
| `htmlmediaelement.rs` | 1224–1250 | `resource_fetch_algorithm` Object path: no MediaSource branch |
| `htmlmediaelement.rs` | 1431 | TODO: "If the media element's assigned media provider object is a MediaSource object, then detach it." |
| `htmlmediaelement.rs` | 1889 | `resolve_media_source()`: no MediaSource → push-pipe conversion |
| `htmlmediaelement.rs` | 2798–2811 | `GetSrcObject`/`SetSrcObject`: union doesn't include MediaSource |

### 2.5 Platform Backend: No Push/Append API

All three platform backends (GStreamer, AVPlayer, IMFMediaEngine) use URL-based
source models. There is no `appsrc` (GStreamer), custom `AVAssetResourceLoader`
(Apple), or `IMFByteStream` push (Windows) support.

The GStreamer FFI (`gstreamer_sys.rs`) loads `libgstapp-1.0.so.0` but only
binds `appsink` functions. No `gst_app_src_*` functions are bound.

The software AV1 player (`SoftwareAv1Player`) reads the entire file at init
and indexes samples via seekable `Read+Seek`. No incremental append.

### 2.6 VideoOp Channel: No Append Operation

`VideoOp` enum (in `controller.rs`) has: `PrepareVideo`, `PrepareAudio`,
`Play`, `Pause`, `Resume`, `Mute`, `Unmute`, `Seek`, `SetVolume`,
`SetPlaybackRate`, `Cleanup`. No `AppendData`, `EndOfStream`, or similar.

### 2.7 MediaEvent Channel: No Buffer Update

`MediaEvent` enum has: `Prepared`, `PositionChanged`, `PlaybackCompleted`,
`Error`, `SeekableRanges`, `BufferedRanges`. No `UpdateEnd`,
`SourceBufferUpdateEnd`, or similar.

## 3. MSE API Surface (Minimum Viable)

Per W3C MSE spec (https://w3c.github.io/media-source/), the minimum API for
incremental MP4 chunk playback:

### 3.1 MediaSource Interface

```webidl
enum ReadyState { "closed", "open", "ended" };
enum EndOfStreamError { "network", "decode" };

[Exposed=Window]
interface MediaSource : EventTarget {
    constructor();
    readonly attribute SourceBufferList sourceBuffers;
    readonly attribute SourceBufferList activeSourceBuffers;
    readonly attribute ReadyState readyState;
    attribute unrestricted double duration;

    [Throws] SourceBuffer addSourceBuffer(DOMString type);
    [Throws] undefined removeSourceBuffer(SourceBuffer sourceBuffer);
    [Throws] undefined endOfStream(optional EndOfStreamError error);
    [Throws] undefined setLiveSeekableRange(double start, double end);
    [Throws] undefined clearLiveSeekableRange();

    static boolean isTypeSupported(DOMString type);
};
```

### 3.2 SourceBuffer Interface

```webidl
enum AppendMode { "segments", "sequence" };

[Exposed=Window]
interface SourceBuffer : EventTarget {
    attribute AppendMode mode;
    readonly attribute boolean updating;
    readonly attribute TimeRanges buffered;
    attribute double timestampOffset;

    [Throws] undefined appendBuffer(BufferSource data);
    [Throws] undefined abort();
    [Throws] undefined remove(double start, unrestricted double end);

    attribute EventHandler onupdatestart;
    attribute EventHandler onupdate;
    attribute EventHandler onupdateend;
    attribute EventHandler onerror;
    attribute EventHandler onabort;
};
```

### 3.3 SourceBufferList Interface

```webidl
[Exposed=Window]
interface SourceBufferList : EventTarget {
    readonly attribute unsigned long length;
    getter SourceBuffer (unsigned long index);

    attribute EventHandler onaddsourcebuffer;
    attribute EventHandler onremovesourcebuffer;
};
```

### 3.4 URL.createObjectURL Extension

MSE requires `URL.createObjectURL(MediaSource)` to produce a `blob:` URL
that HTMLMediaElement can resolve back to the MediaSource object. Existing
`URL.CreateObjectURL` only accepts `Blob`.

## 4. Implementation Plan

### Phase 1: MVP — DOM Stubs + Software Decode Append Path

Goal: `new MediaSource()` → `addSourceBuffer()` → `appendBuffer()` →
playback of AV1/MP4 chunks via the software dav1d path.

#### 4.1 WebIDL Files to Create

| File | Contents |
|------|----------|
| `havi/components/script_bindings/webidls/MediaSource.webidl` | `MediaSource` interface with `ReadyState` enum, `EndOfStreamError` enum |
| `havi/components/script_bindings/webidls/SourceBuffer.webidl` | `SourceBuffer` interface with `AppendMode` enum |
| `havi/components/script_bindings/webidls/SourceBufferList.webidl` | `SourceBufferList` interface |

All gated behind `Pref="dom_media_source_enabled"`.

#### 4.2 DOM Implementation Files to Create

| File | Struct | Role |
|------|--------|------|
| `havi/components/script/dom/media/mediasource.rs` | `MediaSource` | ReadyState machine, sourceBuffers list, duration, object URL registration |
| `havi/components/script/dom/media/sourcebuffer.rs` | `SourceBuffer` | Append buffer queue, `updating` flag, buffered ranges, demux/decode coordination |
| `havi/components/script/dom/media/sourcebufferlist.rs` | `SourceBufferList` | Indexed collection of SourceBuffers, `addsourcebuffer`/`removesourcebuffer` events |

Register in `havi/components/script/dom/media/mod.rs`:
```rust
pub(crate) mod mediasource;
pub(crate) mod sourcebuffer;
pub(crate) mod sourcebufferlist;
```

Register in `havi/components/script/dom/mod.rs` — no change needed; `media`
module already re-exports via `mod.rs`.

#### 4.3 HTMLMediaElement Modifications

File: `havi/components/script/dom/html/htmlmediaelement.rs`

1. **Update `MediaProvider` typedef** in `HTMLMediaElement.webidl`:
   ```
   typedef (MediaStream or MediaSource or Blob) MediaProvider;
   ```
   This changes the generated union from `MediaStreamOrBlob` to
   `MediaStreamOrMediaSourceOrBlob`.

2. **Update `SrcObject` enum** (line 134):
   ```rust
   enum SrcObject {
       MediaStream(Dom<MediaStream>),
       MediaSource(Dom<MediaSource>),
       Blob(Dom<Blob>),
   }
   ```

3. **Update `From` impl** (line 139): add `MediaSource` arm.

4. **Update `resource_selection_algorithm_sync`** (line 760): when
   `src_object` is `MediaSource`, enter MSE attach flow instead of blob
   fetch.

5. **Add `load_from_media_source()`** method: attach the MediaSource, set
   `readyState` to `"open"`, fire `sourceopen` event on the MediaSource.

6. **Update `resource_fetch_algorithm`** (line 1224): add `MediaSource`
   branch in `Resource::Object` match.

7. **Update `Load()` method** (line 1431): implement the TODO for detaching
   MediaSource.

8. **Update `GetSrcObject`/`SetSrcObject`** (lines 2798–2811): handle
   three-variant union.

9. **Import fixups**: update union import from `MediaStreamOrBlob` to
   `MediaStreamOrMediaSourceOrBlob` everywhere it appears.

#### 4.4 URL.createObjectURL Extension

File: `havi/components/script_bindings/webidls/URL.webidl`

Add overload or change `Blob` to union `(Blob or MediaSource)`.

File: `havi/components/script/dom/url.rs`

`CreateObjectURL` needs to accept `MediaSource`. Store the MediaSource
reference in a global registry keyed by the blob URL id, so
HTMLMediaElement's resource selection can resolve it back.

File: `havi/components/script/dom/media/mediasource.rs`

Maintain a `thread_local!` or `GlobalScope`-scoped registry:
`HashMap<Uuid, DomRoot<MediaSource>>` for object URL → MediaSource lookup.

#### 4.5 MediaSource State Machine

```
closed ──[attachToMediaElement]──→ open ──[endOfStream()]──→ ended
  ↑                                  |                         |
  └──────[detach / error]────────────┘─────────────────────────┘
```

Key internal state in `MediaSource`:

```rust
struct MediaSource {
    eventtarget: EventTarget,
    ready_state: Cell<ReadyState>,      // closed | open | ended
    source_buffers: Dom<SourceBufferList>,
    active_source_buffers: Dom<SourceBufferList>,
    duration: Cell<f64>,
    media_element: MutNullableDom<HTMLMediaElement>,
    live_seekable_range: Cell<Option<(f64, f64)>>,
    // Registry for URL.createObjectURL resolution
    object_url_id: DomRefCell<Option<Uuid>>,
}
```

#### 4.6 SourceBuffer Append Pipeline

The SourceBuffer append loop is the core of MSE. It runs asynchronously.

```
JS calls appendBuffer(data)
  → set updating = true
  → fire "updatestart"
  → queue append task on media_element_task_source
  → task: feed bytes to demuxer
  → demuxer produces samples → feed to decoder
  → decoder produces frames → update buffered ranges
  → set updating = false
  → fire "update"
  → fire "updateend"
```

Key internal state in `SourceBuffer`:

```rust
struct SourceBuffer {
    eventtarget: EventTarget,
    media_source: Dom<MediaSource>,
    mode: Cell<AppendMode>,
    updating: Cell<bool>,
    buffered: DomRefCell<TimeRangesContainer>,
    timestamp_offset: Cell<f64>,
    append_state: DomRefCell<AppendState>,
    // Pending buffer queue
    pending_data: DomRefCell<Vec<u8>>,
    // Byte buffer accumulating partial MP4 data
    input_buffer: DomRefCell<Vec<u8>>,
    mime_type: DomRefCell<String>,
}
```

#### 4.7 Demux + Decode Backend for Append

For MVP, use the existing software path:

1. **Incremental MP4 demuxer**: Extend `makepad-media/makepad-media/src/demux.rs`
   to support incremental parsing. Current `parse_mp4()` requires seekable
   `Read+Seek` over the full file. Add `IncrementalDemuxer` struct that:
   - accepts `push_data(&mut self, data: &[u8])`
   - emits `DemuxEvent::InitSegment { width, height, timescale, ... }` and
     `DemuxEvent::MediaSample { data, pts, dts, is_sync }` as boxes complete.
   - Tracks state: waiting-for-ftyp → reading-moov → ready → reading-moof/mdat.
   - CMAF/fMP4 format (init segment `[ftyp+moov]` + media segments `[moof+mdat]`).

2. **Incremental AV1 decode**: The existing `Dav1dDecoder` in
   `makepad-media/makepad-media/src/dav1d_ffi.rs` already accepts individual
   OBU/frame data via `send_data()` and produces frames via `get_picture()`.
   Wire `IncrementalDemuxer` sample output to `Dav1dDecoder`.

3. **New crate or module**: `havi/components/media/mse-backend/` or inline in
   `havi/components/media/media-thread/mse.rs`:

```rust
pub struct MseBackend {
    demuxer: IncrementalDemuxer,
    decoder: Dav1dDecoder,
    video_id: u64,
    image_key: (u32, u32),
    buffered_ranges: Vec<(f64, f64)>,
}

impl MseBackend {
    pub fn new(mime: &str, video_id: u64, image_key: (u32, u32)) -> Result<Self, String>;
    pub fn append_data(&mut self, data: &[u8]) -> Result<AppendResult, String>;
    pub fn end_of_stream(&mut self);
    pub fn buffered(&self) -> &[(f64, f64)];
    pub fn remove(&mut self, start: f64, end: f64);
}

pub struct AppendResult {
    pub init_segment_parsed: bool,
    pub width: u32,
    pub height: u32,
    pub duration_ms: u128,
    pub new_frames: Vec<DecodedFrame>,
}
```

#### 4.8 MediaController Extension

File: `havi/components/media/media-thread/controller.rs`

Add new `VideoOp` variants:

```rust
pub enum VideoOp {
    // ... existing variants ...

    /// Set up an MSE-backed video player (no source URL).
    PrepareMseVideo {
        video_id: u64,
        image_key: (u32, u32),
    },
    /// Append buffer data to an MSE source.
    MseAppendData {
        video_id: u64,
        data: Vec<u8>,
    },
    /// Signal end of stream for MSE source.
    MseEndOfStream {
        video_id: u64,
    },
    /// Remove buffered data in a time range.
    MseRemove {
        video_id: u64,
        start: f64,
        end: f64,
    },
}
```

Add new `MediaEvent` variants:

```rust
pub enum MediaEvent {
    // ... existing variants ...

    /// Append operation completed; source buffer can accept more data.
    MseAppendDone {
        buffered_ranges: Vec<(f64, f64)>,
    },
    /// Append or decode error in MSE pipeline.
    MseError(String),
    /// Init segment parsed; metadata available.
    MseInitSegmentParsed {
        width: u32,
        height: u32,
        duration_ms: u128,
    },
}
```

#### 4.9 Havishell MSE Bridge

File: `havi/ports/havishell/src/app.rs`

In `drain_video_ops()`, handle new MSE-related `VideoOp` variants.

**Design choice**: For MVP, MSE decode happens in the software path
entirely in the bridge thread (or a dedicated thread), bypassing the
platform `playbin`/`AVPlayer`/`IMFMediaEngine`. Decoded frames are uploaded
as YUV textures via the existing `VideoTextureMap` + YUV shader path.

This avoids modifying three platform backends and leverages the existing
dav1d software decoder which already supports AV1.

#### 4.10 Preference Gate

File: `havi/resources/prefs.json` (or equivalent servo config)

Add `dom_media_source_enabled: true` pref to control feature exposure.

### Phase 2: Correctness

Goal: Spec-compliant state machine, error handling, multi-SourceBuffer.

1. **Append window**: implement `appendWindowStart`/`appendWindowEnd`
   attributes on SourceBuffer.

2. **Coded frame processing**: implement the full "coded frame processing"
   algorithm per MSE spec §3.5.5:
   - group start/end timestamps
   - coded frame removal
   - track buffer management

3. **readyState transitions**: implement full `canplay`, `canplaythrough`,
   `waiting`, `stalled` events based on SourceBuffer buffered ranges vs.
   current playback position.

4. **Seeking with MSE**: when HTMLMediaElement seeks, check SourceBuffer
   buffered ranges and either resume playback or fire `waiting` until
   sufficient data is appended.

5. **Multiple SourceBuffers**: video + audio in separate SourceBuffers.
   `activeSourceBuffers` tracking based on selected tracks.

6. **Duration management**: implement duration change algorithm per MSE
   spec §2.4.6.

7. **abort()**: properly cancel in-progress append and reset parser state.

8. **remove()**: implement coded frame removal algorithm per MSE spec §3.5.9.

### Phase 3: Conformance

Goal: Pass relevant WPT tests, hardware decode path.

1. **Platform backend integration**: add `appsrc`-based GStreamer pipeline
   for Linux (avoids software decode overhead). Requires:
   - `gst_app_src_*` FFI bindings in `gstreamer_sys.rs`
   - New pipeline: `appsrc ! parsebin ! avdec_av1 ! appsink`
   - Similar for macOS (`AVSampleBufferDisplayLayer`) and Windows
     (`IMFSourceReader` with custom `IMFByteStream`)

2. **WPT test alignment**: run MSE WPT test subset, fix failures.

3. **`isTypeSupported` precision**: parse codec parameters fully, validate
   level/profile against HAVI AV1 policy.

4. **`changeType()`**: support dynamic codec switching (MSE §3.5.11).

## 5. Threading Model

### 5.1 Current Model

- **Script thread**: runs DOM, HTMLMediaElement, event dispatch
- **Media bridge thread**: per-controller thread (`media-bridge-{id}`);
  receives `MediaEvent` from crossbeam channel, queues tasks on script thread
  via `SendableTaskSource`
- **Makepad main thread**: processes `VideoOp` from crossbeam channel,
  drives platform backend
- **Platform decode thread**: GStreamer internal threads / AVPlayer internal

### 5.2 MSE Threading

MSE adds:

- **Append processing**: must not block script thread. Options:
  a. Process in media bridge thread (extend its role)
  b. Dedicated MSE worker thread per SourceBuffer
  c. Process in Makepad main thread (adds latency)

  **Recommended**: option (b). Each SourceBuffer spawns a worker thread that
  runs the demuxer and decoder. Results flow back via MediaEvent channel.

- **Task source**: MSE events (`updatestart`, `update`, `updateend`) must
  fire on the `media element task source` per spec. The existing
  `media_element_task_source()` in `task_manager.rs` (line 146) is reused.

- **Buffer ownership**: `appendBuffer()` copies the `ArrayBuffer` data on
  the script thread, then sends `Vec<u8>` to the worker. No shared-memory
  complications.

### 5.3 Data Flow for MSE

```
Script thread                    MSE worker thread          Makepad main thread
────────────────────────────────────────────────────────────────────────────────
appendBuffer(data)
  → copy ArrayBuffer to Vec<u8>
  → send to worker ──────────→  demux MP4 boxes
                                  → feed AV1 OBUs to dav1d
                                  → produce YUV frames
                                  → send frames via channel ──→ upload to GPU texture
                                  → send MediaEvent::MseAppendDone
  ← task queued on script ←──  (back via MediaEvent channel)
  fire "updateend"
```

## 6. `isTypeSupported` Implementation

File: `havi/components/script/dom/media/mediasource.rs`

```rust
fn IsTypeSupported(mime: DOMString) -> bool {
    // Parse MIME type + codecs parameter
    // Apply HAVI media policy: only video/mp4 with av01 video codec
    // Audio codecs: opus, mp4a (AAC), flac
    // Reject all other containers and codecs
    media::controller::can_play_type(&mime) != ""
}
```

Reuse the existing `can_play_type()` in
`havi/components/media/media-thread/controller.rs` (line 147), which
already implements the HAVI AV1/MP4-only policy.

## 7. Spec/Doc Updates Required

### 7.1 HAVI Spec

File: `havi/spec/080-MEDIA.md`

Add section under "Recorder/MSE status":

```markdown
## MediaSource (MSE)

`MediaSource` is exposed with AV1/MP4-only support.

`MediaSource.isTypeSupported()` follows HAVI media policy: only
`video/mp4` with `av01` video codecs and `opus`/`mp4a`/`flac` audio
codecs return `true`.

Supported features:
- `MediaSource` constructor, `readyState`, `duration`
- `addSourceBuffer()` / `removeSourceBuffer()`
- `SourceBuffer.appendBuffer()` / `abort()` / `remove()`
- `endOfStream()`

CMAF/fragmented MP4 input format expected (init segment + media segments).
```

### 7.2 JS API Spec

File: `havi/spec/060-JS-API.md`

Update "MediaRecorder and MediaSource status" section to reflect
`MediaSource` availability.

## 8. Tests

### 8.1 Integration Test: MSE Basic Playback

File: `havi/tests/havi/mse-basic-test.sh`

Test page: `havi/tests/havi/content/mse-basic.html`

Test cases:
1. `MediaSource` constructor creates object in `"closed"` state
2. `MediaSource.isTypeSupported('video/mp4; codecs="av01.0.04M.08"')` → `true`
3. `MediaSource.isTypeSupported('video/webm; codecs="vp9"')` → `false`
4. `URL.createObjectURL(mediaSource)` returns `blob:` URL
5. Setting `video.src = blobURL` transitions MediaSource to `"open"`
6. `addSourceBuffer('video/mp4; codecs="av01.0.04M.08"')` succeeds
7. `appendBuffer(initSegment)` fires `updatestart` → `update` → `updateend`
8. `appendBuffer(mediaSegment)` populates `buffered` ranges
9. `endOfStream()` transitions to `"ended"`
10. Video plays frames (verify via `timeupdate` event)
11. `video.duration` reflects content duration after init segment

### 8.2 Integration Test: MSE Error Handling

File: `havi/tests/havi/content/mse-errors.html`

Test cases:
1. `addSourceBuffer()` with unsupported type throws `NotSupportedError`
2. `appendBuffer()` while `updating` throws `InvalidStateError`
3. `appendBuffer()` when MediaSource is `"closed"` throws `InvalidStateError`
4. `endOfStream('decode')` fires `error` on video element
5. Invalid MP4 data in `appendBuffer()` fires `error` on SourceBuffer

### 8.3 Integration Test: MSE Policy

File: `havi/tests/havi/content/mse-policy.html`

Test cases:
1. `isTypeSupported` returns `false` for all non-AV1 video codecs
2. `isTypeSupported` returns `false` for WebM/Ogg containers
3. `addSourceBuffer` with rejected type throws `NotSupportedError`

### 8.4 WPT Test Subset

Relevant WPT directories (if adopted):
- `media-source/` — core MSE tests
- `media-source/mediasource-*.html` — individual API tests

Priority WPT tests for MVP validation:
- `mediasource-is-type-supported.html`
- `mediasource-sourcebuffer-mode.html`
- `mediasource-appendbuffer.html`
- `mediasource-duration.html`
- `mediasource-errors.html`
- `mediasource-endofstream.html`

Expected MVP pass rate: ~40-50% of core MSE WPT tests. Remaining failures
from unimplemented features (changeType, sequence mode edge cases, quota
management).

### 8.5 Test Media Assets

Need fragmented MP4 test files with AV1 video:

Create via ffmpeg:
```bash
ffmpeg -f lavfi -i testsrc=duration=2:size=320x240:rate=30 \
  -c:v libsvtav1 -preset 8 -crf 35 \
  -movflags frag_keyframe+empty_moov+default_base_moof \
  test-av1-frag.mp4
```

Split into init + media segments:
```bash
mp4fragment test-av1-frag.mp4 test-av1-frag-split.mp4
# Or use mp4box/bento4 tools to extract init.mp4 + segment1.m4s
```

Store in `havi/tests/havi/content/media/` and import via test setup.

## 9. Risks and Unknowns

### 9.1 Fragmented MP4 Demuxer

**Risk**: The existing `demux.rs` parser handles only non-fragmented MP4
(single `moov` + `mdat`). CMAF/fMP4 uses `moof` + `mdat` segments.

**Validation**: Before coding, write a standalone test that parses a
fragmented AV1 MP4 file with the existing demuxer. Confirm it fails.
Then implement incremental fMP4 parsing.

**Mitigation**: The ISOBMFF box structure is well-documented. Parsing
`moof` (containing `traf`/`trun`) is straightforward. The existing
`parse_mp4` already handles `moov`/`stbl` parsing which is more complex.

### 9.2 dav1d Frame Timing

**Risk**: dav1d produces frames asynchronously. Frame PTS from the demuxer
must be preserved and matched to decoded output for correct buffered range
tracking.

**Validation**: Test with a 2-second AV1 clip: verify PTS values survive
round-trip through `IncrementalDemuxer` → `Dav1dDecoder` → `DecodedFrame`.

### 9.3 Texture Upload from Worker Thread

**Risk**: YUV frame data produced on the MSE worker thread must be uploaded
to GPU textures on the Makepad main thread. The existing `VideoTextureMap`
+ YUV shader path handles this for regular playback but is driven by
platform events. MSE needs to drive it from append results.

**Validation**: Verify the existing `havi_render::video_texture_map::set_yuv_planes`
can be called with data from any thread. If not, use the existing `VideoOp`
channel to shuttle frame data.

**Mitigation**: Use a bounded channel for decoded frames. The Makepad main
thread polls it in `drain_video_ops()` alongside existing VideoOp processing.

### 9.4 Memory Pressure

**Risk**: Unbounded `appendBuffer` calls accumulate decoded frames in memory.

**Validation**: Test with 60 seconds of 720p AV1 content. Measure memory
usage.

**Mitigation**: Implement quota management (discard decoded frames outside
a configurable window around current playback position). Initial limit:
100 MB per SourceBuffer.

### 9.5 Audio Sync

**Risk**: MVP may decode video-only. Adding audio requires a second
SourceBuffer with audio samples fed to an audio render pipeline.

**Validation**: First verify video-only playback works. Then test with
muxed audio+video in a single SourceBuffer (requires demuxer to handle
multiple tracks from one fMP4 stream).

**Mitigation**: Phase 1 can be video-only. Audio support in Phase 2
by routing audio samples to the existing `AudioRenderer` infrastructure
in `havi/components/media/audio/`.

### 9.6 Name Collision: `MediaSource`

**Risk**: `media::controller::MediaSource` (the `InMemory`/`Network`/
`Filesystem` enum) collides with the MSE `MediaSource` DOM type.

**Mitigation**: Rename the controller enum to `MediaSourceKind` or
`MediaOrigin` before starting MSE work. Affects:
- `havi/components/media/media-thread/controller.rs`
- `havi/components/script/dom/html/htmlmediaelement.rs` (import + usage)
- `havi/ports/havishell/src/app.rs` (import alias)

### 9.7 Object URL Lifecycle

**Risk**: `URL.createObjectURL(mediaSource)` must keep the MediaSource
alive until `URL.revokeObjectURL()` or page unload. Current blob URL
infrastructure uses IPC to the file manager thread. MediaSource objects
live on the script thread and cannot be sent via IPC.

**Mitigation**: Store MediaSource object URLs in a script-thread-local
registry (not the file manager). HTMLMediaElement resolves blob URLs by
checking this registry first, falling back to file manager for Blob URLs.

## 10. Dependency Summary

### New Files (create)

| Path | Purpose |
|------|---------|
| `havi/components/script_bindings/webidls/MediaSource.webidl` | WebIDL definition |
| `havi/components/script_bindings/webidls/SourceBuffer.webidl` | WebIDL definition |
| `havi/components/script_bindings/webidls/SourceBufferList.webidl` | WebIDL definition |
| `havi/components/script/dom/media/mediasource.rs` | DOM implementation |
| `havi/components/script/dom/media/sourcebuffer.rs` | DOM implementation |
| `havi/components/script/dom/media/sourcebufferlist.rs` | DOM implementation |
| `havi/tests/havi/mse-basic-test.sh` | Integration test |
| `havi/tests/havi/content/mse-basic.html` | Test page |
| `havi/tests/havi/content/mse-errors.html` | Test page |
| `havi/tests/havi/content/mse-policy.html` | Test page |
| `havi/tests/havi/content/media/test-av1-init.mp4` | Test asset |
| `havi/tests/havi/content/media/test-av1-seg1.m4s` | Test asset |

### Existing Files (modify)

| Path | Change |
|------|--------|
| `havi/components/script_bindings/webidls/HTMLMediaElement.webidl` | Uncomment `MediaSource` in `MediaProvider` typedef |
| `havi/components/script_bindings/webidls/URL.webidl` | Add `MediaSource` overload for `createObjectURL` |
| `havi/components/script/dom/html/htmlmediaelement.rs` | `SrcObject` enum, resource selection, attach/detach logic |
| `havi/components/script/dom/url.rs` | `CreateObjectURL` MediaSource variant |
| `havi/components/script/dom/media/mod.rs` | Register new modules |
| `havi/components/media/media-thread/controller.rs` | Add MSE `VideoOp`/`MediaEvent` variants; rename `MediaSource` enum |
| `havi/ports/havishell/src/app.rs` | Handle MSE `VideoOp` variants in `drain_video_ops()` |
| `havi/spec/080-MEDIA.md` | Document MSE support |
| `havi/spec/060-JS-API.md` | Update MediaSource status |
| `makepad-media/makepad-media/src/demux.rs` | Add `IncrementalDemuxer` for fMP4 |

### No Changes Needed

| Path | Reason |
|------|--------|
| `makepad/platform/src/os/linux/linux_video_playback.rs` | MVP uses software decode, not GStreamer pipeline |
| `makepad/platform/src/os/apple/apple_video_playback.rs` | MVP uses software decode |
| `makepad/platform/src/os/windows/windows_video_playback.rs` | MVP uses software decode |
| `makepad/platform/src/os/linux/gstreamer_sys.rs` | No appsrc needed for MVP |
| `makepad/platform/src/cx_api.rs` | Existing prepare/playback API sufficient |
| `makepad/platform/src/event/video_playback.rs` | Existing event types sufficient |

## 11. Estimated Scope

| Phase | Effort | Deliverable |
|-------|--------|-------------|
| Phase 1 (MVP) | Large | `MediaSource` + `SourceBuffer` DOM, fMP4 demuxer, dav1d decode, basic playback |
| Phase 2 (Correctness) | Medium | Full state machine, multi-SourceBuffer, seeking, error handling |
| Phase 3 (Conformance) | Large | Platform backend integration, WPT alignment, changeType |

Phase 1 is the critical path. The largest single work item is the
incremental fMP4 demuxer (`IncrementalDemuxer`), followed by the
`SourceBuffer` append pipeline integration with the script thread event
model.
