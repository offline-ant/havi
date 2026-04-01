/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use html5ever::{LocalName, Prefix, local_name};
use stylo_dom::ElementState;

use crate::dom::bindings::codegen::Bindings::SVGGradientElementBinding::SVGGradientElementMethods;
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::reflector::DomGlobal;
use crate::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::dom::document::Document;
use crate::dom::svg::svganimatedvalueobjects::{SVGAnimatedEnumeration, SVGAnimatedString, SVGAnimatedTransformList};
use crate::dom::svg::svgelement::SVGElement;

const GRADIENT_UNITS_VALUES: &[(&str, u16)] = &[("userSpaceOnUse", 1), ("objectBoundingBox", 2)];
const SPREAD_METHOD_VALUES: &[(&str, u16)] = &[("pad", 1), ("reflect", 2), ("repeat", 3)];

#[dom_struct]
pub(crate) struct SVGGradientElement {
    svgelement: SVGElement,
    href: MutNullableDom<SVGAnimatedString>,
    gradient_units: MutNullableDom<SVGAnimatedEnumeration>,
    gradient_transform: MutNullableDom<SVGAnimatedTransformList>,
    spread_method: MutNullableDom<SVGAnimatedEnumeration>,
}

impl SVGGradientElement {
    pub(crate) fn new_inherited(local_name: LocalName, prefix: Option<Prefix>, document: &Document) -> Self {
        Self {
            svgelement: SVGElement::new_inherited_with_state(ElementState::empty(), local_name, prefix, document),
            href: Default::default(),
            gradient_units: Default::default(),
            gradient_transform: Default::default(),
            spread_method: Default::default(),
        }
    }
}

impl SVGGradientElementMethods<crate::DomTypeHolder> for SVGGradientElement {
    fn Href(&self) -> DomRoot<SVGAnimatedString> {
        self.href.or_init(|| SVGAnimatedString::new(&self.global(), self.upcast(), local_name!("href"), crate::script_runtime::CanGc::note()))
    }

    fn GradientUnits(&self) -> DomRoot<SVGAnimatedEnumeration> {
        self.gradient_units.or_init(|| SVGAnimatedEnumeration::new(&self.global(), self.upcast(), local_name!("gradientUnits"), GRADIENT_UNITS_VALUES, crate::script_runtime::CanGc::note()))
    }

    fn GradientTransform(&self) -> DomRoot<SVGAnimatedTransformList> {
        self.gradient_transform.or_init(|| SVGAnimatedTransformList::new(&self.global(), self.upcast(), local_name!("gradientTransform"), crate::script_runtime::CanGc::note()))
    }

    fn SpreadMethod(&self) -> DomRoot<SVGAnimatedEnumeration> {
        self.spread_method.or_init(|| SVGAnimatedEnumeration::new(&self.global(), self.upcast(), local_name!("spreadMethod"), SPREAD_METHOD_VALUES, crate::script_runtime::CanGc::note()))
    }
}
