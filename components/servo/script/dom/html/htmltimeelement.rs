/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use html5ever::{LocalName, Prefix};
use js::rust::HandleObject;

use crate::script::dom::bindings::codegen::GenericBindings::HTMLTimeElementBinding::HTMLTimeElementMethods;
use crate::script::dom::bindings::root::DomRoot;
use crate::script::dom::bindings::str::DOMString;
use crate::script::dom::document::Document;
use crate::script::dom::html::htmlelement::HTMLElement;
use crate::script::dom::node::Node;
use crate::script::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct HTMLTimeElement {
    htmlelement: HTMLElement,
}

impl HTMLTimeElement {
    fn new_inherited(
        local_name: LocalName,
        prefix: Option<Prefix>,
        document: &Document,
    ) -> HTMLTimeElement {
        HTMLTimeElement {
            htmlelement: HTMLElement::new_inherited(local_name, prefix, document),
        }
    }

    pub(crate) fn new(
        local_name: LocalName,
        prefix: Option<Prefix>,
        document: &Document,
        proto: Option<HandleObject>,
        can_gc: CanGc,
    ) -> DomRoot<HTMLTimeElement> {
        Node::reflect_node_with_proto(
            Box::new(HTMLTimeElement::new_inherited(local_name, prefix, document)),
            document,
            proto,
            can_gc,
        )
    }
}

impl HTMLTimeElementMethods<crate::DomTypeHolder> for HTMLTimeElement {
    // https://html.spec.whatwg.org/multipage/#dom-time-datetime
    make_getter!(DateTime, "datetime");

    // https://html.spec.whatwg.org/multipage/#dom-time-datetime
    make_setter!(SetDateTime, "datetime");
}
