/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR resolve result DOM binding.

use dom_struct::dom_struct;

use crate::script::dom::bindings::codegen::GenericBindings::HpprResolveResultBinding::HpprResolveResultMethods;
use crate::script::dom::bindings::reflector::{Reflector, reflect_dom_object};
use crate::script::dom::bindings::root::{Dom, DomRoot};
use crate::script::dom::bindings::str::DOMString;
use crate::script::dom::globalscope::GlobalScope;
use crate::script::dom::hpprpacket::HpprPacket;
use crate::script::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct HpprResolveResult {
    reflector_: Reflector,
    packet: Dom<HpprPacket>,
    endpoint: String,
    signer: Option<String>,
    content_authority: Option<String>,
    is_repo: bool,
}

impl HpprResolveResult {
    fn new_inherited(
        packet: &HpprPacket,
        endpoint: String,
        signer: Option<String>,
        content_authority: Option<String>,
        is_repo: bool,
    ) -> Self {
        Self {
            reflector_: Reflector::new(),
            packet: Dom::from_ref(packet),
            endpoint,
            signer,
            content_authority,
            is_repo,
        }
    }

    pub(crate) fn new(
        global: &GlobalScope,
        packet: &HpprPacket,
        endpoint: String,
        signer: Option<String>,
        content_authority: Option<String>,
        is_repo: bool,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        reflect_dom_object(
            Box::new(Self::new_inherited(packet, endpoint, signer, content_authority, is_repo)),
            global,
            can_gc,
        )
    }
}

impl HpprResolveResultMethods<crate::DomTypeHolder> for HpprResolveResult {
    fn Packet(&self) -> DomRoot<HpprPacket> {
        DomRoot::from_ref(&self.packet)
    }

    fn Endpoint(&self) -> DOMString {
        DOMString::from(self.endpoint.as_str())
    }

    fn GetSigner(&self) -> Option<DOMString> {
        self.signer.as_deref().map(DOMString::from)
    }

    fn GetContentAuthority(&self) -> Option<DOMString> {
        self.content_authority.as_deref().map(DOMString::from)
    }

    fn IsRepo(&self) -> bool {
        self.is_repo
    }
}
