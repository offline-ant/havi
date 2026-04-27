/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HAVI internal helper capability root.

use dom_struct::dom_struct;
use script_bindings::codegen::GenericBindings::WindowBinding::WindowMethods;

use crate::dom::bindings::codegen::Bindings::HaviInternalBinding::HaviInternalMethods;
use crate::dom::bindings::reflector::{DomGlobal, Reflector, reflect_dom_object};
use crate::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::dom::globalscope::GlobalScope;
use crate::dom::haviadmin::HaviAdmin;
use crate::script_runtime::CanGc;

pub(crate) fn allow_havi_internal(global: &GlobalScope) -> bool {
    global.get_url().scheme() == "havi"
}

#[dom_struct]
pub(crate) struct HaviInternal {
    reflector_: Reflector,
    admin: MutNullableDom<HaviAdmin>,
}

impl HaviInternal {
    fn new_inherited() -> Self {
        Self {
            reflector_: Reflector::new(),
            admin: MutNullableDom::new(None),
        }
    }

    pub(crate) fn new(global: &GlobalScope, can_gc: CanGc) -> DomRoot<Self> {
        reflect_dom_object(Box::new(Self::new_inherited()), global, can_gc)
    }
}

impl HaviInternalMethods<crate::DomTypeHolder> for HaviInternal {
    fn GetAdmin(&self) -> Option<DomRoot<HaviAdmin>> {
        let global = self.global();
        let window = global.as_window();
        let (ring1_name, token) = window.Document().admin_credentials()?;
        Some(
            self.admin
                .or_init(|| HaviAdmin::new(&global, &ring1_name, &token, CanGc::note())),
        )
    }
}
