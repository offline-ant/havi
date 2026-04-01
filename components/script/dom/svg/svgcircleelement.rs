/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use html5ever::{LocalName, Prefix, local_name};
use js::rust::HandleObject;

use crate::dom::bindings::codegen::Bindings::SVGCircleElementBinding::SVGCircleElementMethods;
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::reflector::DomGlobal;
use crate::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::dom::document::Document;
use crate::dom::node::Node;
use crate::dom::svg::svganimatedvalueobjects::SVGAnimatedLength;
use crate::dom::svg::svggeometryelement::SVGGeometryElement;
use crate::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct SVGCircleElement {
    svggeometryelement: SVGGeometryElement,
    cx: MutNullableDom<SVGAnimatedLength>,
    cy: MutNullableDom<SVGAnimatedLength>,
    r: MutNullableDom<SVGAnimatedLength>,
}

impl SVGCircleElement {
    fn new_inherited(local_name: LocalName, prefix: Option<Prefix>, document: &Document) -> Self {
        Self {
            svggeometryelement: SVGGeometryElement::new_inherited(local_name, prefix, document),
            cx: Default::default(),
            cy: Default::default(),
            r: Default::default(),
        }
    }

    pub(crate) fn new(local_name: LocalName, prefix: Option<Prefix>, document: &Document, proto: Option<HandleObject>, can_gc: CanGc) -> DomRoot<Self> {
        Node::reflect_node_with_proto(Box::new(Self::new_inherited(local_name, prefix, document)), document, proto, can_gc)
    }
}

impl SVGCircleElementMethods<crate::DomTypeHolder> for SVGCircleElement {
    fn Cx(&self) -> DomRoot<SVGAnimatedLength> { self.cx.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("cx"), CanGc::note())) }
    fn Cy(&self) -> DomRoot<SVGAnimatedLength> { self.cy.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("cy"), CanGc::note())) }
    fn R(&self) -> DomRoot<SVGAnimatedLength> { self.r.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("r"), CanGc::note())) }
}
