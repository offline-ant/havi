[Exposed=Window, Pref="dom_hppr_enabled"]
interface HpprSource {
    readonly attribute HpprClient client;
    readonly attribute DOMString kind;
    readonly attribute DOMString? authority;
};
