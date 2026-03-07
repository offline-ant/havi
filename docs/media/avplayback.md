# A/V Playback through HTMLMediaElement

Media elements present audio and/or video data to the user.
The [HTMLMediaElement](https://html.spec.whatwg.org/multipage/media.html#htmlmediaelement)
interface adds to [HTMLElement](https://html.spec.whatwg.org/multipage/dom.html#htmlelement)
the properties and methods needed to support basic
media-related capabilities common to audio and video.
The [HTMLVideoElement](https://html.spec.whatwg.org/multipage/media.html#htmlvideoelement)
(`<video>`) and [HTMLAudioElement](https://html.spec.whatwg.org/multipage/media.html#htmlaudioelement)
(`<audio>`) elements both inherit this interface.


`servo-media` exposes a Rust `Player` API that provides
audio and video playback capabilities. Servo uses it to
[implement](https://github.com/servo/servo/blob/7bfa9179319d714656e7184e5159ea42595086e5/components/script/dom/htmlmediaelement.rs#L1)
the HTMLMediaElement API.
```rust
/*
  This is an example of a very basic Player playing media from a file.
  NOTE: Some boilerplate has been removed for simplicity.
  Please, visit the examples folder for a more complete version.
*/

// Create Player instance
let (sender, receiver) = ipc::channel().unwrap();
let player = servo_media.create_player(
    &ClientContextId::build(1, 1),
    StreamType::Seekable,
    sender,
    None,
    None,
    Box::new(PlayerContextDummy()),
);

// Open file and set input size.
let file = match File::open(&path)?;
if let Ok(metadata) = file.metadata() {
    player
        .lock()
        .unwrap()
        .set_input_size(metadata.len())
        .unwrap();
}

// Read from file and push buffers to the player.
let player_clone = Arc::clone(&player);
thread::spawn(move || {
    let player = &player_clone;
    let mut buf_reader = BufReader::new(file);
    let mut buffer = [0; 1024];
    let mut read = |offset| {
        match buf_reader.read(&mut buffer[..]) {
            Ok(0) => {
                println!("Finished pushing data");
                break;
            }
            Ok(size) => player
                .lock()
                .unwrap()
                .push_data(Vec::from(&buffer[0..size]))
                .unwrap(),
            Err(e) => {
                eprintln!("Error: {}", e);
                break;
            }
        }
    };
});

// Start playing.
player.lock().unwrap().play().unwrap();

// Listen for Player events.
while let Ok(event) = receiver.recv() {
    match event {
        PlayerEvent::EndOfStream => {
            println!("\nEOF");
            break;
        }
        PlayerEvent::Error(ref s) => {
            println!("\nError {:?}", s);
            break;
        }
        PlayerEvent::MetadataUpdated(ref m) => {
            println!("\nMetadata updated! {:?}", m);
        }
        PlayerEvent::DurationChanged(d) => {
            println!("\nDuration changed! {:?}", d);
        },
        PlayerEvent::StateChanged(ref s) => {
            println!("\nPlayer state changed to {:?}", s);
        }
        PlayerEvent::VideoFrameUpdated => eprint!("."),
        PlayerEvent::PositionChanged(p) => {}
        PlayerEvent::SeekData(_, _) => {}
        PlayerEvent::SeekDone(_) => {},
        PlayerEvent::NeedData => println!("\nNeedData"),
        PlayerEvent::EnoughData => println!("\nEnoughData"),
    }
}
```

## Implementation
The entry point is
[ServoMedia.create_player()](https://github.com/servo/media/blob/b64b86b727ade722eaf571e65ff678364b69fc08/servo-media/lib.rs#L34),
which takes an `IpcSender<PlayerEvent>` for events, and
shared references to
[VideoFrameRenderer](https://github.com/servo/media/blob/main/player/video.rs#L71)
and
[AudioFrameRenderer](https://github.com/servo/media/blob/b64b86b727ade722eaf571e65ff678364b69fc08/player/audio.rs#L1)
that receive frames as the media player produces them.

Backends implement the
[Player](https://github.com/servo/media/blob/main/player/lib.rs#L92)
trait, which exposes a basic API to control a/v playback.

The media player does not fetch media data. The `Player`
trait exposes a
[push_data()](https://github.com/servo/media/blob/b64b86b727ade722eaf571e65ff678364b69fc08/player/lib.rs#L101)
method that accepts a buffer of media data. Servo's
[HTMLMediaElement implementation](https://github.com/servo/servo/blob/7bfa9179319d714656e7184e5159ea42595086e5/components/script/dom/htmlmediaelement.rs#L900)
fetches data from the network or a file and
[feeds](https://github.com/servo/servo/blob/7bfa9179319d714656e7184e5159ea42595086e5/components/script/dom/htmlmediaelement.rs#L2702)
the backend with media buffers. The player decodes the data
and builds audio and/or video frames. Video frames are
output as raw images by default, or as GL textures when
hardware acceleration is available.
[WebRender](https://github.com/servo/webrender) is
responsible for
[rendering](https://github.com/servo/servo/blob/b41f5f97f26895f874514ce88cb359d65915738c/components/layout/display_list/builder.rs#L1883)
the images that `servo-media`
[outputs](https://github.com/servo/servo/blob/7bfa9179319d714656e7184e5159ea42595086e5/components/script/dom/htmlmediaelement.rs#L179).

The
[GStreamer](https://github.com/servo/media/blob/main/backends/gstreamer/player.rs#L776)
implementation wraps
[GstPlayer](https://gstreamer.freedesktop.org/documentation/player/gstplayer.html?gi-language=c),
a media playback API that hides GStreamer complexity. It
uses the
[playbin](https://gstreamer.freedesktop.org/documentation/playback/playbin3.html?gi-language=c)
element, a stand-alone abstraction for audio/video playback
that dynamically builds the appropriate decoding pipeline.

### Hardware acceleration
TODO

