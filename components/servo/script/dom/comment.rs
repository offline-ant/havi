/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use js::rust::HandleObject;

use crate::script::dom::bindings::codegen::GenericBindings::CommentBinding::CommentMethods;
use crate::script::dom::bindings::codegen::GenericBindings::WindowBinding::WindowMethods;
use crate::script::dom::bindings::error::Fallible;
use crate::script::dom::bindings::root::DomRoot;
use crate::script::dom::bindings::str::DOMString;
use crate::script::dom::characterdata::CharacterData;
use crate::script::dom::document::Document;
use crate::script::dom::node::Node;
use crate::script::dom::window::Window;
use crate::script::script_runtime::CanGc;

/// An HTML comment.
#[dom_struct]
pub(crate) struct Comment {
    characterdata: CharacterData,
}

impl Comment {
    fn new_inherited(text: DOMString, document: &Document) -> Comment {
        Comment {
            characterdata: CharacterData::new_inherited(text, document),
        }
    }

    pub(crate) fn new(
        text: DOMString,
        document: &Document,
        proto: Option<HandleObject>,
        can_gc: CanGc,
    ) -> DomRoot<Comment> {
        Node::reflect_node_with_proto(
            Box::new(Comment::new_inherited(text, document)),
            document,
            proto,
            can_gc,
        )
    }
}

impl CommentMethods<crate::DomTypeHolder> for Comment {
    /// <https://dom.spec.whatwg.org/#dom-comment-comment>
    fn Constructor(
        window: &Window,
        proto: Option<HandleObject>,
        can_gc: CanGc,
        data: DOMString,
    ) -> Fallible<DomRoot<Comment>> {
        let document = window.Document();
        Ok(Comment::new(data, &document, proto, can_gc))
    }
}
