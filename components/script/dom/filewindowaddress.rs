/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use servo_url::BrowserUrl;

use crate::dom::bindings::codegen::Bindings::FileWindowAddressBinding::FileWindowAddressMethods;
use crate::dom::bindings::error::{Error, ErrorResult, Fallible};
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::reflector::reflect_dom_object;
use crate::dom::bindings::root::DomRoot;
use crate::dom::bindings::str::DOMString;
use crate::dom::globalscope::GlobalScope;
use crate::dom::window::Window;
use crate::dom::windowaddress::WindowAddress;
use crate::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct FileWindowAddress {
    windowaddress: WindowAddress,
}

impl FileWindowAddress {
    fn new_inherited(window: &Window) -> Self {
        Self {
            windowaddress: WindowAddress::new_inherited(window),
        }
    }

    pub(crate) fn new(window: &Window, can_gc: CanGc) -> DomRoot<Self> {
        reflect_dom_object(Box::new(Self::new_inherited(window)), window.upcast::<GlobalScope>(), can_gc)
    }
}

impl FileWindowAddressMethods<crate::DomTypeHolder> for FileWindowAddress {
    fn GetPathname(&self) -> Fallible<DOMString> {
        match self.upcast::<WindowAddress>().current_url() {
            BrowserUrl::FileDocument(data) => Ok(DOMString::from(data.document_url().path())),
            _ => Err(Error::InvalidState(None)),
        }
    }

    fn SetPathname(&self, value: DOMString) -> ErrorResult {
        let url = self.upcast::<WindowAddress>().current_url();
        let BrowserUrl::FileDocument(data) = url else {
            return Err(Error::InvalidState(None));
        };

        let mut document_url = data.document_url().clone();
        let pathname = value.to_string();
        document_url.set_path(&pathname);
        let href = format!("{}{}", document_url.as_str(), data.jsonqa());
        self.upcast::<WindowAddress>()
            .navigate_to_href(&href, CanGc::note())
    }
}
