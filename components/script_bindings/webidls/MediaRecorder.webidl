/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// https://w3c.github.io/mediacapture-record/

enum RecordingState {
    "inactive",
    "recording",
    "paused"
};

dictionary MediaRecorderOptions {
    DOMString mimeType = "";
    unsigned long audioBitsPerSecond;
    unsigned long videoBitsPerSecond;
    unsigned long bitsPerSecond;
};

[Exposed=Window, Pref="dom_media_capture_enabled"]
interface MediaRecorder : EventTarget {
    [Throws] constructor(MediaStream stream, optional MediaRecorderOptions options = {});

    readonly attribute RecordingState state;
    readonly attribute DOMString mimeType;
    readonly attribute MediaStream stream;

    attribute EventHandler onstart;
    attribute EventHandler onstop;
    attribute EventHandler ondataavailable;
    attribute EventHandler onerror;

    [Throws] undefined start(optional unsigned long timeslice);
    [Throws] undefined stop();
    [Throws] undefined pause();
    [Throws] undefined resume();
    [Throws] undefined requestData();

    static boolean isTypeSupported(DOMString mimeType);
};
