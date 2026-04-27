/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HAVI internal admin capability descriptor.

use dom_struct::dom_struct;
use hppr_client::Signer;

use crate::dom::bindings::codegen::Bindings::HaviAdminBinding::HaviAdminMethods;
use crate::dom::bindings::reflector::{DomGlobal, Reflector, reflect_dom_object};
use crate::dom::bindings::root::{Dom, DomRoot, MutNullableDom};
use crate::dom::envelopehpprclient::default_endpoint;
use crate::dom::globalscope::GlobalScope;
use crate::dom::hpprclient::HpprClient;
use crate::dom::hpprrepoinfo::HpprRepoInfo;
use crate::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct HaviAdmin {
    reflector_: Reflector,
    client: Dom<HpprClient>,
    repo: MutNullableDom<HpprRepoInfo>,
}

impl HaviAdmin {
    fn new_inherited(client: &HpprClient) -> Self {
        Self {
            reflector_: Reflector::new(),
            client: Dom::from_ref(client),
            repo: MutNullableDom::new(None),
        }
    }

    pub(crate) fn new(
        global: &GlobalScope,
        ring1_name: &str,
        token: &str,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        let signer = Signer::ring1_adhoc(ring1_name, token);
        let client = HpprClient::new_with_signer(
            global,
            signer,
            default_endpoint(),
            None,
            can_gc,
        );
        reflect_dom_object(Box::new(Self::new_inherited(&client)), global, can_gc)
    }
}

impl HaviAdminMethods<crate::DomTypeHolder> for HaviAdmin {
    fn Client(&self) -> DomRoot<HpprClient> {
        DomRoot::from_ref(&self.client)
    }

    fn Repo(&self) -> DomRoot<HpprRepoInfo> {
        self.repo
            .or_init(|| HpprRepoInfo::new(&self.global(), CanGc::note()))
    }
}
