/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use html5ever::{LocalName, Prefix};
use js::rust::HandleObject;

use crate::script::dom::bindings::inheritance::Castable;
use crate::script::dom::bindings::root::DomRoot;
use crate::script::dom::document::Document;
use crate::script::dom::html::htmlelement::HTMLElement;
use crate::script::dom::node::Node;
use crate::script::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct HTMLHtmlElement {
    htmlelement: HTMLElement,
}

#[expect(non_snake_case)]
impl HTMLHtmlElement {
    fn new_inherited(
        localName: LocalName,
        prefix: Option<Prefix>,
        document: &Document,
    ) -> HTMLHtmlElement {
        HTMLHtmlElement {
            htmlelement: HTMLElement::new_inherited(localName, prefix, document),
        }
    }

    pub(crate) fn new(
        localName: LocalName,
        prefix: Option<Prefix>,
        document: &Document,
        proto: Option<HandleObject>,
        can_gc: CanGc,
    ) -> DomRoot<HTMLHtmlElement> {
        let n = Node::reflect_node_with_proto(
            Box::new(HTMLHtmlElement::new_inherited(localName, prefix, document)),
            document,
            proto,
            can_gc,
        );

        n.upcast::<Node>().set_weird_parser_insertion_mode();
        n
    }
}
