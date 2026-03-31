/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use html5ever::{LocalName, Prefix, local_name};
use style_traits::CSSPixel;

use crate::script::dom::bindings::codegen::GenericBindings::CharacterDataBinding::CharacterDataMethods;
use crate::script::dom::bindings::codegen::GenericBindings::DocumentBinding::DocumentMethods;
use crate::script::dom::bindings::codegen::Bindings::DOMPointBinding::DOMPointInit;
use crate::script::dom::bindings::codegen::GenericBindings::SelectionBinding::SelectionMethods;
use crate::script::dom::bindings::codegen::GenericBindings::SVGTextContentElementBinding::SVGTextContentElementMethods;
use crate::script::dom::bindings::inheritance::Castable;
use crate::script::dom::bindings::num::Finite;
use crate::script::dom::bindings::reflector::DomGlobal;
use crate::script::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::script::dom::characterdata::CharacterData;
use crate::script::dom::document::Document;
use crate::script::dom::dompoint::DOMPoint;
use crate::script::dom::domrect::DOMRect;
use crate::script::dom::node::{Node, NodeTraits, ShadowIncluding};
use crate::script::dom::svg::svganimatedvalueobjects::{SVGAnimatedEnumeration, SVGAnimatedLength};
use crate::script::dom::svg::svggraphicselement::SVGGraphicsElement;
use crate::script::dom::svg::svgtextpathelement::SVGTextPathElement;
use crate::script::dom::svg::svgtextelement::SVGTextElement;
use crate::script::dom::svg::svgtspanelement::SVGTSpanElement;
use crate::script::dom::text::Text;
use crate::script::script_runtime::CanGc;

const LENGTH_ADJUST_VALUES: &[(&str, u16)] = &[("spacing", 1), ("spacingAndGlyphs", 2)];

#[dom_struct]
pub(crate) struct SVGTextContentElement {
    svggraphicselement: SVGGraphicsElement,
    text_length: MutNullableDom<SVGAnimatedLength>,
    length_adjust: MutNullableDom<SVGAnimatedEnumeration>,
}

impl SVGTextContentElement {
    pub(crate) fn new_inherited(local_name: LocalName, prefix: Option<Prefix>, document: &Document) -> Self {
        Self {
            svggraphicselement: SVGGraphicsElement::new_inherited(local_name, prefix, document),
            text_length: Default::default(),
            length_adjust: Default::default(),
        }
    }

    fn is_text_query_container(node: &Node) -> bool {
        node.is::<SVGTextElement>() || node.is::<SVGTSpanElement>() || node.is::<SVGTextPathElement>()
    }

    fn text_char_count(text: &Text) -> u32 {
        text.upcast::<CharacterData>().Data().str().chars().count() as u32
    }

    fn utf16_offset_for_char_index(data: &str, char_index: u32) -> u32 {
        data.chars()
            .take(char_index as usize)
            .map(|ch| ch.len_utf16() as u32)
            .sum()
    }

    fn root_text_query_node(&self) -> Option<DomRoot<Node>> {
        self.upcast::<Node>()
            .inclusive_ancestors(ShadowIncluding::No)
            .find(|ancestor| ancestor.is::<SVGTextElement>())
    }

    fn accumulate_subtree_char_range(
        current: &Node,
        target: &Node,
        cursor: &mut u32,
        start: &mut Option<u32>,
        end: &mut Option<u32>,
    ) {
        let is_target = std::ptr::eq(current, target);
        if is_target {
            *start = Some(*cursor);
        }

        if let Some(text) = current.downcast::<Text>() {
            *cursor += Self::text_char_count(text);
        } else if Self::is_text_query_container(current) {
            for child in current.children() {
                Self::accumulate_subtree_char_range(&child, target, cursor, start, end);
            }
        }

        if is_target {
            *end = Some(*cursor);
        }
    }

    fn subtree_char_range(&self) -> Option<(DomRoot<Node>, u32, u32)> {
        let root = self.root_text_query_node()?;
        let mut cursor = 0;
        let mut start = None;
        let mut end = None;
        Self::accumulate_subtree_char_range(&root, self.upcast(), &mut cursor, &mut start, &mut end);
        Some((root, start?, end?.saturating_sub(start?)))
    }

    fn resolve_subtree_text_position(
        current: &Node,
        remaining: &mut u32,
        last: &mut Option<(DomRoot<Node>, u32)>,
    ) -> Option<(DomRoot<Node>, u32)> {
        if let Some(text) = current.downcast::<Text>() {
            let data = text.upcast::<CharacterData>().Data();
            let text_str = data.str();
            let char_count = text_str.chars().count() as u32;
            let utf16_len = text_str.encode_utf16().count() as u32;
            let text_node = DomRoot::from_ref(text.upcast::<Node>());
            if *remaining <= char_count {
                return Some((
                    text_node,
                    Self::utf16_offset_for_char_index(&text_str, *remaining),
                ));
            }
            *remaining -= char_count;
            *last = Some((text_node, utf16_len));
            return None;
        }

        if Self::is_text_query_container(current) {
            for child in current.children() {
                if let Some(found) = Self::resolve_subtree_text_position(&child, remaining, last) {
                    return Some(found);
                }
            }
        }

        None
    }

    fn subtree_dom_position(&self, char_index: u32) -> Option<(DomRoot<Node>, u32)> {
        let mut remaining = char_index;
        let mut last = None;
        Self::resolve_subtree_text_position(self.upcast(), &mut remaining, &mut last).or(last)
    }

    pub(crate) fn subtree_bbox(&self) -> Option<euclid::Rect<f32, CSSPixel>> {
        let (root, start, count) = self.subtree_char_range()?;
        self.owner_window().query_svg_text_range_bbox(&root, start, count)
    }

    pub(crate) fn subtree_ctm(&self) -> Option<euclid::Transform2D<f32, CSSPixel, CSSPixel>> {
        let (root, _, _) = self.subtree_char_range()?;
        self.owner_window().query_svg_ctm(&root)
    }

    pub(crate) fn subtree_screen_ctm(&self) -> Option<euclid::Transform2D<f32, CSSPixel, CSSPixel>> {
        let (root, _, _) = self.subtree_char_range()?;
        self.owner_window().query_svg_screen_ctm(&root)
    }
}

impl SVGTextContentElementMethods<crate::DomTypeHolder> for SVGTextContentElement {
    fn TextLength(&self) -> DomRoot<SVGAnimatedLength> {
        self.text_length.or_init(|| SVGAnimatedLength::new(&self.global(), self.upcast(), local_name!("textLength"), CanGc::note()))
    }

    fn LengthAdjust(&self) -> DomRoot<SVGAnimatedEnumeration> {
        self.length_adjust.or_init(|| SVGAnimatedEnumeration::new(&self.global(), self.upcast(), local_name!("lengthAdjust"), LENGTH_ADJUST_VALUES, CanGc::note()))
    }

    fn GetNumberOfChars(&self) -> i32 {
        self.subtree_char_range()
            .map(|(_, _, count)| count as i32)
            .unwrap_or(0)
    }

    fn GetComputedTextLength(&self) -> Finite<f32> {
        let length = self
            .subtree_char_range()
            .and_then(|(root, start, count)| self.owner_window().query_svg_text_substring_length(&root, start, count))
            .unwrap_or(0.0);
        Finite::wrap(length)
    }

    fn GetSubStringLength(&self, charnum: u32, nchars: u32) -> Finite<f32> {
        let length = self
            .subtree_char_range()
            .and_then(|(root, start, count)| {
                if charnum > count {
                    return None;
                }
                let clamped = nchars.min(count.saturating_sub(charnum));
                self.owner_window()
                    .query_svg_text_substring_length(&root, start + charnum, clamped)
            })
            .unwrap_or(0.0);
        Finite::wrap(length)
    }

    fn GetStartPositionOfChar(&self, charnum: u32) -> DomRoot<DOMPoint> {
        let point = self
            .subtree_char_range()
            .and_then(|(root, start, count)| {
                if charnum >= count {
                    return None;
                }
                self.owner_window()
                    .query_svg_text_char_geometry(&root, start + charnum)
                    .map(|geometry| geometry.start)
            })
            .unwrap_or_else(|| euclid::Point2D::new(0.0, 0.0));
        DOMPoint::new(&self.global(), point.x as f64, point.y as f64, 0.0, 1.0, CanGc::note())
    }

    fn GetEndPositionOfChar(&self, charnum: u32) -> DomRoot<DOMPoint> {
        let point = self
            .subtree_char_range()
            .and_then(|(root, start, count)| {
                if charnum >= count {
                    return None;
                }
                self.owner_window()
                    .query_svg_text_char_geometry(&root, start + charnum)
                    .map(|geometry| geometry.end)
            })
            .unwrap_or_else(|| euclid::Point2D::new(0.0, 0.0));
        DOMPoint::new(&self.global(), point.x as f64, point.y as f64, 0.0, 1.0, CanGc::note())
    }

    fn GetExtentOfChar(&self, charnum: u32) -> DomRoot<DOMRect> {
        let rect = self
            .subtree_char_range()
            .and_then(|(root, start, count)| {
                if charnum >= count {
                    return None;
                }
                self.owner_window()
                    .query_svg_text_char_geometry(&root, start + charnum)
                    .map(|geometry| geometry.extent)
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

    fn GetRotationOfChar(&self, charnum: u32) -> Finite<f32> {
        Finite::wrap(
            self.subtree_char_range()
                .and_then(|(root, start, count)| {
                    if charnum >= count {
                        return None;
                    }
                    self.owner_window()
                        .query_svg_text_char_geometry(&root, start + charnum)
                        .map(|geometry| geometry.rotation)
                })
                .unwrap_or(0.0),
        )
    }

    fn GetCharNumAtPosition(&self, point: &DOMPointInit) -> i32 {
        self.subtree_char_range()
            .and_then(|(root, start, count)| {
                let global = self.owner_window().query_svg_text_char_num_at_position(
                    &root,
                    euclid::Point2D::new(point.x as f32, point.y as f32),
                )?;
                let global = global as u32;
                if global < start || global >= start + count {
                    return Some(-1);
                }
                Some((global - start) as i32)
            })
            .unwrap_or(-1)
    }

    fn SelectSubString(&self, charnum: u32, nchars: u32) {
        let Some((_, _, count)) = self.subtree_char_range() else {
            return;
        };
        if charnum > count {
            return;
        }
        let end = charnum.saturating_add(nchars).min(count);
        let Some((start_node, start_offset)) = self.subtree_dom_position(charnum) else {
            return;
        };
        let Some((end_node, end_offset)) = self.subtree_dom_position(end) else {
            return;
        };
        let Some(selection) = self.owner_document().GetSelection(CanGc::note()) else {
            return;
        };
        let _ = selection.SetBaseAndExtent(
            &start_node,
            start_offset,
            &end_node,
            end_offset,
            CanGc::note(),
        );
    }
}
