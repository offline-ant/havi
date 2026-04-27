/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Committed HPPR document source descriptor.

use dom_struct::dom_struct;

use crate::dom::bindings::codegen::Bindings::HpprSourceBinding::HpprSourceMethods;
use crate::dom::bindings::reflector::{Reflector, reflect_dom_object};
use crate::dom::bindings::root::{Dom, DomRoot};
use crate::dom::bindings::str::DOMString;
use crate::dom::globalscope::GlobalScope;
use crate::dom::hpprclient::HpprClient;
use crate::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct HpprSource {
    reflector_: Reflector,
    client: Dom<HpprClient>,
    kind: String,
    authority: Option<String>,
}

impl HpprSource {
    fn new_inherited(client: &HpprClient, kind: String, authority: Option<String>) -> Self {
        Self {
            reflector_: Reflector::new(),
            client: Dom::from_ref(client),
            kind,
            authority,
        }
    }

    pub(crate) fn new(
        global: &GlobalScope,
        client: &HpprClient,
        kind: String,
        authority: Option<String>,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        reflect_dom_object(
            Box::new(Self::new_inherited(client, kind, authority)),
            global,
            can_gc,
        )
    }
}

impl HpprSourceMethods<crate::DomTypeHolder> for HpprSource {
    fn Client(&self) -> DomRoot<HpprClient> {
        DomRoot::from_ref(&self.client)
    }

    fn Kind(&self) -> DOMString {
        DOMString::from(self.kind.as_str())
    }

    fn GetAuthority(&self) -> Option<DOMString> {
        self.authority.as_deref().map(DOMString::from)
    }
}
