/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use html5ever::{LocalName, Prefix, local_name, ns};
use js::rust::HandleObject;
use style::attr::AttrValue;

use crate::dom::attr::Attr;
use crate::dom::bindings::codegen::Bindings::SVGImageElementBinding::SVGImageElementMethods;
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::reflector::DomGlobal;
use crate::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::dom::bindings::str::DOMString;
use crate::dom::document::Document;
use crate::dom::element::AttributeMutation;
use crate::dom::node::{Node, NodeTraits};
use crate::dom::svg::svganimatedvalueobjects::{SVGAnimatedLength, SVGAnimatedPreserveAspectRatio, SVGAnimatedString};
use crate::dom::svg::svggraphicselement::SVGGraphicsElement;
use crate::dom::virtualmethods::VirtualMethods;
use crate::script_runtime::CanGc;

const DEFAULT_WIDTH: u32 = 300;
const DEFAULT_HEIGHT: u32 = 150;

#[dom_struct]
pub(crate) struct SVGImageElement {
    svggraphicselement: SVGGraphicsElement,
    href: MutNullableDom<SVGAnimatedString>,
    x: MutNullableDom<SVGAnimatedLength>,
    y: MutNullableDom<SVGAnimatedLength>,
    width: MutNullableDom<SVGAnimatedLength>,
    height: MutNullableDom<SVGAnimatedLength>,
    preserve_aspect_ratio: MutNullableDom<SVGAnimatedPreserveAspectRatio>,
}

impl SVGImageElement {
    fn new_inherited(local_name: LocalName, prefix: Option<Prefix>, document: &Document) -> Self {
        Self {
            svggraphicselement: SVGGraphicsElement::new_inherited(local_name, prefix, document),
            href: Default::default(),
            x: Default::default(),
            y: Default::default(),
            width: Default::default(),
            height: Default::default(),
            preserve_aspect_ratio: Default::default(),
        }
    }

    pub(crate) fn new(local_name: LocalName, prefix: Option<Prefix>, document: &Document, proto: Option<HandleObject>, can_gc: CanGc) -> DomRoot<Self> {
        Node::reflect_node_with_proto(Box::new(Self::new_inherited(local_name, prefix, document)), document, proto, can_gc)
    }

    fn fetch_image_resource(&self) {
        self.owner_global()
            .task_manager()
            .dom_manipulation_task_source()
            .queue_simple_event(self.upcast(), atom!("error"));
    }
}

impl VirtualMethods for SVGImageElement {
    fn super_type(&self) -> Option<&dyn VirtualMethods> {
        Some(self.upcast::<SVGGraphicsElement>() as &dyn VirtualMethods)
    }

    fn attribute_mutated(&self, attr: &Attr, mutation: AttributeMutation, can_gc: CanGc) {
        self.super_type().unwrap().attribute_mutated(attr, mutation, can_gc);
        if attr.local_name() == &local_name!("href") && matches!(attr.namespace(), &ns!() | &ns!(xlink)) {
            if let AttributeMutation::Set(..) = mutation {
                self.fetch_image_resource();
            }
        }
    }

    fn attribute_affects_presentational_hints(&self, attr: &Attr) -> bool {
        match attr.local_name() {
            &local_name!("width") | &local_name!("height") => true,
            _ => self.super_type().unwrap().attribute_affects_presentational_hints(attr),
        }
    }

    fn parse_plain_attribute(&self, name: &LocalName, value: DOMString) -> AttrValue {
        match *name {
            local_name!("width") => AttrValue::from_u32(value.into(), DEFAULT_WIDTH),
            local_name!("height") => AttrValue::from_u32(value.into(), DEFAULT_HEIGHT),
            _ => self.super_type().unwrap().parse_plain_attribute(name, value),
        }
    }
}

impl SVGImageElementMethods<crate::DomTypeHolder> for SVGImageElement {
    fn Href(&self) -> DomRoot<SVGAnimatedString> { self.href.or_init(|| SVGAnimatedString::new(&self.global(), self.upcast(), local_name!("href"), CanGc::note())) }
    fn X(&self) -> DomRoot<SVGAnimatedLength> { self.x.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("x"), CanGc::note())) }
    fn Y(&self) -> DomRoot<SVGAnimatedLength> { self.y.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("y"), CanGc::note())) }
    fn Width(&self) -> DomRoot<SVGAnimatedLength> { self.width.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("width"), CanGc::note())) }
    fn Height(&self) -> DomRoot<SVGAnimatedLength> { self.height.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("height"), CanGc::note())) }
    fn PreserveAspectRatio(&self) -> DomRoot<SVGAnimatedPreserveAspectRatio> { self.preserve_aspect_ratio.or_init(|| SVGAnimatedPreserveAspectRatio::new(&self.global(), self.upcast(), local_name!("preserveAspectRatio"), CanGc::note())) }
}
