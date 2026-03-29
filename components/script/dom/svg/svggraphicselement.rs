/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use html5ever::{LocalName, Prefix, local_name};
use stylo_dom::ElementState;

use crate::dom::bindings::codegen::Bindings::SVGGraphicsElementBinding::{SVGBoundingBoxOptions, SVGGraphicsElementMethods};
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::reflector::DomGlobal;
use crate::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::dom::document::Document;
use crate::dom::dommatrix::DOMMatrix;
use crate::dom::domrect::DOMRect;
use crate::dom::svg::svganimatedvalueobjects::SVGAnimatedTransformList;
use crate::dom::svg::svgelement::SVGElement;
use crate::dom::svg::svgvalueobjects::svg_matrix_from_transform;
use crate::dom::virtualmethods::VirtualMethods;
use crate::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct SVGGraphicsElement {
    svgelement: SVGElement,
    transform: MutNullableDom<SVGAnimatedTransformList>,
}

impl SVGGraphicsElement {
    pub(crate) fn new_inherited(
        tag_name: LocalName,
        prefix: Option<Prefix>,
        document: &Document,
    ) -> SVGGraphicsElement {
        SVGGraphicsElement::new_inherited_with_state(
            ElementState::empty(),
            tag_name,
            prefix,
            document,
        )
    }

    pub(crate) fn new_inherited_with_state(
        state: ElementState,
        tag_name: LocalName,
        prefix: Option<Prefix>,
        document: &Document,
    ) -> SVGGraphicsElement {
        SVGGraphicsElement {
            svgelement: SVGElement::new_inherited_with_state(state, tag_name, prefix, document),
            transform: Default::default(),
        }
    }
}

impl VirtualMethods for SVGGraphicsElement {
    fn super_type(&self) -> Option<&dyn VirtualMethods> {
        Some(self.upcast::<SVGElement>() as &dyn VirtualMethods)
    }
}

impl SVGGraphicsElementMethods<crate::DomTypeHolder> for SVGGraphicsElement {
    fn Transform(&self) -> DomRoot<SVGAnimatedTransformList> {
        self.transform.or_init(|| SVGAnimatedTransformList::new(&self.global(), self.upcast(), local_name!("transform"), CanGc::note()))
    }

    fn GetBBox(&self, _options: &SVGBoundingBoxOptions) -> DomRoot<DOMRect> {
        DOMRect::new(&self.global(), 0.0, 0.0, 0.0, 0.0, CanGc::note())
    }

    fn GetCTM(&self) -> Option<DomRoot<DOMMatrix>> {
        Some(svg_matrix_from_transform(&self.global(), euclid::default::Transform2D::identity(), CanGc::note()))
    }

    fn GetScreenCTM(&self) -> Option<DomRoot<DOMMatrix>> {
        self.GetCTM()
    }
}
