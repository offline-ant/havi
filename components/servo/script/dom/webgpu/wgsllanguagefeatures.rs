/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// check-tidy: no specs after this line

use dom_struct::dom_struct;
use indexmap::IndexSet;
use js::rust::HandleObject;
use wgpu_core::naga::front::wgsl::ImplementedLanguageExtension;

use crate::script::dom::bindings::cell::DomRefCell;
use crate::script::dom::bindings::codegen::GenericBindings::WebGPUBinding::WGSLLanguageFeaturesMethods;
use crate::script::dom::bindings::like::Setlike;
use crate::script::dom::bindings::reflector::{Reflector, reflect_dom_object_with_proto};
use crate::script::dom::bindings::root::DomRoot;
use crate::script::dom::bindings::str::DOMString;
use crate::script::dom::globalscope::GlobalScope;
use crate::script::script_runtime::CanGc;

#[dom_struct]
pub struct WGSLLanguageFeatures {
    reflector: Reflector,
    // internal storage for features
    #[custom_trace]
    internal: DomRefCell<IndexSet<DOMString>>,
}

impl WGSLLanguageFeatures {
    pub(crate) fn new(
        global: &GlobalScope,
        proto: Option<HandleObject>,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        let set = ImplementedLanguageExtension::all()
            .iter()
            .map(|le| le.to_ident().into())
            .collect();
        reflect_dom_object_with_proto(
            Box::new(Self {
                reflector: Reflector::new(),
                internal: DomRefCell::new(set),
            }),
            global,
            proto,
            can_gc,
        )
    }
}

impl WGSLLanguageFeaturesMethods<crate::DomTypeHolder> for WGSLLanguageFeatures {
    fn Size(&self) -> u32 {
        self.internal.size()
    }
}

impl Setlike for WGSLLanguageFeatures {
    type Key = DOMString;

    #[inline(always)]
    fn get_index(&self, index: u32) -> Option<Self::Key> {
        self.internal.get_index(index)
    }
    #[inline(always)]
    fn size(&self) -> u32 {
        self.internal.size()
    }
    #[inline(always)]
    fn add(&self, _key: Self::Key) {
        unreachable!("readonly");
    }
    #[inline(always)]
    fn has(&self, key: Self::Key) -> bool {
        self.internal.has(key)
    }
    #[inline(always)]
    fn clear(&self) {
        unreachable!("readonly");
    }
    #[inline(always)]
    fn delete(&self, _key: Self::Key) -> bool {
        unreachable!("readonly");
    }
}
