/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// HPPR ADD command options
dictionary HpprAddOptions {
    (DOMString or sequence<DOMString>) headers;              // Custom headers (string or array)
    (Blob or ArrayBuffer or ArrayBufferView or USVString) data;  // Packet body
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
    // Browser-mediated named client, subject to origin grant.
    [NewObject, Throws] static Promise<HpprClient> named(DOMString name);

    // Convert to EnvelopeHpprClient for transport-oriented operations.
    [NewObject] EnvelopeHpprClient envelope();

    // Identity info (read-only)
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
};
