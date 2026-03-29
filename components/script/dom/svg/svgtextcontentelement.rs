/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use html5ever::{LocalName, Prefix, local_name};

use crate::dom::bindings::codegen::Bindings::DOMPointBinding::DOMPointInit;
use crate::dom::bindings::codegen::Bindings::SVGTextContentElementBinding::SVGTextContentElementMethods;
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::num::Finite;
use crate::dom::bindings::reflector::DomGlobal;
use crate::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::dom::document::Document;
use crate::dom::dompoint::DOMPoint;
use crate::dom::domrect::DOMRect;
use crate::dom::svg::svganimatedvalueobjects::{SVGAnimatedEnumeration, SVGAnimatedLength};
use crate::dom::svg::svggraphicselement::SVGGraphicsElement;

const LENGTH_ADJUST_VALUES: &[(&str, u16)] = &[("spacing", 1), ("spacingAndGlyphs", 2)];

#[dom_struct]
pub(crate) struct SVGTextContentElement {
    svggraphicselement: SVGGraphicsElement,
    text_length: MutNullableDom<SVGAnimatedLength>,
    length_adjust: MutNullableDom<SVGAnimatedEnumeration>,
}

impl SVGTextContentElement {
    pub(crate) fn new_inherited(local_name: LocalName, prefix: Option<Prefix>, document: &Document) -> Self {
        Self {
            svggraphicselement: SVGGraphicsElement::new_inherited(local_name, prefix, document),
            text_length: Default::default(),
            length_adjust: Default::default(),
        }
    }
}

impl SVGTextContentElementMethods<crate::DomTypeHolder> for SVGTextContentElement {
    fn TextLength(&self) -> DomRoot<SVGAnimatedLength> {
        self.text_length.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("textLength"), crate::script_runtime::CanGc::note()))
    }

    fn LengthAdjust(&self) -> DomRoot<SVGAnimatedEnumeration> {
        self.length_adjust.or_init(|| SVGAnimatedEnumeration::new(&self.global(), self.upcast(), local_name!("lengthAdjust"), LENGTH_ADJUST_VALUES, crate::script_runtime::CanGc::note()))
    }

    fn GetNumberOfChars(&self) -> i32 { 0 }
    fn GetComputedTextLength(&self) -> Finite<f32> { Finite::wrap(0.0) }
    fn GetSubStringLength(&self, _charnum: u32, _nchars: u32) -> Finite<f32> { Finite::wrap(0.0) }
    fn GetStartPositionOfChar(&self, _charnum: u32) -> DomRoot<DOMPoint> {
        DOMPoint::new(&self.global(), 0.0, 0.0, 0.0, 1.0, crate::script_runtime::CanGc::note())
    }
    fn GetEndPositionOfChar(&self, _charnum: u32) -> DomRoot<DOMPoint> {
        DOMPoint::new(&self.global(), 0.0, 0.0, 0.0, 1.0, crate::script_runtime::CanGc::note())
    }
    fn GetExtentOfChar(&self, _charnum: u32) -> DomRoot<DOMRect> {
        DOMRect::new(&self.global(), 0.0, 0.0, 0.0, 0.0, crate::script_runtime::CanGc::note())
    }
    fn GetRotationOfChar(&self, _charnum: u32) -> Finite<f32> { Finite::wrap(0.0) }
    fn GetCharNumAtPosition(&self, _point: &DOMPointInit) -> i32 { -1 }
    fn SelectSubString(&self, _charnum: u32, _nchars: u32) {}
}
