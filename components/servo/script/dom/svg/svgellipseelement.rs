/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use html5ever::{LocalName, Prefix, local_name};
use js::rust::HandleObject;

use crate::script::dom::bindings::codegen::GenericBindings::SVGEllipseElementBinding::SVGEllipseElementMethods;
use crate::script::dom::bindings::inheritance::Castable;
use crate::script::dom::bindings::reflector::DomGlobal;
use crate::script::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::script::dom::document::Document;
use crate::script::dom::node::Node;
use crate::script::dom::svg::svganimatedvalueobjects::SVGAnimatedLength;
use crate::script::dom::svg::svggeometryelement::SVGGeometryElement;
use crate::script::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct SVGEllipseElement {
    svggeometryelement: SVGGeometryElement,
    cx: MutNullableDom<SVGAnimatedLength>,
    cy: MutNullableDom<SVGAnimatedLength>,
    rx: MutNullableDom<SVGAnimatedLength>,
    ry: MutNullableDom<SVGAnimatedLength>,
}

impl SVGEllipseElement {
    fn new_inherited(local_name: LocalName, prefix: Option<Prefix>, document: &Document) -> Self {
        Self {
            svggeometryelement: SVGGeometryElement::new_inherited(local_name, prefix, document),
            cx: Default::default(),
            cy: Default::default(),
            rx: Default::default(),
            ry: Default::default(),
        }
    }

    pub(crate) fn new(local_name: LocalName, prefix: Option<Prefix>, document: &Document, proto: Option<HandleObject>, can_gc: CanGc) -> DomRoot<Self> {
        Node::reflect_node_with_proto(Box::new(Self::new_inherited(local_name, prefix, document)), document, proto, can_gc)
    }
}

impl SVGEllipseElementMethods<crate::DomTypeHolder> for SVGEllipseElement {
    fn Cx(&self) -> DomRoot<SVGAnimatedLength> { self.cx.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("cx"), CanGc::note())) }
    fn Cy(&self) -> DomRoot<SVGAnimatedLength> { self.cy.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("cy"), CanGc::note())) }
    fn Rx(&self) -> DomRoot<SVGAnimatedLength> { self.rx.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("rx"), CanGc::note())) }
    fn Ry(&self) -> DomRoot<SVGAnimatedLength> { self.ry.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("ry"), CanGc::note())) }
}
