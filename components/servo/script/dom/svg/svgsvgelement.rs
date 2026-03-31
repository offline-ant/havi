/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use cssparser::{Parser, ParserInput};
use dom_struct::dom_struct;
use html5ever::{LocalName, Prefix, local_name};
use js::context::JSContext;
use js::rust::HandleObject;
use stylo_atoms::Atom;
use style::attr::AttrValue;
use style::parser::ParserContext;
use style::stylesheets::Origin;
use style::values::specified::Length;
use style_traits::ParsingMode;

use crate::script::dom::attr::Attr;
use crate::script::dom::bindings::codegen::Bindings::DOMMatrixBinding::DOMMatrix2DInit;
use crate::script::dom::bindings::codegen::GenericBindings::SVGSVGElementBinding::SVGSVGElementMethods;
use crate::script::dom::bindings::codegen::GenericBindings::SVGTransformBinding::SVGTransformMethods;
use crate::script::dom::bindings::inheritance::Castable;
use crate::script::dom::bindings::reflector::DomGlobal;
use crate::script::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::script::dom::bindings::str::DOMString;
use crate::script::dom::document::Document;
use crate::script::dom::dommatrix::DOMMatrix;
use crate::script::dom::dompoint::DOMPoint;
use crate::script::dom::domrect::DOMRect;
use crate::script::dom::element::{AttributeMutation, Element};
use crate::script::dom::node::{ChildrenMutation, Node, NodeDamage, NodeTraits, UnbindContext};
use crate::script::dom::svg::svganimatedvalueobjects::{
    SVGAnimatedLength, SVGAnimatedPreserveAspectRatio, SVGAnimatedRect,
};
use crate::script::dom::svg::svggraphicselement::SVGGraphicsElement;
use crate::script::dom::svg::svgvalueobjects::{SVGLength, SVGNumber, SVGTransform};
use crate::script::dom::virtualmethods::VirtualMethods;
use crate::script::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct SVGSVGElement {
    svggraphicselement: SVGGraphicsElement,
    x: MutNullableDom<SVGAnimatedLength>,
    y: MutNullableDom<SVGAnimatedLength>,
    width: MutNullableDom<SVGAnimatedLength>,
    height: MutNullableDom<SVGAnimatedLength>,
    view_box: MutNullableDom<SVGAnimatedRect>,
    preserve_aspect_ratio: MutNullableDom<SVGAnimatedPreserveAspectRatio>,
    current_translate: MutNullableDom<DOMPoint>,
}

impl SVGSVGElement {
    fn new_inherited(
        local_name: LocalName,
        prefix: Option<Prefix>,
        document: &Document,
    ) -> SVGSVGElement {
        SVGSVGElement {
            svggraphicselement: SVGGraphicsElement::new_inherited(local_name, prefix, document),
            x: Default::default(),
            y: Default::default(),
            width: Default::default(),
            height: Default::default(),
            view_box: Default::default(),
            preserve_aspect_ratio: Default::default(),
            current_translate: Default::default(),
        }
    }

    #[cfg_attr(crown, allow(crown::unrooted_must_root))]
    pub(crate) fn new(
        local_name: LocalName,
        prefix: Option<Prefix>,
        document: &Document,
        proto: Option<HandleObject>,
        can_gc: CanGc,
    ) -> DomRoot<SVGSVGElement> {
        Node::reflect_node_with_proto(
            Box::new(SVGSVGElement::new_inherited(local_name, prefix, document)),
            document,
            proto,
            can_gc,
        )
    }

    pub(crate) fn invalidate_svg_subtree(&self) {
        self.upcast::<Node>().dirty(NodeDamage::Other);
    }
}

impl VirtualMethods for SVGSVGElement {
    fn super_type(&self) -> Option<&dyn VirtualMethods> {
        Some(self.upcast::<SVGGraphicsElement>() as &dyn VirtualMethods)
    }

    fn attribute_mutated(&self, attr: &Attr, mutation: AttributeMutation, can_gc: CanGc) {
        self.super_type()
            .unwrap()
            .attribute_mutated(attr, mutation, can_gc);
        self.invalidate_svg_subtree();
    }

    fn attribute_affects_presentational_hints(&self, attr: &Attr) -> bool {
        match attr.local_name() {
            &local_name!("width") | &local_name!("height") => true,
            _ => self
                .super_type()
                .unwrap()
                .attribute_affects_presentational_hints(attr),
        }
    }

    fn parse_plain_attribute(&self, name: &LocalName, value: DOMString) -> AttrValue {
        match *name {
            local_name!("width") | local_name!("height") => {
                let value = &value.str();
                let parser_input = &mut ParserInput::new(value);
                let parser = &mut Parser::new(parser_input);
                let doc = self.owner_document();
                let url = doc.url().into_url().into();
                let context = ParserContext::new(
                    Origin::Author,
                    &url,
                    None,
                    ParsingMode::ALLOW_UNITLESS_LENGTH,
                    doc.quirks_mode(),
                    Default::default(),
                    None,
                    None,
                );
                let val = Length::parse_quirky(
                    &context,
                    parser,
                    style::values::specified::AllowQuirks::Always,
                );
                AttrValue::Length(value.to_string(), val.ok())
            }
            _ => self
                .super_type()
                .unwrap()
                .parse_plain_attribute(name, value),
        }
    }

    fn children_changed(&self, cx: &mut JSContext, mutation: &ChildrenMutation) {
        if let Some(super_type) = self.super_type() {
            super_type.children_changed(cx, mutation);
        }
        self.invalidate_svg_subtree();
    }

    fn unbind_from_tree(&self, context: &UnbindContext<'_>, can_gc: CanGc) {
        if let Some(s) = self.super_type() {
            s.unbind_from_tree(context, can_gc);
        }
        self.invalidate_svg_subtree();
    }
}

impl SVGSVGElementMethods<crate::DomTypeHolder> for SVGSVGElement {
    fn X(&self) -> DomRoot<SVGAnimatedLength> {
        self.x.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("x"), CanGc::note()))
    }

    fn Y(&self) -> DomRoot<SVGAnimatedLength> {
        self.y.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("y"), CanGc::note()))
    }

    fn Width(&self) -> DomRoot<SVGAnimatedLength> {
        self.width.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("width"), CanGc::note()))
    }

    fn Height(&self) -> DomRoot<SVGAnimatedLength> {
        self.height.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("height"), CanGc::note()))
    }

    fn ViewBox(&self) -> DomRoot<SVGAnimatedRect> {
        self.view_box.or_init(|| SVGAnimatedRect::new(&self.global(), self.upcast(), local_name!("viewBox"), CanGc::note()))
    }

    fn PreserveAspectRatio(&self) -> DomRoot<SVGAnimatedPreserveAspectRatio> {
        self.preserve_aspect_ratio.or_init(|| SVGAnimatedPreserveAspectRatio::new(&self.global(), self.upcast(), local_name!("preserveAspectRatio"), CanGc::note()))
    }

    fn CurrentTranslate(&self) -> DomRoot<DOMPoint> {
        self.current_translate.or_init(|| DOMPoint::new(&self.global(), 0.0, 0.0, 0.0, 1.0, CanGc::note()))
    }

    fn CreateSVGNumber(&self) -> DomRoot<SVGNumber> {
        SVGNumber::new_detached(&self.global(), 0.0, CanGc::note())
    }

    fn CreateSVGLength(&self) -> DomRoot<SVGLength> {
        SVGLength::new_detached(&self.global(), Default::default(), CanGc::note())
    }

    fn CreateSVGPoint(&self) -> DomRoot<DOMPoint> {
        DOMPoint::new(&self.global(), 0.0, 0.0, 0.0, 1.0, CanGc::note())
    }

    fn CreateSVGMatrix(&self) -> DomRoot<DOMMatrix> {
        DOMMatrix::new(
            &self.global(),
            true,
            euclid::default::Transform3D::identity(),
            CanGc::note(),
        )
    }

    fn CreateSVGRect(&self) -> DomRoot<DOMRect> {
        DOMRect::new(&self.global(), 0.0, 0.0, 0.0, 0.0, CanGc::note())
    }

    fn CreateSVGTransform(&self) -> DomRoot<SVGTransform> {
        SVGTransform::new_detached(&self.global(), Default::default(), CanGc::note())
    }

    fn CreateSVGTransformFromMatrix(&self, matrix: &DOMMatrix2DInit) -> DomRoot<SVGTransform> {
        let transform = self.CreateSVGTransform();
        let _ = transform.SetMatrix(matrix);
        transform
    }

    fn GetElementById(&self, element_id: DOMString) -> Option<DomRoot<Element>> {
        self.owner_document().get_element_by_id(&Atom::from(element_id))
    }
}
