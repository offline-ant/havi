/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// HPPR STREAM_IN publisher interface — pushes trailer-format data to the server

dictionary StreamInOptions {
    DOMString key;                        // Signing key — enables publisher mode
    record<DOMString, DOMString> headers; // Extra headers per segment
    unsigned long maxSegmentSize;         // Max data bytes per segment
    ByteString flushSeq;                  // Flush sequence (raw bytes)
};

[Exposed=Window, Pref="dom_hppr_enabled"]
interface StreamIn : EventTarget {
    const unsigned short CONNECTING = 0;
    const unsigned short OPEN = 1;
    const unsigned short CLOSING = 2;
    const unsigned short CLOSED = 3;

    readonly attribute unsigned short readyState;
    readonly attribute USVString prefix;

    attribute EventHandler onopen;
    attribute EventHandler onerror;
    attribute EventHandler onclose;
    attribute EventHandler onpacket;

    [NewObject] Promise<undefined> write(BufferSource data);
    undefined finishSegment();
    undefined close();
};
