/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use html5ever::{LocalName, Prefix, local_name};
use js::rust::HandleObject;

use crate::script::dom::bindings::codegen::GenericBindings::SVGRectElementBinding::SVGRectElementMethods;
use crate::script::dom::bindings::inheritance::Castable;
use crate::script::dom::bindings::reflector::DomGlobal;
use crate::script::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::script::dom::document::Document;
use crate::script::dom::node::Node;
use crate::script::dom::svg::svganimatedvalueobjects::SVGAnimatedLength;
use crate::script::dom::svg::svggeometryelement::SVGGeometryElement;
use crate::script::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct SVGRectElement {
    svggeometryelement: SVGGeometryElement,
    x: MutNullableDom<SVGAnimatedLength>,
    y: MutNullableDom<SVGAnimatedLength>,
    width: MutNullableDom<SVGAnimatedLength>,
    height: MutNullableDom<SVGAnimatedLength>,
    rx: MutNullableDom<SVGAnimatedLength>,
    ry: MutNullableDom<SVGAnimatedLength>,
}

impl SVGRectElement {
    fn new_inherited(local_name: LocalName, prefix: Option<Prefix>, document: &Document) -> Self {
        Self {
            svggeometryelement: SVGGeometryElement::new_inherited(local_name, prefix, document),
            x: Default::default(),
            y: Default::default(),
            width: Default::default(),
            height: Default::default(),
            rx: Default::default(),
            ry: Default::default(),
        }
    }

    pub(crate) fn new(local_name: LocalName, prefix: Option<Prefix>, document: &Document, proto: Option<HandleObject>, can_gc: CanGc) -> DomRoot<Self> {
        Node::reflect_node_with_proto(Box::new(Self::new_inherited(local_name, prefix, document)), document, proto, can_gc)
    }
}

impl SVGRectElementMethods<crate::DomTypeHolder> for SVGRectElement {
    fn X(&self) -> DomRoot<SVGAnimatedLength> { self.x.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("x"), CanGc::note())) }
    fn Y(&self) -> DomRoot<SVGAnimatedLength> { self.y.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("y"), CanGc::note())) }
    fn Width(&self) -> DomRoot<SVGAnimatedLength> { self.width.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("width"), CanGc::note())) }
    fn Height(&self) -> DomRoot<SVGAnimatedLength> { self.height.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("height"), CanGc::note())) }
    fn Rx(&self) -> DomRoot<SVGAnimatedLength> { self.rx.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("rx"), CanGc::note())) }
    fn Ry(&self) -> DomRoot<SVGAnimatedLength> { self.ry.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("ry"), CanGc::note())) }
}
