/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use html5ever::{LocalName, Prefix, local_name};
use layout_api::SVGBoundingBoxOptionsData;
use stylo_dom::ElementState;

use crate::dom::bindings::codegen::Bindings::SVGGraphicsElementBinding::{SVGBoundingBoxOptions, SVGGraphicsElementMethods};
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::reflector::DomGlobal;
use crate::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::dom::document::Document;
use crate::dom::dommatrix::DOMMatrix;
use crate::dom::domrect::DOMRect;
use crate::dom::node::{Node, NodeTraits};
use crate::dom::svg::svganimatedvalueobjects::SVGAnimatedTransformList;
use crate::dom::svg::svgelement::SVGElement;
use crate::dom::svg::svgtextcontentelement::SVGTextContentElement;
use crate::dom::svg::svgvalueobjects::svg_matrix_from_transform;
use crate::dom::virtualmethods::VirtualMethods;
use crate::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct SVGGraphicsElement {
    svgelement: SVGElement,
    transform: MutNullableDom<SVGAnimatedTransformList>,
}

impl SVGGraphicsElement {
    pub(crate) fn new_inherited(
        tag_name: LocalName,
        prefix: Option<Prefix>,
        document: &Document,
    ) -> SVGGraphicsElement {
        SVGGraphicsElement::new_inherited_with_state(
            ElementState::empty(),
            tag_name,
            prefix,
            document,
        )
    }

    pub(crate) fn new_inherited_with_state(
        state: ElementState,
        tag_name: LocalName,
        prefix: Option<Prefix>,
        document: &Document,
    ) -> SVGGraphicsElement {
        SVGGraphicsElement {
            svgelement: SVGElement::new_inherited_with_state(state, tag_name, prefix, document),
            transform: Default::default(),
        }
    }
}

impl VirtualMethods for SVGGraphicsElement {
    fn super_type(&self) -> Option<&dyn VirtualMethods> {
        Some(self.upcast::<SVGElement>() as &dyn VirtualMethods)
    }
}

impl SVGGraphicsElementMethods<crate::DomTypeHolder> for SVGGraphicsElement {
    fn Transform(&self) -> DomRoot<SVGAnimatedTransformList> {
        self.transform.or_init(|| SVGAnimatedTransformList::new(&self.global(), self.upcast(), local_name!("transform"), CanGc::note()))
    }

    fn GetBBox(&self, options: &SVGBoundingBoxOptions) -> DomRoot<DOMRect> {
        let query_options = SVGBoundingBoxOptionsData {
            fill: options.fill,
            stroke: options.stroke,
            markers: options.markers,
            clipped: options.clipped,
        };
        let rect = self
            .owner_window()
            .query_svg_bbox(self.upcast::<Node>(), query_options)
            .or_else(|| {
                self.downcast::<SVGTextContentElement>()
                    .and_then(SVGTextContentElement::subtree_bbox)
            })
            .unwrap_or_else(euclid::Rect::zero);
        DOMRect::new(
            &self.global(),
            rect.origin.x as f64,
            rect.origin.y as f64,
            rect.size.width as f64,
            rect.size.height as f64,
            CanGc::note(),
        )
    }

    fn GetCTM(&self) -> Option<DomRoot<DOMMatrix>> {
        self.owner_window()
            .query_svg_ctm(self.upcast::<Node>())
            .or_else(|| {
                self.downcast::<SVGTextContentElement>()
                    .and_then(SVGTextContentElement::subtree_ctm)
            })
            .map(|transform| {
                svg_matrix_from_transform(
                    &self.global(),
                    euclid::default::Transform2D::new(
                        transform.m11,
                        transform.m12,
                        transform.m21,
                        transform.m22,
                        transform.m31,
                        transform.m32,
                    ),
                    CanGc::note(),
                )
            })
    }

    fn GetScreenCTM(&self) -> Option<DomRoot<DOMMatrix>> {
        self.owner_window()
            .query_svg_screen_ctm(self.upcast::<Node>())
            .or_else(|| {
                self.downcast::<SVGTextContentElement>()
                    .and_then(SVGTextContentElement::subtree_screen_ctm)
            })
            .map(|transform| {
                svg_matrix_from_transform(
                    &self.global(),
                    euclid::default::Transform2D::new(
                        transform.m11,
                        transform.m12,
                        transform.m21,
                        transform.m22,
                        transform.m31,
                        transform.m32,
                    ),
                    CanGc::note(),
                )
            })
    }
}
