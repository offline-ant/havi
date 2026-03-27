/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use cssparser::{Parser, ParserInput};
use dom_struct::dom_struct;
use html5ever::{LocalName, Prefix, local_name};
use js::context::JSContext;
use js::rust::HandleObject;
use style::attr::AttrValue;
use style::parser::ParserContext;
use style::stylesheets::Origin;
use style::values::specified::Length;
use style_traits::ParsingMode;

use crate::dom::attr::Attr;
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::root::DomRoot;
use crate::dom::bindings::str::DOMString;
use crate::dom::document::Document;
use crate::dom::element::AttributeMutation;
use crate::dom::node::{ChildrenMutation, Node, NodeDamage, NodeTraits, UnbindContext};
use crate::dom::svg::svggraphicselement::SVGGraphicsElement;
use crate::dom::virtualmethods::VirtualMethods;
use crate::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct SVGSVGElement {
    svggraphicselement: SVGGraphicsElement,
}

impl SVGSVGElement {
    fn new_inherited(
        local_name: LocalName,
        prefix: Option<Prefix>,
        document: &Document,
    ) -> SVGSVGElement {
        SVGSVGElement {
            svggraphicselement: SVGGraphicsElement::new_inherited(local_name, prefix, document),
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
                    /* namespaces = */ Default::default(),
                    None,
                    None,
                );
                let val = Length::parse_quirky(
                    &context,
                    parser,
                    style::values::specified::AllowQuirks::Always,
                );
                AttrValue::Length(value.to_string(), val.ok())
            },
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
