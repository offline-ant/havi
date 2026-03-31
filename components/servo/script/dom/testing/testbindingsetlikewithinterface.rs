/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// check-tidy: no specs after this line

use dom_struct::dom_struct;
use indexmap::IndexSet;
use js::rust::HandleObject;

use crate::script::dom::bindings::cell::DomRefCell;
use crate::script::dom::bindings::codegen::GenericBindings::TestBindingSetlikeWithInterfaceBinding::TestBindingSetlikeWithInterfaceMethods;
use crate::script::dom::bindings::error::Fallible;
use crate::script::dom::bindings::like::Setlike;
use crate::script::dom::bindings::reflector::{Reflector, reflect_dom_object_with_proto};
use crate::script::dom::bindings::root::DomRoot;
use crate::script::dom::globalscope::GlobalScope;
use crate::script::dom::testbinding::TestBinding;
use crate::script::script_runtime::CanGc;
use crate::setlike;

// setlike<TestBinding>
#[dom_struct]
pub(crate) struct TestBindingSetlikeWithInterface {
    reflector: Reflector,
    #[custom_trace]
    internal: DomRefCell<IndexSet<DomRoot<TestBinding>>>,
}

impl TestBindingSetlikeWithInterface {
    fn new(
        global: &GlobalScope,
        proto: Option<HandleObject>,
        can_gc: CanGc,
    ) -> DomRoot<TestBindingSetlikeWithInterface> {
        reflect_dom_object_with_proto(
            Box::new(TestBindingSetlikeWithInterface {
                reflector: Reflector::new(),
                internal: DomRefCell::new(IndexSet::new()),
            }),
            global,
            proto,
            can_gc,
        )
    }
}

impl TestBindingSetlikeWithInterfaceMethods<crate::DomTypeHolder>
    for TestBindingSetlikeWithInterface
{
    fn Constructor(
        global: &GlobalScope,
        proto: Option<HandleObject>,
        can_gc: CanGc,
    ) -> Fallible<DomRoot<TestBindingSetlikeWithInterface>> {
        Ok(TestBindingSetlikeWithInterface::new(global, proto, can_gc))
    }

    fn Size(&self) -> u32 {
        self.internal.size()
    }
}

impl Setlike for TestBindingSetlikeWithInterface {
    type Key = DomRoot<TestBinding>;

    setlike!(self, internal);
}
