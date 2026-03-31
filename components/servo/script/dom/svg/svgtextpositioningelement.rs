/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use html5ever::{LocalName, Prefix, local_name};

use crate::script::dom::bindings::codegen::GenericBindings::SVGTextPositioningElementBinding::SVGTextPositioningElementMethods;
use crate::script::dom::bindings::inheritance::Castable;
use crate::script::dom::bindings::reflector::DomGlobal;
use crate::script::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::script::dom::document::Document;
use crate::script::dom::svg::svganimatedvalueobjects::{SVGAnimatedLengthList, SVGAnimatedNumberList};
use crate::script::dom::svg::svgtextcontentelement::SVGTextContentElement;

#[dom_struct]
pub(crate) struct SVGTextPositioningElement {
    svgtextcontentelement: SVGTextContentElement,
    x: MutNullableDom<SVGAnimatedLengthList>,
    y: MutNullableDom<SVGAnimatedLengthList>,
    dx: MutNullableDom<SVGAnimatedLengthList>,
    dy: MutNullableDom<SVGAnimatedLengthList>,
    rotate: MutNullableDom<SVGAnimatedNumberList>,
}

impl SVGTextPositioningElement {
    pub(crate) fn new_inherited(local_name: LocalName, prefix: Option<Prefix>, document: &Document) -> Self {
        Self {
            svgtextcontentelement: SVGTextContentElement::new_inherited(local_name, prefix, document),
            x: Default::default(),
            y: Default::default(),
            dx: Default::default(),
            dy: Default::default(),
            rotate: Default::default(),
        }
    }
}

impl SVGTextPositioningElementMethods<crate::DomTypeHolder> for SVGTextPositioningElement {
    fn X(&self) -> DomRoot<SVGAnimatedLengthList> {
        self.x.or_init(|| SVGAnimatedLengthList::new(&self.global(), self.upcast(), local_name!("x"), crate::script::script_runtime::CanGc::note()))
    }

    fn Y(&self) -> DomRoot<SVGAnimatedLengthList> {
        self.y.or_init(|| SVGAnimatedLengthList::new(&self.global(), self.upcast(), local_name!("y"), crate::script::script_runtime::CanGc::note()))
    }

    fn Dx(&self) -> DomRoot<SVGAnimatedLengthList> {
        self.dx.or_init(|| SVGAnimatedLengthList::new(&self.global(), self.upcast(), local_name!("dx"), crate::script::script_runtime::CanGc::note()))
    }

    fn Dy(&self) -> DomRoot<SVGAnimatedLengthList> {
        self.dy.or_init(|| SVGAnimatedLengthList::new(&self.global(), self.upcast(), local_name!("dy"), crate::script::script_runtime::CanGc::note()))
    }

    fn Rotate(&self) -> DomRoot<SVGAnimatedNumberList> {
        self.rotate.or_init(|| SVGAnimatedNumberList::new(&self.global(), self.upcast(), local_name!("rotate"), crate::script::script_runtime::CanGc::note()))
    }
}
