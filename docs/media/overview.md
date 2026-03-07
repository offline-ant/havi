 # Servo Media - Overview

 The `servo-media` crate contains the backend implementation
 to support all [Servo](https://github.com/servo/servo)
 multimedia related functionality. This is:
  - the [HTMLMediaElement](https://html.spec.whatwg.org/multipage/media.html#htmlmediaelement) and the `<audio>` and `<video>` elements.
  - the [WebAudio API](https://webaudio.github.io/web-audio-api).
  - the [WebRTC API](https://w3c.github.io/webrtc-pc/).
  - the [Media Capture and Streams APIs](https://w3c.github.io/mediacapture-main/#dom-mediadeviceinfo-groupid).

`servo-media` runs on Linux, macOS, Windows and Android.
Check the
[build](https://github.com/servo/media/tree/f96c33b7374d5b9915b8bae8623723b2d23ec457#build)
instructions for each specific platform.

`servo-media` is built modularly from different crates and
provides an abstraction for multiple media backends. The
only functional backend is
[GStreamer](https://github.com/servo/media/tree/f96c33b7374d5b9915b8bae8623723b2d23ec457/backends/gstreamer).
New backends implement the
[Backend](https://github.com/servo/media/blob/2610789d1abfbe4443579021113c822ba05f34dc/servo-media/lib.rs#L33)
trait, the public API exposed through the
[ServoMedia](https://github.com/servo/media/blob/2610789d1abfbe4443579021113c822ba05f34dc/servo-media/lib.rs#L90)
entry point. See the
[examples](https://github.com/servo/media/tree/f96c33b7374d5b9915b8bae8623723b2d23ec457/examples)
folder, or check how `servo-media` is used in
[Servo](https://github.com/servo/servo).

