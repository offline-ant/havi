/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use html5ever::{LocalName, Prefix, local_name};
use js::rust::HandleObject;

use crate::script::dom::bindings::codegen::GenericBindings::SVGRadialGradientElementBinding::SVGRadialGradientElementMethods;
use crate::script::dom::bindings::inheritance::Castable;
use crate::script::dom::bindings::reflector::DomGlobal;
use crate::script::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::script::dom::document::Document;
use crate::script::dom::node::Node;
use crate::script::dom::svg::svganimatedvalueobjects::SVGAnimatedLength;
use crate::script::dom::svg::svggradientelement::SVGGradientElement;
use crate::script::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct SVGRadialGradientElement {
    svggradientelement: SVGGradientElement,
    cx: MutNullableDom<SVGAnimatedLength>,
    cy: MutNullableDom<SVGAnimatedLength>,
    r: MutNullableDom<SVGAnimatedLength>,
    fx: MutNullableDom<SVGAnimatedLength>,
    fy: MutNullableDom<SVGAnimatedLength>,
    fr: MutNullableDom<SVGAnimatedLength>,
}

impl SVGRadialGradientElement {
    fn new_inherited(local_name: LocalName, prefix: Option<Prefix>, document: &Document) -> Self {
        Self {
            svggradientelement: SVGGradientElement::new_inherited(local_name, prefix, document),
            cx: Default::default(),
            cy: Default::default(),
            r: Default::default(),
            fx: Default::default(),
            fy: Default::default(),
            fr: Default::default(),
        }
    }

    pub(crate) fn new(local_name: LocalName, prefix: Option<Prefix>, document: &Document, proto: Option<HandleObject>, can_gc: CanGc) -> DomRoot<Self> {
        Node::reflect_node_with_proto(Box::new(Self::new_inherited(local_name, prefix, document)), document, proto, can_gc)
    }
}

impl SVGRadialGradientElementMethods<crate::DomTypeHolder> for SVGRadialGradientElement {
    fn Cx(&self) -> DomRoot<SVGAnimatedLength> { self.cx.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("cx"), CanGc::note())) }
    fn Cy(&self) -> DomRoot<SVGAnimatedLength> { self.cy.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("cy"), CanGc::note())) }
    fn R(&self) -> DomRoot<SVGAnimatedLength> { self.r.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("r"), CanGc::note())) }
    fn Fx(&self) -> DomRoot<SVGAnimatedLength> { self.fx.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("fx"), CanGc::note())) }
    fn Fy(&self) -> DomRoot<SVGAnimatedLength> { self.fy.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("fy"), CanGc::note())) }
    fn Fr(&self) -> DomRoot<SVGAnimatedLength> { self.fr.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("fr"), CanGc::note())) }
}
