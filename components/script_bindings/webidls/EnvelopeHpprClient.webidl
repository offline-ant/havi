/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

[Exposed=Window, Pref="dom_hppr_enabled"]
interface EnvelopeHpprClient {
    // Repo client using site's signing key via SealRing1
    [NewObject, Throws] static Promise<EnvelopeHpprClient> home();

    // Remote client with optional identity string
    // Omitted or empty: anyone (no authentication)
    // Identity formats: !ring1/token, !ring1#&.key.H3, @group#&.key.H3
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

    // Admin-only sub-objects (null for non-ring0 clients)
    [SameObject] readonly attribute HpprRepoInfo? repo;

    // WATCH streaming - monitors coordinate prefix for changes
    // Returns WatchSocket with WebSocket-like event interface
    WatchSocket watch(USVString urc);

    // STREAM_IN — publisher streaming (push trailer-format data to repo)
    StreamIn streamIn(USVString prefix, optional StreamInOptions options = {});

    // STREAM_OUT — subscriber streaming (receive trailer-format data as ReadableStream)
    StreamOut streamOut(USVString prefix);
};
