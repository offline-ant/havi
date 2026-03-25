[Exposed=Window, Pref="dom_hppr_enabled"]
interface HpprResolveResult {
    [SameObject] readonly attribute HpprPacket packet;
    readonly attribute DOMString endpoint;
    readonly attribute DOMString? signer;
    readonly attribute DOMString? contentAuthority;
    readonly attribute boolean isRepo;
};
