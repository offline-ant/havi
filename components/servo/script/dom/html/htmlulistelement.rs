/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use html5ever::{LocalName, Prefix};
use js::rust::HandleObject;

use crate::script::dom::bindings::codegen::GenericBindings::HTMLUListElementBinding::HTMLUListElementMethods;
use crate::script::dom::bindings::root::DomRoot;
use crate::script::dom::bindings::str::DOMString;
use crate::script::dom::document::Document;
use crate::script::dom::html::htmlelement::HTMLElement;
use crate::script::dom::node::Node;
use crate::script::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct HTMLUListElement {
    htmlelement: HTMLElement,
}

impl HTMLUListElement {
    fn new_inherited(
        local_name: LocalName,
        prefix: Option<Prefix>,
        document: &Document,
    ) -> HTMLUListElement {
        HTMLUListElement {
            htmlelement: HTMLElement::new_inherited(local_name, prefix, document),
        }
    }

    pub(crate) fn new(
        local_name: LocalName,
        prefix: Option<Prefix>,
        document: &Document,
        proto: Option<HandleObject>,
        can_gc: CanGc,
    ) -> DomRoot<HTMLUListElement> {
        Node::reflect_node_with_proto(
            Box::new(HTMLUListElement::new_inherited(
                local_name, prefix, document,
            )),
            document,
            proto,
            can_gc,
        )
    }
}

impl HTMLUListElementMethods<crate::DomTypeHolder> for HTMLUListElement {
    // https://html.spec.whatwg.org/multipage/#dom-ul-compact
    make_bool_getter!(Compact, "compact");

    // https://html.spec.whatwg.org/multipage/#dom-ul-compact
    make_bool_setter!(SetCompact, "compact");

    // https://html.spec.whatwg.org/multipage/#dom-ul-type
    make_getter!(Type, "type");

    // https://html.spec.whatwg.org/multipage/#dom-ul-type
    make_setter!(SetType, "type");
}
