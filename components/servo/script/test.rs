/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// For compile-fail tests only.
// pub use crate::script::dom::bindings::cell::DomRefCell;
pub use crate::script::dom::bindings::refcounted::TrustedPromise;
// pub use crate::script::dom::bindings::root::Dom;
pub use crate::script::dom::bindings::str::{ByteString, DOMString};
// pub use crate::script::dom::node::Node;

pub mod area {
    pub use crate::script::dom::html::htmlareaelement::{Area, Shape};
}

#[expect(non_snake_case)]
pub mod size_of {
    use std::mem::size_of;

    use crate::script::dom::characterdata::CharacterData;
    use crate::script::dom::element::Element;
    use crate::script::dom::eventtarget::EventTarget;
    use crate::script::dom::html::htmldivelement::HTMLDivElement;
    use crate::script::dom::html::htmlelement::HTMLElement;
    use crate::script::dom::html::htmlspanelement::HTMLSpanElement;
    use crate::script::dom::node::Node;
    use crate::script::dom::text::Text;

    pub fn CharacterData() -> usize {
        size_of::<CharacterData>()
    }

    pub fn Element() -> usize {
        size_of::<Element>()
    }

    pub fn EventTarget() -> usize {
        size_of::<EventTarget>()
    }

    pub fn HTMLDivElement() -> usize {
        size_of::<HTMLDivElement>()
    }

    pub fn HTMLElement() -> usize {
        size_of::<HTMLElement>()
    }

    pub fn HTMLSpanElement() -> usize {
        size_of::<HTMLSpanElement>()
    }

    pub fn Node() -> usize {
        size_of::<Node>()
    }

    pub fn Text() -> usize {
        size_of::<Text>()
    }
}

pub mod srcset {
    pub use crate::script::dom::html::htmlimageelement::{
        Descriptor, ImageSource, parse_a_srcset_attribute,
    };
}

pub mod timeranges {
    pub use crate::script::dom::timeranges::TimeRangesContainer;
}

pub mod textinput {
    pub use crate::script::clipboard_provider::ClipboardProvider;
    pub use crate::script::textinput::{Direction, SelectionDirection, TextInput};
}

pub mod encoding_detection {
    pub use crate::script::dom::servoparser::encoding::{
        get_xml_encoding, prescan_the_byte_stream_to_determine_the_encoding,
    };
}
