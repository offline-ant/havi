/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use html5ever::{LocalName, Prefix, local_name};
use js::rust::HandleObject;

use crate::dom::bindings::codegen::Bindings::SVGLinearGradientElementBinding::SVGLinearGradientElementMethods;
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::reflector::DomGlobal;
use crate::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::dom::document::Document;
use crate::dom::node::Node;
use crate::dom::svg::svganimatedvalueobjects::SVGAnimatedLength;
use crate::dom::svg::svggradientelement::SVGGradientElement;
use crate::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct SVGLinearGradientElement {
    svggradientelement: SVGGradientElement,
    x1: MutNullableDom<SVGAnimatedLength>,
    y1: MutNullableDom<SVGAnimatedLength>,
    x2: MutNullableDom<SVGAnimatedLength>,
    y2: MutNullableDom<SVGAnimatedLength>,
}

impl SVGLinearGradientElement {
    fn new_inherited(local_name: LocalName, prefix: Option<Prefix>, document: &Document) -> Self {
        Self {
            svggradientelement: SVGGradientElement::new_inherited(local_name, prefix, document),
            x1: Default::default(),
            y1: Default::default(),
            x2: Default::default(),
            y2: Default::default(),
        }
    }

    pub(crate) fn new(local_name: LocalName, prefix: Option<Prefix>, document: &Document, proto: Option<HandleObject>, can_gc: CanGc) -> DomRoot<Self> {
        Node::reflect_node_with_proto(Box::new(Self::new_inherited(local_name, prefix, document)), document, proto, can_gc)
    }
}

impl SVGLinearGradientElementMethods<crate::DomTypeHolder> for SVGLinearGradientElement {
    fn X1(&self) -> DomRoot<SVGAnimatedLength> { self.x1.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("x1"), CanGc::note())) }
    fn Y1(&self) -> DomRoot<SVGAnimatedLength> { self.y1.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("y1"), CanGc::note())) }
    fn X2(&self) -> DomRoot<SVGAnimatedLength> { self.x2.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("x2"), CanGc::note())) }
    fn Y2(&self) -> DomRoot<SVGAnimatedLength> { self.y2.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("y2"), CanGc::note())) }
}
