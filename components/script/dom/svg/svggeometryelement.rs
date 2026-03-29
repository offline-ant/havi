/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use html5ever::{LocalName, Prefix, local_name};

use crate::dom::bindings::codegen::Bindings::DOMPointBinding::DOMPointInit;
use crate::dom::bindings::codegen::Bindings::SVGGeometryElementBinding::SVGGeometryElementMethods;
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::num::Finite;
use crate::dom::bindings::reflector::DomGlobal;
use crate::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::dom::document::Document;
use crate::dom::dompoint::DOMPoint;
use crate::dom::svg::svganimatedvalueobjects::SVGAnimatedNumber;
use crate::dom::svg::svggraphicselement::SVGGraphicsElement;
use crate::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct SVGGeometryElement {
    svggraphicselement: SVGGraphicsElement,
    path_length: MutNullableDom<SVGAnimatedNumber>,
}

impl SVGGeometryElement {
    pub(crate) fn new_inherited(
        local_name: LocalName,
        prefix: Option<Prefix>,
        document: &Document,
    ) -> SVGGeometryElement {
        SVGGeometryElement {
            svggraphicselement: SVGGraphicsElement::new_inherited(local_name, prefix, document),
            path_length: Default::default(),
        }
    }
}

impl SVGGeometryElementMethods<crate::DomTypeHolder> for SVGGeometryElement {
    fn PathLength(&self) -> DomRoot<SVGAnimatedNumber> {
        self.path_length.or_init(|| SVGAnimatedNumber::new(&self.global(), self.upcast(), local_name!("pathLength"), CanGc::note()))
    }

    fn IsPointInFill(&self, _point: &DOMPointInit) -> bool { false }

    fn IsPointInStroke(&self, _point: &DOMPointInit) -> bool { false }

    fn GetTotalLength(&self) -> Finite<f32> { Finite::wrap(0.0) }

    fn GetPointAtLength(&self, _distance: Finite<f32>) -> DomRoot<DOMPoint> {
        DOMPoint::new(&self.global(), 0.0, 0.0, 0.0, 1.0, CanGc::note())
    }
}
