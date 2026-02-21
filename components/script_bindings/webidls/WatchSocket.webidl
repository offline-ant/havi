/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// HPPR WATCH streaming interface - mirrors WebSocket event pattern
// https://github.com/user/hppr

[Exposed=Window, Pref="dom_hppr_enabled"]
interface WatchSocket : EventTarget {
    // Ready state constants (mirrors WebSocket)
    const unsigned short CONNECTING = 0;
    const unsigned short OPEN = 1;
    const unsigned short CLOSING = 2;
    const unsigned short CLOSED = 3;

    // Current connection state
    readonly attribute unsigned short readyState;

    // The coordinate being watched
    readonly attribute USVString urc;

    // Event handlers
    attribute EventHandler onopen;
    attribute EventHandler onmessage;
    attribute EventHandler onerror;
    attribute EventHandler onclose;

    // Close the watch connection
    undefined close();
};
