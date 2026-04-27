[Exposed=Window, Pref="dom_hppr_enabled"]
interface HpprResolveResult {
    [SameObject] readonly attribute HpprPacket packet;
    readonly attribute DOMString kind;
    readonly attribute DOMString? contentAuthority;
};
