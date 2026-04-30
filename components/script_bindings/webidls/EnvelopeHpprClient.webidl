/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

[Exposed=Window, Pref="dom_hppr_enabled"]
interface EnvelopeHpprClient {
    // Remote client with optional identity string.
    // Omitted or empty: anyone (no authentication).
    [NewObject, Throws] static Promise<EnvelopeHpprClient> connect(DOMString endpoint, optional DOMString identity);

    // Unpack the envelope: returns an HpprClient that yields values directly.
    [NewObject] HpprClient unpack();

    // Remote transport info.
    readonly attribute DOMString? endpoint;
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

    // WATCH/STREAM remain on the transport-oriented surface because browser-
    // owned local and named-client backends do not provide universal parity.
    WatchSocket watch(USVString urc);
    StreamPub streamPub(USVString prefix, optional StreamPubOptions options = {});
    StreamSub streamSub(USVString prefix);
};
