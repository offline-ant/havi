/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// HPPR STREAM_OUT subscriber interface — receives trailer-format data as ReadableStream

[Exposed=Window, Pref="dom_hppr_enabled"]
interface StreamOut : EventTarget {
    const unsigned short CONNECTING = 0;
    const unsigned short OPEN = 1;
    const unsigned short CLOSING = 2;
    const unsigned short CLOSED = 3;

    readonly attribute unsigned short readyState;
    readonly attribute USVString prefix;
    readonly attribute ReadableStream stream;

    attribute EventHandler onopen;
    attribute EventHandler onerror;
    attribute EventHandler onclose;
    attribute EventHandler onpacket;

    undefined close();
};
