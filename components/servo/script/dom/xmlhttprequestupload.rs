/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;

use crate::script::dom::bindings::reflector::reflect_dom_object;
use crate::script::dom::bindings::root::DomRoot;
use crate::script::dom::globalscope::GlobalScope;
use crate::script::dom::xmlhttprequesteventtarget::XMLHttpRequestEventTarget;
use crate::script::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct XMLHttpRequestUpload {
    eventtarget: XMLHttpRequestEventTarget,
}

impl XMLHttpRequestUpload {
    fn new_inherited() -> XMLHttpRequestUpload {
        XMLHttpRequestUpload {
            eventtarget: XMLHttpRequestEventTarget::new_inherited(),
        }
    }
    pub(crate) fn new(global: &GlobalScope, can_gc: CanGc) -> DomRoot<XMLHttpRequestUpload> {
        reflect_dom_object(
            Box::new(XMLHttpRequestUpload::new_inherited()),
            global,
            can_gc,
        )
    }
}
