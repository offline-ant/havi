/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;

use crate::script::dom::bindings::codegen::GenericBindings::DebuggerAddDebuggeeEventBinding::PipelineIdMethods;
use crate::script::dom::bindings::reflector::{Reflector, reflect_dom_object};
use crate::script::dom::bindings::root::DomRoot;
use crate::script::dom::globalscope::GlobalScope;
use crate::script::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct PipelineId {
    reflector_: Reflector,
    #[no_trace]
    inner: base::id::PipelineId,
}

impl PipelineId {
    pub(crate) fn new(
        global: &GlobalScope,
        pipeline_id: base::id::PipelineId,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        reflect_dom_object(
            Box::new(Self {
                reflector_: Reflector::new(),
                inner: pipeline_id,
            }),
            global,
            can_gc,
        )
    }
}

impl PipelineIdMethods<crate::DomTypeHolder> for PipelineId {
    // check-tidy: no specs after this line
    fn NamespaceId(&self) -> u32 {
        self.inner.namespace_id.0
    }

    fn Index(&self) -> u32 {
        self.inner.index.0.get()
    }
}
