/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use html5ever::{LocalName, Prefix, local_name};
use js::rust::HandleObject;

use crate::dom::bindings::codegen::Bindings::SVGClipPathElementBinding::SVGClipPathElementMethods;
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::reflector::DomGlobal;
use crate::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::dom::document::Document;
use crate::dom::node::Node;
use crate::dom::svg::svganimatedvalueobjects::{SVGAnimatedEnumeration, SVGAnimatedTransformList};
use crate::dom::svg::svgelement::SVGElement;
use crate::script_runtime::CanGc;

const CLIP_PATH_UNITS_VALUES: &[(&str, u16)] = &[("userSpaceOnUse", 1), ("objectBoundingBox", 2)];

#[dom_struct]
pub(crate) struct SVGClipPathElement {
    svgelement: SVGElement,
    clip_path_units: MutNullableDom<SVGAnimatedEnumeration>,
    transform: MutNullableDom<SVGAnimatedTransformList>,
}

impl SVGClipPathElement {
    fn new_inherited(local_name: LocalName, prefix: Option<Prefix>, document: &Document) -> Self {
        Self {
            svgelement: SVGElement::new_inherited(local_name, prefix, document),
            clip_path_units: Default::default(),
            transform: Default::default(),
        }
    }

    pub(crate) fn new(local_name: LocalName, prefix: Option<Prefix>, document: &Document, proto: Option<HandleObject>, can_gc: CanGc) -> DomRoot<Self> {
        Node::reflect_node_with_proto(Box::new(Self::new_inherited(local_name, prefix, document)), document, proto, can_gc)
    }
}

impl SVGClipPathElementMethods<crate::DomTypeHolder> for SVGClipPathElement {
    fn ClipPathUnits(&self) -> DomRoot<SVGAnimatedEnumeration> {
        self.clip_path_units.or_init(|| SVGAnimatedEnumeration::new(&self.global(), self.upcast(), local_name!("clipPathUnits"), CLIP_PATH_UNITS_VALUES, CanGc::note()))
    }

    fn Transform(&self) -> DomRoot<SVGAnimatedTransformList> {
        self.transform.or_init(|| SVGAnimatedTransformList::new(&self.global(), self.upcast(), local_name!("transform"), CanGc::note()))
    }
}
