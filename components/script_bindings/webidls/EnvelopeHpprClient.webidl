/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

[Exposed=Window, Pref="dom_hppr_enabled"]
interface EnvelopeHpprClient {
    // Remote client with optional identity string
    // Omitted or empty: anyone (no authentication)
    // Identity formats: ring1:<name>|<password>, ring1:<name>|&.key.H3,
    // ring2:<group>|&.key.H3, ring2:<group>/<user>|<password>
    [NewObject, Throws] static Promise<EnvelopeHpprClient> connect(DOMString endpoint, optional DOMString identity);

    // Unpack the envelope: returns an HpprClient that yields values directly
    [NewObject] HpprClient unpack();

    // Identity info (read-only)
    readonly attribute DOMString endpoint;
    readonly attribute DOMString? account;
    readonly attribute DOMString? group;

    // Query operations - always return HpprResult
    [NewObject] Promise<HpprResult> get(USVString urc);
    [NewObject] Promise<HpprResult> list(USVString urc);
    [NewObject] Promise<HpprResult> headers(USVString urc);
    [NewObject] Promise<HpprResult> tips(USVString urc);
    [NewObject] Promise<HpprResult> members(USVString urc);

    // Mutation operations - always return HpprResult
    [NewObject] Promise<HpprResult> store(HpprPacket packet);
    [NewObject] Promise<HpprResult> detach(DOMString hash);
    [NewObject] Promise<HpprResult> add(optional HpprAddOptions options = {});

    // Get repo greeting via HELLO command (remote clients only)
    [NewObject] Promise<HpprResult> hello();

    // WATCH streaming - monitors coordinate prefix for changes
    // Returns WatchSocket with WebSocket-like event interface
    WatchSocket watch(USVString urc);

    // STREAM_PUB — payload-oriented publisher streaming
    StreamPub streamPub(USVString prefix, optional StreamPubOptions options = {});

    // STREAM_SUB — payload-oriented subscriber streaming
    StreamSub streamSub(USVString prefix);
};
