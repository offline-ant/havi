# WebAudio

The [WebAudio API](https://webaudio.github.io/web-audio-api/)
is a high-level JavaScript API for processing and
synthesizing audio in web applications.

WebAudio uses a
[Modular Routing](https://webaudio.github.io/web-audio-api/#ModularRouting)
model that connects multiple
[AudioNode](https://github.com/servo/media/blob/2610789d1abfbe4443579021113c822ba05f34dc/audio/node.rs#L122)
inputs and outputs. Nodes can be `sources` (no inputs,
single output), `destinations` (one input, no output) or
`filters` (multiple inputs and outputs). The simplest case
is a single source routed directly to the output.

![Modular Routing](images/modular-routing1.png)

Everything happens within an
[AudioContext](https://github.com/servo/media/blob/2610789d1abfbe4443579021113c822ba05f34dc/audio/context.rs#L105)
that manages and plays all sounds to its single
[AudioDestinationNode](https://github.com/servo/media/blob/2610789d1abfbe4443579021113c822ba05f34dc/audio/destination_node.rs#L6).
Audio can be rendered to hardware or to a buffer via
[OfflineAudioContext](https://github.com/servo/media/blob/2610789d1abfbe4443579021113c822ba05f34dc/audio/context.rs#L94).

The `servo-media` Rust API for WebAudio is deliberately
close to the actual WebAudio JavaScript API.

```rust
/*
  This is an example of a very basic WebAudio pipeline with an OscillatorNode connected to a GainNode.
  ------------------------------------------------------------
  |  AudioContext                                            |
  |      OscillatorNode -> GainNode -> AudioDestinationNode  |
  ------------------------------------------------------------
  NOTE: Some boilerplate has been removed for simplicity.
  Please, visit the examples folder for a more complete version.
*/

// Context creation.
let context =
  servo_media.create_audio_context(&ClientContextId::build(1, 1), Default::default());

// Create and configure nodes.
let osc = context.create_node(
  AudioNodeInit::OscillatorNode(Default::default()),
  Default::default(),
).expect("Failed to create oscillator node");
let mut options = GainNodeOptions::default();
options.gain = 0.5;
let gain = context.create_node(AudioNodeInit::GainNode(options), Default::default())
  .expect("Failed to create gain node");

// Connect nodes.
let dest = context.dest_node();
context.connect_ports(osc.output(0), gain.input(0));
context.connect_ports(gain.output(0), dest.input(0));

// Start playing.
context.message_node(
  osc,
  AudioNodeMessage::AudioScheduledSourceNode(AudioScheduledSourceNodeMessage::Start(0.)),
);
```

## Implementation

### Threading model

Following the
[WebAudio API specification](https://webaudio.github.io/web-audio-api/#control-thread-and-rendering-thread),
`servo-media` implements the concepts of
[control thread](https://webaudio.github.io/web-audio-api/#control-thread)
and
[rendering thread](https://webaudio.github.io/web-audio-api/#rendering-thread).

The `control thread` is the thread from which the
`AudioContext` is instantiated and from which authors
manipulate the audio graph. In Servo's case, this is the
[script](https://github.com/servo/servo/blob/594ea14d5bd7b76d09b679fd0454165259ffbe7a/components/script/script_thread.rs#L5)
thread.

The `rendering thread` is where actual audio processing
happens. It keeps an
[event loop](https://github.com/servo/media/blob/2610789d1abfbe4443579021113c822ba05f34dc/audio/render_thread.rs#L250)
that handles control messages from the `control thread`
and processes audio in 128-sample blocks called
[render quantums](https://webaudio.github.io/web-audio-api/#render-quantum).
Each iteration calls
[AudioRenderThread.process](https://github.com/servo/media/blob/2610789d1abfbe4443579021113c822ba05f34dc/audio/render_thread.rs#L233),
which runs a
[DFS](https://en.wikipedia.org/wiki/Depth-first_search)
traversal calling
[process](https://github.com/servo/media/blob/main/audio/node.rs#L126)
on each node. The resulting audio data is
[pushed](https://github.com/servo/media/blob/2610789d1abfbe4443579021113c822ba05f34dc/audio/render_thread.rs#L337)
to the audio sink.

### Audio Playback

WebAudio renders processed audio to hardware or to a
buffer. `servo-media` abstracts this via the
[AudioSink](https://github.com/servo/media/blob/2610789d1abfbe4443579021113c822ba05f34dc/audio/sink.rs#L16)
trait.

For offline rendering, there is an
[OfflineAudioSink](https://github.com/servo/media/blob/2610789d1abfbe4443579021113c822ba05f34dc/audio/offline_sink.rs#L38)
implementation.

For hardware rendering, backends implement `AudioSink`.
The GStreamer
[implementation](https://github.com/servo/media/blob/2610789d1abfbe4443579021113c822ba05f34dc/backends/gstreamer/audio_sink.rs#L73)
creates a simple audio pipeline:

![WebAudio Playback Pipeline](images/webaudiopipeline.png)

The core piece is the
[appsrc](https://gstreamer.freedesktop.org/documentation/applib/gstappsrc.html?gi-language=c)
element that inserts audio chunks into the GStreamer
pipeline. We use `appsrc` in push mode, repeatedly calling
[push-buffer](https://gstreamer.freedesktop.org/documentation/applib/gstappsrc.html?gi-language=c#gst_app_src_push_buffer)
with new buffers. To avoid blocking the render thread, we
set the max queued bytes to 1 and use
[get_current_level_bytes](https://gstreamer.freedesktop.org/documentation/applib/gstappsrc.html?gi-language=c#gst_app_src_get_current_level_bytes)
and the
[need-data](https://gstreamer.freedesktop.org/documentation/applib/gstappsrc.html?gi-language=c#GstAppSrc::need-data)
signal to decide whether to push.

### Audio Decoding

WebAudio also supports decoding audio data.

`servo-media` exposes an
[AudioDecoder](https://github.com/ferjm/media/blob/a95e063729324c359976236104d825244bb180e8/servo-media/src/audio/decoder.rs#L92)
trait with a single `decode` method that takes audio data
and an
[AudioDecoderCallbacks](https://github.com/ferjm/media/blob/a95e063729324c359976236104d825244bb180e8/servo-media/src/audio/decoder.rs#L1)
instance for end-of-stream, error, and progress events.

`servo-media` backends are required to implement this trait.

The GStreamer `AudioDecoder` implementation creates a
pipeline of this form:

![WebAudio Decoding Pipeline](images/webaudiopipeline_decoder.png)

`decodebin` auto-constructs a decoding pipeline using
available decoders and demuxers via auto-plugging.


