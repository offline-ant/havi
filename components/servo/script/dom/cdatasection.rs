/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;

use crate::script::dom::bindings::root::DomRoot;
use crate::script::dom::bindings::str::DOMString;
use crate::script::dom::document::Document;
use crate::script::dom::node::Node;
use crate::script::dom::text::Text;
use crate::script::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct CDATASection {
    text: Text,
}

impl CDATASection {
    fn new_inherited(text: DOMString, document: &Document) -> CDATASection {
        CDATASection {
            text: Text::new_inherited(text, document),
        }
    }

    pub(crate) fn new(
        text: DOMString,
        document: &Document,
        can_gc: CanGc,
    ) -> DomRoot<CDATASection> {
        Node::reflect_node(
            Box::new(CDATASection::new_inherited(text, document)),
            document,
            can_gc,
        )
    }
}
