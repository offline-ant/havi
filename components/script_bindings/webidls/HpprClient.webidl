/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// HPPR ADD command options
dictionary HpprAddOptions {
    (DOMString or sequence<DOMString>) headers;              // Custom headers (string or array)
    (Blob or ArrayBuffer or ArrayBufferView or USVString) data;  // Packet body
};

// Options for HpprClient.repo()
dictionary HpprRepoOptions {
    DOMString role;  // Optional role name for elevated access (HAVI-role:<app>#<role>)
};

dictionary HpprGreeting {
    required DOMString repoName;
    required DOMString sessionId;
    required DOMString verifyingKey;
    DOMString? phc;
    required DOMString format;
    required sequence<DOMString> commands;
    DOMString? status;
    DOMString? uptime;
    DOMString? version;
    DOMString? backend;
};

[Exposed=Window, Pref="dom_hppr_enabled"]
interface HpprClient {
    // Repo client with optional role for elevated access
    // Without role: uses site sandbox HAVI-site:<group>#<app>
    // With role: uses elevated HAVI-role:<app>#<role>, signed by site key
    [NewObject, Throws] static Promise<HpprClient> home(optional HpprRepoOptions options = {});

    // Remote client with optional identity string
    // Omitted or empty: anyone (no authentication)
    // Identity formats: !ring1/token, !ring1#&.key.H3, @group#&.key.H3
    [NewObject, Throws] static Promise<HpprClient> connect(DOMString endpoint, optional DOMString identity);

    // Convert to EnvelopeHpprClient (returns HpprResult with envelopes)
    [NewObject] EnvelopeHpprClient envelope();

    // Identity info (read-only)
    readonly attribute DOMString endpoint;
    readonly attribute DOMString? account;
    readonly attribute DOMString? group;
    readonly attribute DOMString? ring1Name;

    // Query operations - return value directly
    [NewObject] Promise<any> get(USVString urc);
    [NewObject] Promise<any> list(USVString urc);
    [NewObject] Promise<any> headers(USVString urc);
    [NewObject] Promise<any> tips(USVString urc);
    [NewObject] Promise<any> members(USVString urc);

    // Mutation operations - return value directly
    [NewObject] Promise<any> store(HpprPacket packet);
    [NewObject] Promise<any> detach(DOMString hash);
    [NewObject] Promise<any> add(optional HpprAddOptions options = {});

    // Get repo greeting via HELLO command (remote clients only)
    [NewObject] Promise<HpprGreeting> hello();

    // Admin-only sub-objects (null for non-ring0 clients)
    [SameObject] readonly attribute HpprRepoInfo? repo;

    // WATCH streaming - monitors coordinate prefix for changes
    WatchSocket watch(USVString urc);

    // STREAM_IN — publisher streaming (push trailer-format data to repo)
    StreamIn streamIn(USVString prefix, optional StreamInOptions options = {});

    // STREAM_OUT — subscriber streaming (receive trailer-format data as ReadableStream)
    StreamOut streamOut(USVString prefix);
};