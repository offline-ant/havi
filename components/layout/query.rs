/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Utilities for querying the layout, as needed by layout.
use std::rc::Rc;

use app_units::Au;
use euclid::{Point2D, Rect, SideOffsets2D, Size2D};
use itertools::Itertools;
use layout_api::wrapper_traits::{LayoutNode, ThreadSafeLayoutElement, ThreadSafeLayoutNode};
use layout_api::{
    AxesOverflow, BoxAreaType, CSSPixelRectIterator, LayoutElementType, LayoutNodeType,
    OffsetParentResponse, PhysicalSides, ScrollContainerQueryFlags, ScrollContainerResponse,
};
use script::layout_dom::{ServoLayoutNode, ServoThreadSafeLayoutNode};
use servo_arc::Arc as ServoArc;

use servo_url::BrowserUrl;
use style::computed_values::display::T as Display;
use style::computed_values::position::T as Position;
use style::computed_values::visibility::T as Visibility;
use style::computed_values::white_space_collapse::T as WhiteSpaceCollapseValue;
use style::context::{QuirksMode, SharedStyleContext, StyleContext, ThreadLocalStyleContext};
use style::dom::{NodeInfo, OpaqueNode, TElement, TNode};
use style::properties::style_structs::Font;
use style::properties::{
    ComputedValues, Importance, LonghandId, PropertyDeclarationBlock, PropertyDeclarationId,
    PropertyId, ShorthandId, SourcePropertyDeclaration, parse_one_declaration_into,
};
use style::selector_parser::PseudoElement;
use style::shared_lock::SharedRwLock;
use style::stylesheets::{CssRuleType, Origin, UrlExtraData};
use style::stylist::RuleInclusion;
use style::traversal::resolve_style;
use style::values::computed::transform::Matrix3D;
use style::values::computed::{Float, Size};
use style::values::generics::font::LineHeight;
use style::values::generics::position::AspectRatio;
use style::values::specified::GenericGridTemplateComponent;
use style::values::specified::box_::DisplayInside;
use style::values::specified::text::TextTransformCase;
use style_traits::{CSSPixel, ParsingMode, ToCss};

use rustc_hash::FxHashMap;
use style::values::computed::CSSPixelLength;
use webrender_api::ExternalScrollId;
use webrender_api::units::LayoutVector2D;

use crate::dom::NodeExt;
use crate::flow::inline::construct::{TextTransformation, WhitespaceCollapse, capitalize_string};
use crate::fragment_tree::{FragmentFlags, FragmentTree};
use crate::geom::PhysicalRect;
use crate::style_ext::ComputedValuesExt;
use crate::svg::hit_test::hit_test_svg_path;
use crate::svg::transform::then_svg_transform;
use havi_types::fragment_tree as published;

fn au_rect_to_length_rect(rect: &Rect<Au, CSSPixel>) -> Rect<CSSPixelLength, CSSPixel> {
    Rect::new(
        Point2D::new(rect.origin.x.into(), rect.origin.y.into()),
        Size2D::new(rect.size.width.into(), rect.size.height.into()),
    )
}

fn transform_svg_rect(
    rect: PhysicalRect<Au>,
    transform: published::SVGTransform,
) -> PhysicalRect<Au> {
    let corners = [
        (rect.origin.x.to_f32_px(), rect.origin.y.to_f32_px()),
        (
            (rect.origin.x + rect.size.width).to_f32_px(),
            rect.origin.y.to_f32_px(),
        ),
        (
            rect.origin.x.to_f32_px(),
            (rect.origin.y + rect.size.height).to_f32_px(),
        ),
        (
            (rect.origin.x + rect.size.width).to_f32_px(),
            (rect.origin.y + rect.size.height).to_f32_px(),
        ),
    ];
    let transformed = corners.map(|(x, y)| {
        (
            transform.m11 * x + transform.m21 * y + transform.m31,
            transform.m12 * x + transform.m22 * y + transform.m32,
        )
    });
    let min_x = transformed
        .iter()
        .map(|(x, _)| *x)
        .fold(f32::INFINITY, f32::min);
    let min_y = transformed
        .iter()
        .map(|(_, y)| *y)
        .fold(f32::INFINITY, f32::min);
    let max_x = transformed
        .iter()
        .map(|(x, _)| *x)
        .fold(f32::NEG_INFINITY, f32::max);
    let max_y = transformed
        .iter()
        .map(|(_, y)| *y)
        .fold(f32::NEG_INFINITY, f32::max);
    PhysicalRect::new(
        crate::geom::PhysicalPoint::new(Au::from_f32_px(min_x), Au::from_f32_px(min_y)),
        crate::geom::PhysicalSize::new(
            Au::from_f32_px(max_x - min_x),
            Au::from_f32_px(max_y - min_y),
        ),
    )
}

/// Scroll offset state passed to geometry queries so they can account for
/// ancestor scroll containers when converting between document and viewport
/// coordinate spaces.
pub(crate) struct ScrollOffsets<'a> {
    pub offsets: &'a FxHashMap<ExternalScrollId, LayoutVector2D>,
    pub pipeline_id: webrender_api::PipelineId,
}

impl ScrollOffsets<'_> {
    /// Walk up from `node` through its DOM ancestors, accumulating scroll offsets
    /// of all ancestor scroll containers. Returns the total offset that should be
    /// subtracted from document-relative coordinates to get viewport-relative
    /// coordinates.
    fn cumulative_scroll_offset(&self, node: ServoLayoutNode<'_>) -> euclid::Vector2D<Au, CSSPixel> {
        let mut offset = euclid::Vector2D::<Au, CSSPixel>::zero();

        // Walk up through parent nodes looking for scroll containers.
        let mut current = node.parent_node();
        while let Some(ancestor) = current {
            if let Some(element) = ancestor.as_element() {
                let ts = ancestor.to_threadsafe();
                if let Some(layout_data) = ts.inner_layout_data() {
                    let layout_box = layout_data.self_box.borrow();
                    if let Some(layout_box) = layout_box.as_ref() {
                        if let Some((style, flags)) =
                            layout_box.with_base(|base| (base.style.clone(), base.base_fragment_info.flags))
                        {
                            if style.establishes_scroll_container(flags) {
                                let external_id = ExternalScrollId(
                                    element.as_node().opaque().id() as u64,
                                    self.pipeline_id,
                                );
                                if let Some(scroll_offset) = self.offsets.get(&external_id) {
                                    offset.x += Au::from_f32_px(scroll_offset.x);
                                    offset.y += Au::from_f32_px(scroll_offset.y);
                                }
                            }
                        }
                    }
                }
            }
            current = ancestor.parent_node();
        }

        // Also include the root scroll offset (ExternalScrollId(0, pipeline_id)).
        let root_id = ExternalScrollId(0, self.pipeline_id);
        if let Some(root_offset) = self.offsets.get(&root_id) {
            offset.x += Au::from_f32_px(root_offset.x);
            offset.y += Au::from_f32_px(root_offset.y);
        }

        offset
    }
}

fn first_fragment_id(
    fragment_tree: &FragmentTree,
    node: ServoThreadSafeLayoutNode<'_>,
    pseudo: Option<PseudoElement>,
) -> Option<published::FragmentId> {
    fragment_tree
        .fragments_for_node(node.opaque(), pseudo)
        .first()
        .copied()
}

fn node_is_fixed_positioned(node: ServoThreadSafeLayoutNode<'_>) -> bool {
    let Some(layout_data) = node.inner_layout_data() else {
        return false;
    };
    let layout_box = layout_data.self_box.borrow();
    layout_box
        .as_ref()
        .and_then(|layout_box| layout_box.with_base(|base| base.style.get_box().position))
        == Some(Position::Fixed)
}

fn accumulated_svg_transform_and_origin(
    generation: &published::FragmentArenaGeneration,
    fragment_id: published::FragmentId,
) -> Option<(published::SVGTransform, euclid::Vector2D<Au, CSSPixel>)> {
    let mut transform = published::SVGTransform::identity();
    let mut current = Some(fragment_id);
    let mut top_svg_fragment = None;

    while let Some(id) = current {
        match generation.kind(id) {
            published::FragmentKind::SVGViewport(svg_fragment) => {
                transform = then_svg_transform(transform, svg_fragment.local_to_parent_transform);
                top_svg_fragment = Some(id);
            }
            published::FragmentKind::SVGContainer(svg_fragment) => {
                transform = then_svg_transform(transform, svg_fragment.local_transform);
                top_svg_fragment = Some(id);
            }
            published::FragmentKind::SVGLeaf(svg_fragment) => {
                transform = then_svg_transform(transform, svg_fragment.local_transform);
                top_svg_fragment = Some(id);
            }
            _ => break,
        }
        current = generation.node(id).parent;
    }

    let top_svg_fragment = top_svg_fragment?;
    Some((
        transform,
        generation.containing_block(top_svg_fragment).origin.to_vector(),
    ))
}

fn box_area_rect(
    generation: &published::FragmentArenaGeneration,
    fragment_id: published::FragmentId,
    area: BoxAreaType,
) -> Option<PhysicalRect<Au>> {
    match generation.kind(fragment_id) {
        published::FragmentKind::Box(box_fragment) | published::FragmentKind::Float(box_fragment) => {
            let containing_block = generation.containing_block(fragment_id);
            Some(match area {
                BoxAreaType::Content => box_fragment.content_rect(),
                BoxAreaType::Padding => box_fragment.padding_rect(),
                BoxAreaType::Border => box_fragment.border_rect(),
            }
            .translate(containing_block.origin.to_vector()))
        }
        published::FragmentKind::Positioning(positioning_fragment) => {
            let containing_block = generation.containing_block(fragment_id);
            Some(positioning_fragment.base.rect.translate(containing_block.origin.to_vector()))
        }
        published::FragmentKind::SVGViewport(svg_fragment) => {
            let containing_block = generation.containing_block(fragment_id);
            Some(svg_fragment.base.rect.translate(containing_block.origin.to_vector()))
        }
        published::FragmentKind::SVGContainer(svg_fragment) => {
            let (transform, origin) = accumulated_svg_transform_and_origin(generation, fragment_id)?;
            Some(transform_svg_rect(svg_fragment.base.rect, transform).translate(origin))
        }
        published::FragmentKind::SVGLeaf(svg_fragment) => {
            let (transform, origin) = accumulated_svg_transform_and_origin(generation, fragment_id)?;
            Some(transform_svg_rect(svg_fragment.base.rect, transform).translate(origin))
        }
        published::FragmentKind::Text(_) |
        published::FragmentKind::Image(_) |
        published::FragmentKind::IFrame(_) => None,
    }
}

fn fragment_is_fixed_positioned(
    generation: &published::FragmentArenaGeneration,
    mut fragment_id: published::FragmentId,
) -> bool {
    loop {
        let node = generation.node(fragment_id);
        if node.base().style.get_box().position == Position::Fixed {
            return true;
        }
        let Some(parent) = node.parent else {
            return false;
        };
        fragment_id = parent;
    }
}

fn fragment_client_rect(
    fragment_tree: &FragmentTree,
    fragment_id: published::FragmentId,
) -> Rect<i32, CSSPixel> {
    let generation = fragment_tree.generation();
    let rect = match generation.kind(fragment_id) {
        published::FragmentKind::Box(box_fragment) | published::FragmentKind::Float(box_fragment) => {
            if box_fragment
                .base
                .style
                .is_inline_box(crate::fragment_tree::FragmentFlags::from_bits_retain(
                    box_fragment.base.flags.bits(),
                )) {
                return Rect::zero();
            }
            let mut rect = if matches!(
                box_fragment.specific_layout_info,
                Some(published::SpecificLayoutInfo::TableWrapper)
            ) {
                box_fragment.border_rect()
            } else {
                let mut padding_rect = box_fragment.padding_rect();
                padding_rect.origin = crate::geom::PhysicalPoint::new(
                    box_fragment.border.left,
                    box_fragment.border.top,
                );
                padding_rect
            };
            rect.origin = crate::geom::PhysicalPoint::new(
                rect.origin.x,
                rect.origin.y,
            );
            rect
        }
        _ => return Rect::zero(),
    };

    Rect::new(
        Point2D::new(rect.origin.x.to_f32_px(), rect.origin.y.to_f32_px()),
        Size2D::new(rect.size.width.to_f32_px(), rect.size.height.to_f32_px()),
    )
    .round()
    .to_i32()
}

fn fragment_scrolling_area(
    fragment_tree: &FragmentTree,
    fragment_id: published::FragmentId,
) -> PhysicalRect<Au> {
    let generation = fragment_tree.generation();
    match generation.kind(fragment_id) {
        published::FragmentKind::Box(_) | published::FragmentKind::Float(_) => generation
            .scrollable_overflow_for(fragment_id)
            .translate(generation.containing_block(fragment_id).origin.to_vector()),
        _ => generation.base(fragment_id).rect,
    }
}

fn fragment_is_inline_box(fragment_tree: &FragmentTree, fragment_id: published::FragmentId) -> bool {
    let generation = fragment_tree.generation();
    match generation.kind(fragment_id) {
        published::FragmentKind::Box(box_fragment) => box_fragment
            .base
            .style
            .is_inline_box(crate::fragment_tree::FragmentFlags::from_bits_retain(
                box_fragment.base.flags.bits(),
            )),
        _ => false,
    }
}

fn resolved_size_should_be_used_value(
    fragment_tree: &FragmentTree,
    fragment_id: published::FragmentId,
) -> bool {
    match fragment_tree.generation().kind(fragment_id) {
        published::FragmentKind::Box(box_fragment) => !box_fragment
            .base
            .style
            .is_inline_box(crate::fragment_tree::FragmentFlags::from_bits_retain(
                box_fragment.base.flags.bits(),
            )),
        published::FragmentKind::Float(_) |
        published::FragmentKind::Positioning(_) |
        published::FragmentKind::Image(_) |
        published::FragmentKind::IFrame(_) |
        published::FragmentKind::SVGViewport(_) |
        published::FragmentKind::SVGContainer(_) => true,
        published::FragmentKind::SVGLeaf(svg) => !matches!(svg.kind, published::SVGLeafKind::Text(_)),
        published::FragmentKind::Text(_) => false,
    }
}

pub(crate) fn process_padding_request(
    fragment_tree: &FragmentTree,
    node: ServoThreadSafeLayoutNode<'_>,
) -> Option<PhysicalSides> {
    let fragment_id = first_fragment_id(fragment_tree, node, None)?;
    Some(match fragment_tree.generation().kind(fragment_id) {
        published::FragmentKind::Box(box_fragment) | published::FragmentKind::Float(box_fragment) => {
            let padding = box_fragment.padding;
            PhysicalSides {
                top: padding.top,
                left: padding.left,
                bottom: padding.bottom,
                right: padding.right,
            }
        }
        _ => Default::default(),
    })
}

pub(crate) fn process_box_area_request(
    fragment_tree: &FragmentTree,
    node: ServoLayoutNode<'_>,
    area: BoxAreaType,
    exclude_transform_and_inline: bool,
    scroll_offsets: &ScrollOffsets<'_>,
) -> Option<Rect<Au, CSSPixel>> {
    let ts = node.to_threadsafe();
    let requested_node_is_fixed = node_is_fixed_positioned(ts);
    let scroll_offset = scroll_offsets.cumulative_scroll_offset(node);
    let generation = fragment_tree.generation();
    let mut rects = fragment_tree
        .fragments_for_node(ts.opaque(), None)
        .iter()
        .copied()
        .filter(|fragment_id| !exclude_transform_and_inline || !fragment_is_inline_box(fragment_tree, *fragment_id))
        .filter_map(|fragment_id| {
            box_area_rect(&generation, fragment_id, area).map(|rect| {
                if requested_node_is_fixed || fragment_is_fixed_positioned(&generation, fragment_id) {
                    rect
                } else {
                    rect.translate(-scroll_offset)
                }
            })
        })
        .peekable();

    rects.peek()?;
    Some(rects.fold(Rect::zero(), |unioned_rect, rect| rect.union(&unioned_rect)))
}

pub(crate) fn process_box_areas_request(
    fragment_tree: &FragmentTree,
    node: ServoLayoutNode<'_>,
    area: BoxAreaType,
    scroll_offsets: &ScrollOffsets<'_>,
) -> CSSPixelRectIterator {
    let requested_node_is_fixed = node_is_fixed_positioned(node.to_threadsafe());
    let scroll_offset = scroll_offsets.cumulative_scroll_offset(node);
    let generation = fragment_tree.generation();
    let fragment_ids = fragment_tree
        .fragments_for_node(node.to_threadsafe().opaque(), None)
        .to_vec();
    let rects: Vec<_> = fragment_ids
        .into_iter()
        .filter_map(|fragment_id| {
            box_area_rect(&generation, fragment_id, area).map(|rect| {
                if requested_node_is_fixed || fragment_is_fixed_positioned(&generation, fragment_id) {
                    rect
                } else {
                    rect.translate(-scroll_offset)
                }
            })
        })
        .collect();

    Box::new(rects.into_iter())
}

pub fn process_client_rect_request(
    fragment_tree: &FragmentTree,
    node: ServoThreadSafeLayoutNode<'_>,
) -> Rect<i32, CSSPixel> {
    first_fragment_id(fragment_tree, node, None)
        .map(|fragment_id| fragment_client_rect(fragment_tree, fragment_id))
        .unwrap_or_default()
}

/// Process a query for the current CSS zoom of an element.
/// <https://drafts.csswg.org/cssom-view/#dom-element-currentcsszoom>
///
/// Returns the effective zoom of the element, which is the product of all zoom
/// values from the element up to the root. Returns 1.0 if the element is not
/// being rendered (has no associated box).
pub fn process_current_css_zoom_query(node: ServoLayoutNode<'_>) -> f32 {
    let Some(layout_data) = node.to_threadsafe().inner_layout_data() else {
        return 1.0;
    };
    let layout_box = layout_data.self_box.borrow();
    let Some(layout_box) = layout_box.as_ref() else {
        return 1.0;
    };
    layout_box
        .with_base(|base| base.style.effective_zoom.value())
        .unwrap_or(1.0)
}

/// <https://drafts.csswg.org/cssom-view/#scrolling-area>
pub fn process_node_scroll_area_request(
    requested_node: Option<ServoThreadSafeLayoutNode<'_>>,
    fragment_tree: Option<Rc<FragmentTree>>,
) -> Rect<i32, CSSPixel> {
    let Some(tree) = fragment_tree else {
        return Rect::zero();
    };

    let rect = match requested_node {
        Some(node) => first_fragment_id(&tree, node, None)
            .map(|fragment_id| fragment_scrolling_area(&tree, fragment_id))
            .unwrap_or_default(),
        None => tree.scrollable_overflow(),
    };

    Rect::new(
        rect.origin.map(Au::to_f32_px),
        rect.size.to_vector().map(Au::to_f32_px).to_size(),
    )
    .round()
    .to_i32()
}

/// Return the resolved value of property for a given (pseudo)element.
/// <https://drafts.csswg.org/cssom/#resolved-value>
pub fn process_resolved_style_request(
    fragment_tree: Option<&FragmentTree>,
    context: &SharedStyleContext,
    node: ServoLayoutNode<'_>,
    pseudo: &Option<PseudoElement>,
    property: &PropertyId,
) -> String {
    if !node.as_element().unwrap().has_data() {
        return process_resolved_style_request_for_unstyled_node(context, node, pseudo, property);
    }

    // We call process_resolved_style_request after performing a whole-document
    // traversal, so in the common case, the element is styled.
    let layout_element = node.to_threadsafe().as_element().unwrap();
    let layout_element = match pseudo {
        Some(pseudo_element_type) => {
            match layout_element.with_pseudo(*pseudo_element_type) {
                Some(layout_element) => layout_element,
                None => {
                    // The pseudo doesn't exist, return nothing.  Chrome seems to query
                    // the element itself in this case, Firefox uses the resolved value.
                    // https://www.w3.org/Bugs/Public/show_bug.cgi?id=29006
                    return String::new();
                },
            }
        },
        None => layout_element,
    };

    let style = &*layout_element.style(context);
    let longhand_id = match *property {
        PropertyId::NonCustom(id) => match id.longhand_or_shorthand() {
            Ok(longhand_id) => longhand_id,
            Err(shorthand_id) => return shorthand_to_css_string(shorthand_id, style),
        },
        PropertyId::Custom(ref name) => {
            return style.computed_value_to_string(PropertyDeclarationId::Custom(name));
        },
    }
    .to_physical(style.writing_mode);

    let Some(fragment_tree) = fragment_tree else {
        return style.computed_value_to_string(PropertyDeclarationId::Longhand(longhand_id));
    };

    // From <https://drafts.csswg.org/css-transforms-2/#serialization-of-the-computed-value>
    let serialize_transform_value = |box_fragment: Option<&published::BoxFragment>| -> Result<String, ()> {
        let transform_list = &style.get_box().transform;
        if transform_list.0.is_empty() {
            return Ok("none".into());
        }
        let length_rect = box_fragment
            .map(|box_fragment| au_rect_to_length_rect(&box_fragment.border_rect()).to_untyped());
        let (transform, is_3d) = transform_list.to_transform_3d_matrix(length_rect.as_ref())?;
        let matrix = Matrix3D::from(transform);
        if !is_3d {
            Ok(matrix.into_2d()?.to_css_string())
        } else {
            Ok(matrix.to_css_string())
        }
    };

    let computed_style = |fragment: Option<published::FragmentId>| match longhand_id {
        LonghandId::MinWidth
            if style.clone_min_width() == Size::Auto &&
                !should_honor_min_size_auto(fragment_tree, fragment, style) =>
        {
            String::from("0px")
        }
        LonghandId::MinHeight
            if style.clone_min_height() == Size::Auto &&
                !should_honor_min_size_auto(fragment_tree, fragment, style) =>
        {
            String::from("0px")
        }
        LonghandId::Transform => match serialize_transform_value(None) {
            Ok(value) => value,
            Err(..) => style.computed_value_to_string(PropertyDeclarationId::Longhand(longhand_id)),
        },
        _ => style.computed_value_to_string(PropertyDeclarationId::Longhand(longhand_id)),
    };

    if longhand_id == LonghandId::LineHeight {
        let font = style.get_font();
        let font_size = font.font_size.computed_size();
        return match font.line_height {
            LineHeight::Normal => computed_style(None),
            LineHeight::Number(value) => (font_size * value.0).to_css_string(),
            LineHeight::Length(value) => value.0.to_css_string(),
        };
    }

    let display = style.get_box().display;
    if display.is_none() || display.is_contents() {
        return computed_style(None);
    }

    let resolve_for_fragment = |fragment_id: published::FragmentId| {
        let generation = fragment_tree.generation();
        let (content_rect, margins, padding, specific_layout_info) = match generation.kind(fragment_id) {
            published::FragmentKind::Box(box_fragment) | published::FragmentKind::Float(box_fragment) => {
                if style.get_box().position != Position::Static {
                    if longhand_id == LonghandId::Transform {
                        if let Ok(string) = serialize_transform_value(Some(box_fragment)) {
                            return string;
                        }
                    }
                }
                (
                    box_fragment.base.rect,
                    box_fragment.margin,
                    box_fragment.padding,
                    box_fragment.specific_layout_info.as_ref(),
                )
            }
            published::FragmentKind::Positioning(positioning_fragment) => (
                positioning_fragment.base.rect,
                SideOffsets2D::zero(),
                SideOffsets2D::zero(),
                None,
            ),
            published::FragmentKind::Text(_) |
            published::FragmentKind::Image(_) |
            published::FragmentKind::IFrame(_) |
            published::FragmentKind::SVGViewport(_) |
            published::FragmentKind::SVGContainer(_) |
            published::FragmentKind::SVGLeaf(_) => return computed_style(Some(fragment_id)),
        };

        if display.inside() == DisplayInside::Grid {
            if let Some(published::SpecificLayoutInfo::Grid(info)) = specific_layout_info {
                if let Some(value) = resolve_grid_template(info, style, longhand_id) {
                    return value;
                }
            }
        }

        match longhand_id {
            LonghandId::Width if resolved_size_should_be_used_value(fragment_tree, fragment_id) => {
                content_rect.size.width
            }
            LonghandId::Height if resolved_size_should_be_used_value(fragment_tree, fragment_id) => {
                content_rect.size.height
            }
            LonghandId::Top | LonghandId::Right | LonghandId::Bottom | LonghandId::Left => {
                return computed_style(Some(fragment_id));
            }
            LonghandId::MarginBottom => margins.bottom,
            LonghandId::MarginTop => margins.top,
            LonghandId::MarginLeft => margins.left,
            LonghandId::MarginRight => margins.right,
            LonghandId::PaddingBottom => padding.bottom,
            LonghandId::PaddingTop => padding.top,
            LonghandId::PaddingLeft => padding.left,
            LonghandId::PaddingRight => padding.right,
            _ => return computed_style(Some(fragment_id)),
        }
        .to_css_string()
    };

    fragment_tree
        .fragments_for_node(node.to_threadsafe().opaque(), *pseudo)
        .first()
        .copied()
        .map(resolve_for_fragment)
        .unwrap_or_else(|| computed_style(None))
}

fn should_honor_min_size_auto(
    fragment_tree: &FragmentTree,
    fragment: Option<published::FragmentId>,
    style: &ComputedValues,
) -> bool {
    // <https://drafts.csswg.org/css-sizing-3/#automatic-minimum-size>
    // For backwards-compatibility, the resolved value of an automatic minimum size is zero
    // for boxes of all CSS2 display types: block and inline boxes, inline blocks, and all
    // the table layout boxes. It also resolves to zero when no box is generated.
    //
    // <https://github.com/w3c/csswg-drafts/issues/11716>
    // However, when a box is generated and `aspect-ratio` isn't `auto`, we need to preserve
    // the automatic minimum size as `auto`.
    let Some(fragment_id) = fragment else {
        return false;
    };
    let generation = fragment_tree.generation();
    let published::FragmentKind::Box(box_fragment) = generation.kind(fragment_id) else {
        return false;
    };
    let flags = box_fragment.base.flags;
    flags.contains(published::FragmentFlags::IS_FLEX_OR_GRID_ITEM) ||
        style.clone_aspect_ratio() != AspectRatio::auto()
}

fn resolve_grid_template(
    grid_info: &published::GridLayoutInfo,
    style: &ComputedValues,
    longhand_id: LonghandId,
) -> Option<String> {
    /// <https://drafts.csswg.org/css-grid/#resolved-track-list-standalone>
    fn serialize_standalone_non_subgrid_track_list(track_sizes: &[Au]) -> Option<String> {
        match track_sizes.is_empty() {
            // Standalone non subgrid grids with empty track lists should compute to `none`.
            // As of current standard, this behaviour should only invoked by `none` computed value,
            // therefore we can fallback into computed value resolving.
            true => None,
            // <https://drafts.csswg.org/css-grid/#resolved-track-list-standalone>
            // > - Every track listed individually, whether implicitly or explicitly created,
            //     without using the repeat() notation.
            // > - Every track size given as a length in pixels, regardless of sizing function.
            // > - Adjacent line names collapsed into a single bracketed set.
            // TODO: implement line names
            false => Some(
                track_sizes
                    .iter()
                    .map(|size| size.to_css_string())
                    .join(" "),
            ),
        }
    }

    let (track_sizes, computed_value) = match longhand_id {
        LonghandId::GridTemplateRows => (&grid_info.rows, &style.get_position().grid_template_rows),
        LonghandId::GridTemplateColumns => (
            &grid_info.columns,
            &style.get_position().grid_template_columns,
        ),
        _ => return None,
    };

    match computed_value {
        // <https://drafts.csswg.org/css-grid/#resolved-track-list-standalone>
        // > When an element generates a grid container box, the resolved value of its grid-template-rows or
        // > grid-template-columns property in a standalone axis is the used value, serialized with:
        GenericGridTemplateComponent::None |
        GenericGridTemplateComponent::TrackList(_) |
        GenericGridTemplateComponent::Masonry => {
            serialize_standalone_non_subgrid_track_list(track_sizes)
        },

        // <https://drafts.csswg.org/css-grid/#resolved-track-list-subgrid>
        // > When an element generates a grid container box that is a subgrid, the resolved value of the
        // > grid-template-rows and grid-template-columns properties represents the used number of columns,
        // > serialized as the subgrid keyword followed by a list representing each of its lines as a
        // > line name set of all the line’s names explicitly defined on the subgrid (not including those
        // > adopted from the parent grid), without using the repeat() notation.
        // TODO: implement subgrid
        GenericGridTemplateComponent::Subgrid(_) => None,
    }
}

pub fn process_resolved_style_request_for_unstyled_node(
    context: &SharedStyleContext,
    node: ServoLayoutNode<'_>,
    pseudo: &Option<PseudoElement>,
    property: &PropertyId,
) -> String {
    // In a display: none subtree. No pseudo-element exists.
    if pseudo.is_some() {
        return String::new();
    }

    let mut tlc = ThreadLocalStyleContext::new();
    let mut context = StyleContext {
        shared: context,
        thread_local: &mut tlc,
    };

    let element = node.as_element().unwrap();
    let styles = resolve_style(
        &mut context,
        element,
        RuleInclusion::All,
        pseudo.as_ref(),
        None,
    );
    let style = styles.primary();
    let longhand_id = match *property {
        PropertyId::NonCustom(id) => match id.longhand_or_shorthand() {
            Ok(longhand_id) => longhand_id,
            Err(shorthand_id) => return shorthand_to_css_string(shorthand_id, style),
        },
        PropertyId::Custom(ref name) => {
            return style.computed_value_to_string(PropertyDeclarationId::Custom(name));
        },
    };

    match longhand_id {
        // <https://drafts.csswg.org/css-sizing-3/#automatic-minimum-size>
        // The resolved value of an automatic minimum size is zero when no box is generated.
        LonghandId::MinWidth if style.clone_min_width() == Size::Auto => String::from("0px"),
        LonghandId::MinHeight if style.clone_min_height() == Size::Auto => String::from("0px"),

        // No need to care about used values here, since we're on a display: none
        // subtree, use the computed value.
        _ => style.computed_value_to_string(PropertyDeclarationId::Longhand(longhand_id)),
    }
}

fn shorthand_to_css_string(
    id: style::properties::ShorthandId,
    style: &style::properties::ComputedValues,
) -> String {
    use style::values::resolved::Context;
    let mut block = PropertyDeclarationBlock::new();
    let mut dest = String::new();
    for longhand in id.longhands() {
        block.push(
            style.computed_or_resolved_declaration(
                longhand,
                Some(&mut Context {
                    style,
                    for_property: longhand.into(),
                    current_longhand: None,
                }),
            ),
            Importance::Normal,
        );
    }
    match block.shorthand_to_css(id, &mut dest) {
        Ok(_) => dest.to_owned(),
        Err(_) => String::new(),
    }
}

struct OffsetParentFragments {
    parent: published::FragmentId,
    grandparent: Option<published::FragmentId>,
}

/// <https://www.w3.org/TR/2016/WD-cssom-view-1-20160317/#dom-htmlelement-offsetparent>
fn offset_parent_fragments(
    fragment_tree: &FragmentTree,
    node: ServoLayoutNode<'_>,
) -> Option<OffsetParentFragments> {
    // 1. If any of the following holds true return null and terminate this algorithm:
    //  * The element does not have an associated CSS layout box.
    //  * The element is the root element.
    //  * The element is the HTML body element.
    //  * The element’s computed value of the position property is fixed.
    let fragment = first_fragment_id(fragment_tree, node.to_threadsafe(), None)?;
    let generation = fragment_tree.generation();
    let base = generation.base(fragment);
    if base.flags.intersects(
        published::FragmentFlags::IS_ROOT_ELEMENT |
            published::FragmentFlags::IS_BODY_ELEMENT_OF_HTML_ELEMENT_ROOT,
    ) {
        return None;
    }
    if matches!(
        fragment_tree.generation().kind(fragment),
        published::FragmentKind::Box(box_fragment)
            if box_fragment.base.style.get_box().position == Position::Fixed
    ) {
        return None;
    }

    // 2.  Return the nearest ancestor element of the element for which at least one of
    //     the following is true and terminate this algorithm if such an ancestor is found:
    //  * The computed value of the position property is not static.
    //  * It is the HTML body element.
    //  * The computed value of the position property of the element is static and the
    //    ancestor is one of the following HTML elements: td, th, or table.
    let mut maybe_parent_node = node.parent_node();
    while let Some(parent_node) = maybe_parent_node {
        maybe_parent_node = parent_node.parent_node();

        if let Some(parent_fragment) = first_fragment_id(fragment_tree, parent_node.to_threadsafe(), None) {
            let generation = fragment_tree.generation();
            let parent_box = match generation.kind(parent_fragment) {
                published::FragmentKind::Box(box_fragment) | published::FragmentKind::Float(box_fragment) => box_fragment,
                _ => continue,
            };

            let grandparent_fragment = maybe_parent_node
                .and_then(|ancestor| first_fragment_id(fragment_tree, ancestor.to_threadsafe(), None));

            if parent_box.base.style.get_box().position != Position::Static {
                return Some(OffsetParentFragments {
                    parent: parent_fragment,
                    grandparent: grandparent_fragment,
                });
            }

            let flags = parent_box.base.flags;
            if flags.intersects(
                published::FragmentFlags::IS_BODY_ELEMENT_OF_HTML_ELEMENT_ROOT |
                    published::FragmentFlags::IS_TABLE_TH_OR_TD_ELEMENT,
            ) {
                return Some(OffsetParentFragments {
                    parent: parent_fragment,
                    grandparent: grandparent_fragment,
                });
            }
        }
    }

    None
}

#[inline]
pub fn process_offset_parent_query(
    fragment_tree: &FragmentTree,
    node: ServoLayoutNode<'_>,
) -> Option<OffsetParentResponse> {
    // Only consider the first fragment of the node found as per a
    // possible interpretation of the specification: "[...] return the
    // y-coordinate of the top border edge of the first CSS layout box
    // associated with the element [...]"
    //
    // FIXME: Browsers implement this all differently (e.g., [1]) -
    // Firefox does returns the union of all layout elements of some
    // sort. Chrome returns the first fragment for a block element (the
    // same as ours) or the union of all associated fragments in the
    // first containing block fragment for an inline element. We could
    // implement Chrome's behavior, but our fragment tree currently
    // provides insufficient information.
    //
    // [1]: https://github.com/w3c/csswg-drafts/issues/4541
    // > 1. If the element is the HTML body element or does not have any associated CSS
    //      layout box return zero and terminate this algorithm.
    let generation = fragment_tree.generation();
    let fragment = first_fragment_id(fragment_tree, node.to_threadsafe(), None)?;
    let mut border_box = box_area_rect(&generation, fragment, BoxAreaType::Border)?;

    // 2.  If the offsetParent of the element is null return the x-coordinate of the left
    //     border edge of the first CSS layout box associated with the element, relative to
    //     the initial containing block origin, ignoring any transforms that apply to the
    //     element and its ancestors, and terminate this algorithm.
    let Some(offset_parent_fragment) = offset_parent_fragments(fragment_tree, node) else {
        return Some(OffsetParentResponse {
            node_address: None,
            rect: border_box,
        });
    };

    let parent_fragment = match generation.kind(offset_parent_fragment.parent) {
        published::FragmentKind::Box(box_fragment) | published::FragmentKind::Float(box_fragment) => box_fragment,
        _ => return None,
    };
    let parent_is_static_body_element = parent_fragment
        .base
        .flags
        .contains(published::FragmentFlags::IS_BODY_ELEMENT_OF_HTML_ELEMENT_ROOT) &&
        parent_fragment.base.style.get_box().position == Position::Static;

    // For `offsetLeft`:
    // 3. Return the result of subtracting the y-coordinate of the top padding edge of the
    //    first CSS layout box associated with the offsetParent of the element from the
    //    y-coordinate of the top border edge of the first CSS layout box associated with the
    //    element, relative to the initial containing block origin, ignoring any transforms
    //    that apply to the element and its ancestors.
    //
    // We generalize this for `offsetRight` as described in the specification.
    let grandparent_box_fragment = || match offset_parent_fragment.grandparent {
        Some(fragment_id) => match generation.kind(fragment_id) {
            published::FragmentKind::Box(box_fragment) | published::FragmentKind::Float(box_fragment) => Some(box_fragment),
            _ => None,
        },
        None => None,
    };

    // The spec (https://www.w3.org/TR/cssom-view-1/#extensions-to-the-htmlelement-interface)
    // says that offsetTop/offsetLeft are always relative to the padding box of the offsetParent.
    // However, in practice this is not true in major browsers in the case that the offsetParent is the body
    // element and the body element is position:static. In that case offsetLeft/offsetTop are computed
    // relative to the root node's border box.
    //
    // See <https://github.com/w3c/csswg-drafts/issues/10549>.
    let parent_offset_rect = if parent_is_static_body_element {
        if let Some(grandparent_fragment) = grandparent_box_fragment() {
            let grandparent_id = offset_parent_fragment.grandparent.expect("grandparent fragment id");
            grandparent_fragment
                .border_rect()
                .translate(generation.containing_block(grandparent_id).origin.to_vector())
        } else {
            parent_fragment
                .padding_rect()
                .translate(generation.containing_block(offset_parent_fragment.parent).origin.to_vector())
        }
    } else {
        parent_fragment
            .padding_rect()
            .translate(generation.containing_block(offset_parent_fragment.parent).origin.to_vector())
    };
    // TODO(havi-render): Apply cumulative sticky offsets. Requires the full scroll tree
    // to compute sticky positioning based on ancestor scroll state.

    border_box = border_box.translate(-parent_offset_rect.origin.to_vector());

    Some(OffsetParentResponse {
        node_address: parent_fragment.base.tag.map(|tag| tag.node.into()),
        rect: border_box,
    })
}

/// An implementation of `scrollParent` that can also be used to for `scrollIntoView`:
/// <https://drafts.csswg.org/cssom-view/#dom-htmlelement-scrollparent>.
///
#[inline]
pub(crate) fn process_scroll_container_query(
    node: Option<ServoLayoutNode<'_>>,
    query_flags: ScrollContainerQueryFlags,
    viewport_overflow: AxesOverflow,
) -> Option<ScrollContainerResponse> {
    let Some(node) = node else {
        return Some(ScrollContainerResponse::Viewport(viewport_overflow));
    };

    let layout_data = node.to_threadsafe().inner_layout_data()?;

    // 1. If any of the following holds true, return null and terminate this algorithm:
    //  - The element does not have an associated box.
    let layout_box = layout_data.self_box.borrow();
    let layout_box = layout_box.as_ref()?;

    let (style, flags) =
        layout_box.with_base(|base| (base.style.clone(), base.base_fragment_info.flags))?;

    // - The element is the root element.
    // - The element is the body element.
    //
    // Note: We only do this for `scrollParent`, which needs to be null. But `scrollIntoView` on the
    // `<body>` or root element should still bring it into view by scrolling the viewport.
    if query_flags.contains(ScrollContainerQueryFlags::ForScrollParent) &&
        flags.intersects(
            FragmentFlags::IS_ROOT_ELEMENT | FragmentFlags::IS_BODY_ELEMENT_OF_HTML_ELEMENT_ROOT,
        )
    {
        return None;
    }

    if query_flags.contains(ScrollContainerQueryFlags::Inclusive) &&
        style.establishes_scroll_container(flags)
    {
        return Some(ScrollContainerResponse::Element(
            node.opaque().into(),
            style.effective_overflow(flags),
        ));
    }

    // - The element’s computed value of the position property is fixed and no ancestor
    //   establishes a fixed position containing block.
    //
    // This is handled below in step 2.

    // 2. Let ancestor be the containing block of the element in the flat tree and repeat these substeps:
    // - If ancestor is the initial containing block, return the scrollingElement for the
    //   element’s document if it is not closed-shadow-hidden from the element, otherwise
    //   return null.
    // - If ancestor is not closed-shadow-hidden from the element, and is a scroll
    //   container, terminate this algorithm and return ancestor.
    // - If the computed value of the position property of ancestor is fixed, and no
    //   ancestor establishes a fixed position containing block, terminate this algorithm
    //   and return null.
    // - Let ancestor be the containing block of ancestor in the flat tree.
    //
    // Notes: We don't follow the specification exactly below, but we follow the spirit.
    //
    // TODO: Handle the situation where the ancestor is "closed-shadow-hidden" from the element.
    let mut current_position_value = style.clone_position();
    let mut current_ancestor = node.as_element()?;
    while let Some(ancestor) = current_ancestor.traversal_parent() {
        current_ancestor = ancestor;

        let Some(layout_data) = ancestor.as_node().to_threadsafe().inner_layout_data() else {
            continue;
        };
        let ancestor_layout_box = layout_data.self_box.borrow();
        let Some(ancestor_layout_box) = ancestor_layout_box.as_ref() else {
            continue;
        };

        let Some((ancestor_style, ancestor_flags)) = ancestor_layout_box
            .with_base(|base| (base.style.clone(), base.base_fragment_info.flags))
        else {
            continue;
        };

        let is_containing_block = match current_position_value {
            Position::Static | Position::Relative | Position::Sticky => {
                !ancestor_style.is_inline_box(ancestor_flags)
            },
            Position::Absolute => {
                ancestor_style.establishes_containing_block_for_absolute_descendants(ancestor_flags)
            },
            Position::Fixed => {
                ancestor_style.establishes_containing_block_for_all_descendants(ancestor_flags)
            },
        };
        if !is_containing_block {
            continue;
        }

        if ancestor_style.establishes_scroll_container(ancestor_flags) {
            return Some(ScrollContainerResponse::Element(
                ancestor.as_node().opaque().into(),
                ancestor_style.effective_overflow(ancestor_flags),
            ));
        }

        current_position_value = ancestor_style.clone_position();
    }

    match current_position_value {
        Position::Fixed => None,
        _ => Some(ScrollContainerResponse::Viewport(viewport_overflow)),
    }
}

/// <https://html.spec.whatwg.org/multipage/#get-the-text-steps>
pub fn get_the_text_steps(node: ServoLayoutNode<'_>) -> String {
    // Step 1: If element is not being rendered or if the user agent is a non-CSS user agent, then
    // return element's descendant text content.
    // This is taken care of in HTMLElement code

    // Step 2: Let results be a new empty list.
    let mut results = Vec::new();
    let mut max_req_line_break_count = 0;

    // Step 3: For each child node node of element:
    let mut state = Default::default();
    for child in node.dom_children() {
        // Step 1: Let current be the list resulting in running the rendered text collection steps with node.
        let mut current = rendered_text_collection_steps(child, &mut state);
        // Step 2: For each item item in current, append item to results.
        results.append(&mut current);
    }

    let mut output = String::new();
    for item in results {
        match item {
            InnerOrOuterTextItem::Text(s) => {
                // Step 3.
                if !s.is_empty() {
                    if max_req_line_break_count > 0 {
                        // Step 5.
                        output.push_str(&"\u{000A}".repeat(max_req_line_break_count));
                        max_req_line_break_count = 0;
                    }
                    output.push_str(&s);
                }
            },
            InnerOrOuterTextItem::RequiredLineBreakCount(count) => {
                // Step 4.
                if output.is_empty() {
                    // Remove required line break count at the start.
                    continue;
                }
                // Store the count if it's the max of this run, but it may be ignored if no text
                // item is found afterwards, which means that these are consecutive line breaks at
                // the end.
                if count > max_req_line_break_count {
                    max_req_line_break_count = count;
                }
            },
        }
    }
    output
}

enum InnerOrOuterTextItem {
    Text(String),
    RequiredLineBreakCount(usize),
}

#[derive(Clone)]
struct RenderedTextCollectionState {
    /// Used to make sure we don't add a `\n` before the first row
    first_table_row: bool,
    /// Used to make sure we don't add a `\t` before the first column
    first_table_cell: bool,
    /// Keeps track of whether we're inside a table, since there are special rules like ommiting everything that's not
    /// inside a TableCell/TableCaption
    within_table: bool,
    /// Determines whether we truncate leading whitespaces for normal nodes or not
    may_start_with_whitespace: bool,
    /// Is set whenever we truncated a white space char, used to prepend a single space before the next element,
    /// that way we truncate trailing white space without having to look ahead
    did_truncate_trailing_white_space: bool,
    /// Is set to true when we're rendering the children of TableCell/TableCaption elements, that way we render
    /// everything inside those as normal, while omitting everything that's in a Table but NOT in a Cell/Caption
    within_table_content: bool,
}

impl Default for RenderedTextCollectionState {
    fn default() -> Self {
        RenderedTextCollectionState {
            first_table_row: true,
            first_table_cell: true,
            may_start_with_whitespace: true,
            did_truncate_trailing_white_space: false,
            within_table: false,
            within_table_content: false,
        }
    }
}

/// <https://html.spec.whatwg.org/multipage/#rendered-text-collection-steps>
fn rendered_text_collection_steps(
    node: ServoLayoutNode<'_>,
    state: &mut RenderedTextCollectionState,
) -> Vec<InnerOrOuterTextItem> {
    // Step 1. Let items be the result of running the rendered text collection
    // steps with each child node of node in tree order,
    // and then concatenating the results to a single list.
    let mut items = vec![];
    if !node.is_connected() || !(node.is_element() || node.is_text_node()) {
        return items;
    }

    match node.type_id() {
        LayoutNodeType::Text => {
            if let Some(element) = node.parent_node() {
                match element.type_id() {
                    // Any text contained in these elements must be ignored.
                    LayoutNodeType::Element(LayoutElementType::HTMLCanvasElement) |
                    LayoutNodeType::Element(LayoutElementType::HTMLImageElement) |
                    LayoutNodeType::Element(LayoutElementType::HTMLIFrameElement) |
                    LayoutNodeType::Element(LayoutElementType::HTMLObjectElement) |
                    LayoutNodeType::Element(LayoutElementType::HTMLInputElement) |
                    LayoutNodeType::Element(LayoutElementType::HTMLTextAreaElement) |
                    LayoutNodeType::Element(LayoutElementType::HTMLMediaElement) => {
                        return items;
                    },
                    // Select/Option/OptGroup elements are handled a bit differently.
                    // Basically: a Select can only contain Options or OptGroups, while
                    // OptGroups may also contain Options. Everything else gets ignored.
                    LayoutNodeType::Element(LayoutElementType::HTMLOptGroupElement) => {
                        if let Some(element) = element.parent_node() {
                            if !matches!(
                                element.type_id(),
                                LayoutNodeType::Element(LayoutElementType::HTMLSelectElement)
                            ) {
                                return items;
                            }
                        } else {
                            return items;
                        }
                    },
                    LayoutNodeType::Element(LayoutElementType::HTMLSelectElement) => return items,
                    _ => {},
                }

                // Tables are also a bit special, mainly by only allowing
                // content within TableCell or TableCaption elements once
                // we're inside a Table.
                if state.within_table && !state.within_table_content {
                    return items;
                }

                let Some(style_data) = element.style_data() else {
                    return items;
                };

                let element_data = style_data.element_data.borrow();
                let Some(style) = element_data.styles.get_primary() else {
                    return items;
                };

                // Step 2: If node's computed value of 'visibility' is not 'visible', then return items.
                //
                // We need to do this check here on the Text fragment, if we did it on the element and
                // just skipped rendering all child nodes then there'd be no way to override the
                // visibility in a child node.
                if style.get_inherited_box().visibility != Visibility::Visible {
                    return items;
                }

                // Step 3: If node is not being rendered, then return items. For the purpose of this step,
                // the following elements must act as described if the computed value of the 'display'
                // property is not 'none':
                let display = style.get_box().display;
                if display == Display::None {
                    match element.type_id() {
                        // Even if set to Display::None, Option/OptGroup elements need to
                        // be rendered.
                        LayoutNodeType::Element(LayoutElementType::HTMLOptGroupElement) |
                        LayoutNodeType::Element(LayoutElementType::HTMLOptionElement) => {},
                        _ => {
                            return items;
                        },
                    }
                }

                let text_content = node.to_threadsafe().text_content();

                let white_space_collapse = style.clone_white_space_collapse();
                let preserve_whitespace = white_space_collapse == WhiteSpaceCollapseValue::Preserve;
                let is_inline = matches!(
                    display,
                    Display::InlineBlock | Display::InlineFlex | Display::InlineGrid
                );
                // Now we need to decide on whether to remove beginning white space or not, this
                // is mainly decided by the elements we rendered before, but may be overwritten by the white-space
                // property.
                let trim_beginning_white_space =
                    !preserve_whitespace && (state.may_start_with_whitespace || is_inline);
                let with_white_space_rules_applied = WhitespaceCollapse::new(
                    text_content.chars(),
                    white_space_collapse,
                    trim_beginning_white_space,
                );

                // Step 4: If node is a Text node, then for each CSS text box produced by node, in
                // content order, compute the text of the box after application of the CSS
                // 'white-space' processing rules and 'text-transform' rules, set items to the list
                // of the resulting strings, and return items. The CSS 'white-space' processing
                // rules are slightly modified: collapsible spaces at the end of lines are always
                // collapsed, but they are only removed if the line is the last line of the block,
                // or it ends with a br element. Soft hyphens should be preserved.
                let text_transform = style.clone_text_transform().case();
                let mut transformed_text: String =
                    TextTransformation::new(with_white_space_rules_applied, text_transform)
                        .collect();

                // Since iterator for capitalize not doing anything, we must handle it outside here
                // FIXME: This assumes the element always start at a word boundary. But can fail:
                // a<span style="text-transform: capitalize">b</span>c
                if TextTransformCase::Capitalize == text_transform {
                    transformed_text = capitalize_string(&transformed_text, true);
                }

                let is_preformatted_element =
                    white_space_collapse == WhiteSpaceCollapseValue::Preserve;

                let is_final_character_whitespace = transformed_text
                    .chars()
                    .next_back()
                    .filter(char::is_ascii_whitespace)
                    .is_some();

                let is_first_character_whitespace = transformed_text
                    .chars()
                    .next()
                    .filter(char::is_ascii_whitespace)
                    .is_some();

                // By truncating trailing white space and then adding it back in once we
                // encounter another text node we can ensure no trailing white space for
                // normal text without having to look ahead
                if state.did_truncate_trailing_white_space && !is_first_character_whitespace {
                    items.push(InnerOrOuterTextItem::Text(String::from(" ")));
                };

                if !transformed_text.is_empty() {
                    // Here we decide whether to keep or truncate the final white
                    // space character, if there is one.
                    if is_final_character_whitespace && !is_preformatted_element {
                        state.may_start_with_whitespace = false;
                        state.did_truncate_trailing_white_space = true;
                        transformed_text.pop();
                    } else {
                        state.may_start_with_whitespace = is_final_character_whitespace;
                        state.did_truncate_trailing_white_space = false;
                    }
                    items.push(InnerOrOuterTextItem::Text(transformed_text));
                }
            } else {
                // If we don't have a parent element then there's no style data available,
                // in this (pretty unlikely) case we just return the Text fragment as is.
                items.push(InnerOrOuterTextItem::Text(
                    node.to_threadsafe().text_content().into(),
                ));
            }
        },
        LayoutNodeType::Element(LayoutElementType::HTMLBRElement) => {
            // Step 5: If node is a br element, then append a string containing a single U+000A
            // LF code point to items.
            state.did_truncate_trailing_white_space = false;
            state.may_start_with_whitespace = true;
            items.push(InnerOrOuterTextItem::Text(String::from("\u{000A}")));
        },
        _ => {
            // First we need to gather some infos to setup the various flags
            // before rendering the child nodes
            let Some(style_data) = node.style_data() else {
                return items;
            };

            let element_data = style_data.element_data.borrow();
            let Some(style) = element_data.styles.get_primary() else {
                return items;
            };
            let inherited_box = style.get_inherited_box();

            if inherited_box.visibility != Visibility::Visible {
                // If the element is not visible, then we'll immediately render all children,
                // skipping all other processing.
                // We can't just stop here since a child can override a parents visibility.
                for child in node.dom_children() {
                    items.append(&mut rendered_text_collection_steps(child, state));
                }
                return items;
            }

            let style_box = style.get_box();
            let display = style_box.display;
            let mut surrounding_line_breaks = 0;

            // Treat absolutely positioned or floated elements like Block elements
            if style_box.position == Position::Absolute || style_box.float != Float::None {
                surrounding_line_breaks = 1;
            }

            // Depending on the display property we have to do various things
            // before we can render the child nodes.
            match display {
                Display::Table => {
                    surrounding_line_breaks = 1;
                    state.within_table = true;
                },
                // Step 6: If node's computed value of 'display' is 'table-cell',
                // and node's CSS box is not the last 'table-cell' box of its
                // enclosing 'table-row' box, then append a string containing
                // a single U+0009 TAB code point to items.
                Display::TableCell => {
                    if !state.first_table_cell {
                        items.push(InnerOrOuterTextItem::Text(String::from(
                            "\u{0009}", /* tab */
                        )));
                        // Make sure we don't add a white-space we removed from the previous node
                        state.did_truncate_trailing_white_space = false;
                    }
                    state.first_table_cell = false;
                    state.within_table_content = true;
                },
                // Step 7: If node's computed value of 'display' is 'table-row',
                // and node's CSS box is not the last 'table-row' box of the nearest
                // ancestor 'table' box, then append a string containing a single U+000A
                // LF code point to items.
                Display::TableRow => {
                    if !state.first_table_row {
                        items.push(InnerOrOuterTextItem::Text(String::from(
                            "\u{000A}", /* Line Feed */
                        )));
                        // Make sure we don't add a white-space we removed from the previous node
                        state.did_truncate_trailing_white_space = false;
                    }
                    state.first_table_row = false;
                    state.first_table_cell = true;
                },
                // Step 9: If node's used value of 'display' is block-level or 'table-caption',
                // then append 1 (a required line break count) at the beginning and end of items.
                Display::Block => {
                    surrounding_line_breaks = 1;
                },
                Display::TableCaption => {
                    surrounding_line_breaks = 1;
                    state.within_table_content = true;
                },
                Display::InlineFlex | Display::InlineGrid | Display::InlineBlock => {
                    // InlineBlock's are a bit strange, in that they don't produce a Linebreak, yet
                    // disable white space truncation before and after it, making it one of the few
                    // cases where one can have multiple white space characters following one another.
                    if state.did_truncate_trailing_white_space {
                        items.push(InnerOrOuterTextItem::Text(String::from(" ")));
                        state.did_truncate_trailing_white_space = false;
                        state.may_start_with_whitespace = true;
                    }
                },
                _ => {},
            }

            match node.type_id() {
                // Step 8: If node is a p element, then append 2 (a required line break count) at
                // the beginning and end of items.
                LayoutNodeType::Element(LayoutElementType::HTMLParagraphElement) => {
                    surrounding_line_breaks = 2;
                },
                // Option/OptGroup elements should go on separate lines, by treating them like
                // Block elements we can achieve that.
                LayoutNodeType::Element(LayoutElementType::HTMLOptionElement) |
                LayoutNodeType::Element(LayoutElementType::HTMLOptGroupElement) => {
                    surrounding_line_breaks = 1;
                },
                _ => {},
            }

            if surrounding_line_breaks > 0 {
                items.push(InnerOrOuterTextItem::RequiredLineBreakCount(
                    surrounding_line_breaks,
                ));
                state.did_truncate_trailing_white_space = false;
                state.may_start_with_whitespace = true;
            }

            match node.type_id() {
                // Any text/content contained in these elements is ignored.
                // However we still need to check whether we have to prepend a
                // space, since for example <span>asd <input> qwe</span> must
                // product "asd  qwe" (note the 2 spaces)
                LayoutNodeType::Element(LayoutElementType::HTMLCanvasElement) |
                LayoutNodeType::Element(LayoutElementType::HTMLImageElement) |
                LayoutNodeType::Element(LayoutElementType::HTMLIFrameElement) |
                LayoutNodeType::Element(LayoutElementType::HTMLObjectElement) |
                LayoutNodeType::Element(LayoutElementType::HTMLInputElement) |
                LayoutNodeType::Element(LayoutElementType::HTMLTextAreaElement) |
                LayoutNodeType::Element(LayoutElementType::HTMLMediaElement) => {
                    if display != Display::Block && state.did_truncate_trailing_white_space {
                        items.push(InnerOrOuterTextItem::Text(String::from(" ")));
                        state.did_truncate_trailing_white_space = false;
                    };
                    state.may_start_with_whitespace = false;
                },
                _ => {
                    // Now we can finally iterate over all children, appending whatever
                    // they produce to items.
                    for child in node.dom_children() {
                        items.append(&mut rendered_text_collection_steps(child, state));
                    }
                },
            }

            // Depending on the display property we still need to do some
            // cleanup after rendering all child nodes
            match display {
                Display::InlineFlex | Display::InlineGrid | Display::InlineBlock => {
                    state.did_truncate_trailing_white_space = false;
                    state.may_start_with_whitespace = false;
                },
                Display::Table => {
                    state.within_table = false;
                },
                Display::TableCell | Display::TableCaption => {
                    state.within_table_content = false;
                },
                _ => {},
            }

            if surrounding_line_breaks > 0 {
                items.push(InnerOrOuterTextItem::RequiredLineBreakCount(
                    surrounding_line_breaks,
                ));
                state.did_truncate_trailing_white_space = false;
                state.may_start_with_whitespace = true;
            }
        },
    };
    items
}

type TextFragmentEntry = (published::FragmentId, Point2D<Au, CSSPixel>);

fn text_fragment_distance_to_point_for_glyph_offset(
    fragment: &published::TextFragment,
    point_in_fragment: Point2D<Au, CSSPixel>,
) -> Option<Au> {
    let rect = &fragment.base.rect;
    if point_in_fragment.y < Au::new(0) || point_in_fragment.y > rect.height() {
        return None;
    }
    if point_in_fragment.x < Au::new(0) {
        return None;
    }
    Some(point_in_fragment.x - rect.width().max(Au::new(0)))
}

fn text_fragment_character_offset(
    fragment: &published::TextFragment,
    point_in_fragment: Point2D<Au, CSSPixel>,
    starting_character: usize,
) -> usize {
    let mut current_character = starting_character;
    let mut current_offset = Au::new(0);
    for glyph in &fragment.glyphs {
        if current_offset + glyph.advance.scale_by(0.5) >= point_in_fragment.x {
            return current_character;
        }
        current_offset += glyph.advance;
        current_character += glyph.char_count as usize;
    }
    current_character
}

fn collect_text_fragment_entries(
    generation: &published::FragmentArenaGeneration,
    fragment_id: published::FragmentId,
    point_in_document: Point2D<Au, CSSPixel>,
    out: &mut Vec<TextFragmentEntry>,
) {
    match generation.kind(fragment_id) {
        published::FragmentKind::Text(text_fragment) => {
            let absolute_origin = generation.containing_block(fragment_id).origin +
                text_fragment.base.rect.origin.to_vector();
            out.push((fragment_id, point_in_document - absolute_origin.to_vector()));
        }
        _ => {
            for child in generation.geometry_children(fragment_id) {
                collect_text_fragment_entries(generation, *child, point_in_document, out);
            }
        }
    }
}

fn find_closest_text_fragment(
    generation: &published::FragmentArenaGeneration,
    fragments: &[TextFragmentEntry],
) -> Option<usize> {
    let mut closest_idx = None;
    let mut closest_dist = Au::new(0);
    for (i, (fragment_id, point_in_fragment)) in fragments.iter().enumerate() {
        let published::FragmentKind::Text(text_fragment) = generation.kind(*fragment_id) else {
            continue;
        };
        let Some(distance) =
            text_fragment_distance_to_point_for_glyph_offset(text_fragment, *point_in_fragment)
        else {
            continue;
        };
        if closest_idx.is_none() || distance <= closest_dist {
            closest_idx = Some(i);
            closest_dist = distance;
        }
    }
    closest_idx
}

pub fn find_character_offset_in_fragment_descendants(
    fragment_tree: &FragmentTree,
    layout_node: ServoLayoutNode<'_>,
    node: &ServoThreadSafeLayoutNode,
    point_in_viewport: Point2D<Au, CSSPixel>,
    scroll_offsets: &ScrollOffsets<'_>,
) -> Option<usize> {
    let scroll_offset = scroll_offsets.cumulative_scroll_offset(layout_node);
    let point_in_document = point_in_viewport + scroll_offset;
    let generation = fragment_tree.generation();
    let mut all_frags = Vec::new();
    for fragment_id in fragment_tree
        .fragments_for_node(node.opaque(), None)
        .iter()
        .copied()
    {
        collect_text_fragment_entries(&generation, fragment_id, point_in_document, &mut all_frags);
    }

    let idx = find_closest_text_fragment(&generation, &all_frags)?;
    let (fragment_id, point_in_fragment) = all_frags[idx];
    let published::FragmentKind::Text(text_fragment) = generation.kind(fragment_id) else {
        return None;
    };

    Some(text_fragment_character_offset(
        text_fragment,
        point_in_fragment,
        text_fragment.character_range_start as usize,
    ))
}

/// Like `find_character_offset_in_fragment_descendants`, but returns the OpaqueNode
/// of the text fragment and a character offset within the DOM text node (not just the fragment).
/// Used for document text selection where we need to identify the DOM text node.
pub fn find_text_node_and_offset_in_fragment_descendants(
    fragment_tree: &FragmentTree,
    layout_node: ServoLayoutNode<'_>,
    node: &ServoThreadSafeLayoutNode,
    point_in_viewport: Point2D<Au, CSSPixel>,
    scroll_offsets: &ScrollOffsets<'_>,
) -> Option<(OpaqueNode, usize)> {
    let scroll_offset = scroll_offsets.cumulative_scroll_offset(layout_node);
    let point_in_document = point_in_viewport + scroll_offset;
    let generation = fragment_tree.generation();
    let mut all_frags = Vec::new();
    for fragment_id in fragment_tree
        .fragments_for_node(node.opaque(), None)
        .iter()
        .copied()
    {
        collect_text_fragment_entries(&generation, fragment_id, point_in_document, &mut all_frags);
    }

    let idx = find_closest_text_fragment(&generation, &all_frags)?;
    let (fragment_id, closest_point) = all_frags[idx];
    let published::FragmentKind::Text(text_fragment) = generation.kind(fragment_id) else {
        return None;
    };
    let target_node = text_fragment.base.tag?.node;
    let frag_local_offset = text_fragment_character_offset(text_fragment, closest_point, 0);

    let mut chars_before = 0usize;
    for (i, (candidate_id, _)) in all_frags.iter().enumerate() {
        if i == idx {
            break;
        }
        let published::FragmentKind::Text(candidate) = generation.kind(*candidate_id) else {
            continue;
        };
        if candidate.base.tag.map(|tag| tag.node) == Some(target_node) {
            chars_before += candidate
                .glyphs
                .iter()
                .map(|glyph| glyph.char_count as usize)
                .sum::<usize>();
        }
    }

    Some((target_node, chars_before + frag_local_offset))
}

/// Find the closest text node and character offset to a viewport point by searching
/// all text fragments in the document. This works even when the hit-test node is a
/// container element, because it iterates every box fragment in the stacking context
/// tree rather than being scoped to one DOM node's fragments.
pub fn find_text_node_at_viewport_point(
    fragment_tree: &FragmentTree,
    point_in_viewport: Point2D<Au, CSSPixel>,
    scroll_offsets: &ScrollOffsets<'_>,
) -> Option<(OpaqueNode, usize)> {
    let root_id = ExternalScrollId(0, scroll_offsets.pipeline_id);
    let root_offset = scroll_offsets
        .offsets
        .get(&root_id)
        .map(|offset| euclid::Vector2D::<Au, CSSPixel>::new(
            Au::from_f32_px(offset.x),
            Au::from_f32_px(offset.y),
        ))
        .unwrap_or_default();
    let point_in_document = point_in_viewport + root_offset;
    let generation = fragment_tree.generation();
    let mut all_frags = Vec::new();
    for fragment_id in generation.geometry_roots.iter().copied() {
        collect_text_fragment_entries(&generation, fragment_id, point_in_document, &mut all_frags);
    }

    let idx = find_closest_text_fragment(&generation, &all_frags)?;
    let (fragment_id, closest_point) = all_frags[idx];
    let published::FragmentKind::Text(text_fragment) = generation.kind(fragment_id) else {
        return None;
    };
    let target_node = text_fragment.base.tag?.node;
    let frag_local_offset = text_fragment_character_offset(text_fragment, closest_point, 0);

    let mut chars_before = 0usize;
    for (i, (candidate_id, _)) in all_frags.iter().enumerate() {
        if i == idx {
            break;
        }
        let published::FragmentKind::Text(candidate) = generation.kind(*candidate_id) else {
            continue;
        };
        if candidate.base.tag.map(|tag| tag.node) == Some(target_node) {
            chars_before += candidate
                .glyphs
                .iter()
                .map(|glyph| glyph.char_count as usize)
                .sum::<usize>();
        }
    }

    Some((target_node, chars_before + frag_local_offset))
}

pub fn process_resolved_font_style_query<'dom, E>(
    context: &SharedStyleContext,
    node: E,
    value: &str,
    url_data: BrowserUrl,
    shared_lock: &SharedRwLock,
) -> Option<ServoArc<Font>>
where
    E: LayoutNode<'dom>,
{
    fn create_font_declaration(
        value: &str,
        url_data: &BrowserUrl,
        quirks_mode: QuirksMode,
    ) -> Option<PropertyDeclarationBlock> {
        let mut declarations = SourcePropertyDeclaration::default();
        let result = parse_one_declaration_into(
            &mut declarations,
            PropertyId::NonCustom(ShorthandId::Font.into()),
            value,
            Origin::Author,
            &UrlExtraData(url_data.get_arc()),
            None,
            ParsingMode::DEFAULT,
            quirks_mode,
            CssRuleType::Style,
        );
        let declarations = match result {
            Ok(()) => {
                let mut block = PropertyDeclarationBlock::new();
                block.extend(declarations.drain(), Importance::Normal);
                block
            },
            Err(_) => return None,
        };
        // TODO: Force to set line-height property to 'normal' font property.
        Some(declarations)
    }
    fn resolve_for_declarations<'dom, E>(
        context: &SharedStyleContext,
        parent_style: Option<&ComputedValues>,
        declarations: PropertyDeclarationBlock,
        shared_lock: &SharedRwLock,
    ) -> ServoArc<ComputedValues>
    where
        E: LayoutNode<'dom>,
    {
        let parent_style = match parent_style {
            Some(parent) => parent,
            None => context.stylist.device().default_computed_values(),
        };
        context
            .stylist
            .compute_for_declarations::<E::ConcreteElement>(
                &context.guards,
                parent_style,
                ServoArc::new(shared_lock.wrap(declarations)),
            )
    }

    // https://html.spec.whatwg.org/multipage/#dom-context-2d-font
    // 1. Parse the given font property value
    let quirks_mode = context.quirks_mode();
    let declarations = create_font_declaration(value, &url_data, quirks_mode)?;

    // TODO: Reject 'inherit' and 'initial' values for the font property.

    // 2. Get resolved styles for the parent element
    let element = node.as_element().unwrap();
    let parent_style = if node.is_connected() {
        if element.has_data() {
            node.to_threadsafe().as_element().unwrap().style(context)
        } else {
            let mut tlc = ThreadLocalStyleContext::new();
            let mut context = StyleContext {
                shared: context,
                thread_local: &mut tlc,
            };
            let styles = resolve_style(&mut context, element, RuleInclusion::All, None, None);
            styles.primary().clone()
        }
    } else {
        let default_declarations =
            create_font_declaration("10px sans-serif", &url_data, quirks_mode).unwrap();
        resolve_for_declarations::<E>(context, None, default_declarations, shared_lock)
    };

    // 3. Resolve the parsed value with resolved styles of the parent element
    let computed_values =
        resolve_for_declarations::<E>(context, Some(&*parent_style), declarations, shared_lock);

    Some(computed_values.clone_font())
}

/// Walk the fragment tree and return all elements whose border box contains the given point.
/// Results are ordered deepest-first (front to back).
pub fn query_elements_from_point(
    fragment_tree: &FragmentTree,
    point: webrender_api::units::LayoutPoint,
    _flags: layout_api::ElementsFromPointFlags,
    scroll_offsets: &ScrollOffsets<'_>,
) -> Vec<layout_api::ElementsFromPointResult> {
    use embedder_traits::Cursor;
    use style::computed_values::pointer_events::T as PointerEvents;
    use style::values::specified::ui::CursorKind;

    fn cursor_from_style(style: &ComputedValues) -> Cursor {
        match style.get_inherited_ui().cursor.keyword {
            CursorKind::Auto | CursorKind::Default => Cursor::Default,
            CursorKind::None => Cursor::None,
            CursorKind::Pointer => Cursor::Pointer,
            CursorKind::ContextMenu => Cursor::ContextMenu,
            CursorKind::Help => Cursor::Help,
            CursorKind::Progress => Cursor::Progress,
            CursorKind::Wait => Cursor::Wait,
            CursorKind::Cell => Cursor::Cell,
            CursorKind::Crosshair => Cursor::Crosshair,
            CursorKind::Text => Cursor::Text,
            CursorKind::VerticalText => Cursor::VerticalText,
            CursorKind::Alias => Cursor::Alias,
            CursorKind::Copy => Cursor::Copy,
            CursorKind::Move => Cursor::Move,
            CursorKind::NoDrop => Cursor::NoDrop,
            CursorKind::NotAllowed => Cursor::NotAllowed,
            CursorKind::Grab => Cursor::Grab,
            CursorKind::Grabbing => Cursor::Grabbing,
            CursorKind::EResize => Cursor::EResize,
            CursorKind::NResize => Cursor::NResize,
            CursorKind::NeResize => Cursor::NeResize,
            CursorKind::NwResize => Cursor::NwResize,
            CursorKind::SResize => Cursor::SResize,
            CursorKind::SeResize => Cursor::SeResize,
            CursorKind::SwResize => Cursor::SwResize,
            CursorKind::WResize => Cursor::WResize,
            CursorKind::EwResize => Cursor::EwResize,
            CursorKind::NsResize => Cursor::NsResize,
            CursorKind::NeswResize => Cursor::NeswResize,
            CursorKind::NwseResize => Cursor::NwseResize,
            CursorKind::ColResize => Cursor::ColResize,
            CursorKind::RowResize => Cursor::RowResize,
            CursorKind::AllScroll => Cursor::AllScroll,
            CursorKind::ZoomIn => Cursor::ZoomIn,
            CursorKind::ZoomOut => Cursor::ZoomOut,
        }
    }

    fn push_svg_hit_test_result(
        identity: &published::SVGFragmentIdentity,
        point: Point2D<f32, CSSPixel>,
        rect: Rect<f32, CSSPixel>,
        style: &ComputedValues,
        results: &mut Vec<layout_api::ElementsFromPointResult>,
    ) {
        results.push(layout_api::ElementsFromPointResult {
            node: identity.source_tag.node,
            point_in_target: Point2D::new(
                point.x - rect.origin.x,
                point.y - rect.origin.y,
            ),
            cursor: cursor_from_style(style),
        });
    }

    fn absolute_rect(
        generation: &published::FragmentArenaGeneration,
        fragment_id: published::FragmentId,
        root_scroll_offset: euclid::Vector2D<Au, CSSPixel>,
    ) -> Option<Rect<f32, CSSPixel>> {
        let rect = match generation.kind(fragment_id) {
            published::FragmentKind::Box(box_fragment) | published::FragmentKind::Float(box_fragment) => {
                box_fragment
                    .border_rect()
                    .translate(generation.containing_block(fragment_id).origin.to_vector())
            }
            published::FragmentKind::Text(text_fragment) => PhysicalRect::new(
                generation.containing_block(fragment_id).origin + text_fragment.base.rect.origin.to_vector(),
                text_fragment.base.rect.size,
            ),
            published::FragmentKind::Image(image_fragment) => PhysicalRect::new(
                generation.containing_block(fragment_id).origin + image_fragment.base.rect.origin.to_vector(),
                image_fragment.base.rect.size,
            ),
            published::FragmentKind::IFrame(iframe_fragment) => PhysicalRect::new(
                generation.containing_block(fragment_id).origin + iframe_fragment.base.rect.origin.to_vector(),
                iframe_fragment.base.rect.size,
            ),
            published::FragmentKind::SVGViewport(svg_fragment) => PhysicalRect::new(
                generation.containing_block(fragment_id).origin + svg_fragment.base.rect.origin.to_vector(),
                svg_fragment.base.rect.size,
            ),
            published::FragmentKind::SVGContainer(svg_fragment) => {
                let (transform, origin) = accumulated_svg_transform_and_origin(generation, fragment_id)?;
                transform_svg_rect(svg_fragment.base.rect, transform).translate(origin)
            }
            published::FragmentKind::SVGLeaf(svg_fragment) => {
                let (transform, origin) = accumulated_svg_transform_and_origin(generation, fragment_id)?;
                transform_svg_rect(svg_fragment.base.rect, transform).translate(origin)
            }
            published::FragmentKind::Positioning(_) => return None,
        };
        let rect = if fragment_is_fixed_positioned(generation, fragment_id) {
            rect
        } else {
            rect.translate(-root_scroll_offset)
        };
        Some(Rect::new(
            Point2D::new(rect.origin.x.to_f32_px(), rect.origin.y.to_f32_px()),
            Size2D::new(rect.size.width.to_f32_px(), rect.size.height.to_f32_px()),
        ))
    }

    fn hit_test_paint_child(
        generation: &published::FragmentArenaGeneration,
        child: &published::PaintChild,
        point: Point2D<f32, CSSPixel>,
        root_scroll_offset: euclid::Vector2D<Au, CSSPixel>,
        results: &mut Vec<layout_api::ElementsFromPointResult>,
    ) {
        match child {
            published::PaintChild::Fragment(fragment_id) => {
                hit_test_fragment(generation, *fragment_id, point, root_scroll_offset, results)
            }
            published::PaintChild::Placement(placement_id) => {
                hit_test_fragment(
                    generation,
                    generation.placement(*placement_id).fragment,
                    point,
                    root_scroll_offset,
                    results,
                )
            }
        }
    }

    fn hit_test_fragment(
        generation: &published::FragmentArenaGeneration,
        fragment_id: published::FragmentId,
        point: Point2D<f32, CSSPixel>,
        root_scroll_offset: euclid::Vector2D<Au, CSSPixel>,
        results: &mut Vec<layout_api::ElementsFromPointResult>,
    ) {
        let base = generation.base(fragment_id);
        if base.style.get_inherited_ui().pointer_events == PointerEvents::None {
            return;
        }
        if base.style.get_inherited_box().visibility != Visibility::Visible {
            return;
        }

        match generation.kind(fragment_id) {
            published::FragmentKind::Box(box_fragment) | published::FragmentKind::Float(box_fragment) => {
                let Some(border_rect) = absolute_rect(generation, fragment_id, root_scroll_offset) else {
                    return;
                };
                if point.x < border_rect.origin.x ||
                    point.x > border_rect.origin.x + border_rect.size.width ||
                    point.y < border_rect.origin.y ||
                    point.y > border_rect.origin.y + border_rect.size.height
                {
                    return;
                }
                for child in box_fragment.paint_children.iter().rev() {
                    hit_test_paint_child(generation, child, point, root_scroll_offset, results);
                }
                if let Some(tag) = box_fragment.base.tag {
                    results.push(layout_api::ElementsFromPointResult {
                        node: tag.node,
                        point_in_target: Point2D::new(
                            point.x - border_rect.origin.x,
                            point.y - border_rect.origin.y,
                        ),
                        cursor: cursor_from_style(&box_fragment.base.style),
                    });
                }
            }
            published::FragmentKind::Positioning(positioning_fragment) => {
                for child in positioning_fragment.paint_children.iter().rev() {
                    hit_test_paint_child(generation, child, point, root_scroll_offset, results);
                }
            }
            published::FragmentKind::SVGViewport(svg_fragment) => {
                let Some(rect) = absolute_rect(generation, fragment_id, root_scroll_offset) else {
                    return;
                };
                if point.x < rect.origin.x ||
                    point.x > rect.origin.x + rect.size.width ||
                    point.y < rect.origin.y ||
                    point.y > rect.origin.y + rect.size.height
                {
                    return;
                }
                for child in svg_fragment.paint_children.iter().rev() {
                    hit_test_paint_child(generation, child, point, root_scroll_offset, results);
                }
                push_svg_hit_test_result(
                    &svg_fragment.identity,
                    point,
                    rect,
                    &svg_fragment.base.style,
                    results,
                );
            }
            published::FragmentKind::SVGContainer(svg_fragment) => {
                for child in svg_fragment.paint_children.iter().rev() {
                    hit_test_paint_child(generation, child, point, root_scroll_offset, results);
                }
                if let Some(rect) = absolute_rect(generation, fragment_id, root_scroll_offset) {
                    if point.x >= rect.origin.x && point.x <= rect.origin.x + rect.size.width &&
                        point.y >= rect.origin.y && point.y <= rect.origin.y + rect.size.height
                    {
                        push_svg_hit_test_result(
                            &svg_fragment.identity,
                            point,
                            rect,
                            &svg_fragment.base.style,
                            results,
                        );
                    }
                }
            }
            published::FragmentKind::Text(text_fragment) => {
                let Some(rect) = absolute_rect(generation, fragment_id, root_scroll_offset) else {
                    return;
                };
                if point.x >= rect.origin.x && point.x <= rect.origin.x + rect.size.width &&
                    point.y >= rect.origin.y && point.y <= rect.origin.y + rect.size.height
                {
                    if let Some(tag) = text_fragment.base.tag {
                        results.push(layout_api::ElementsFromPointResult {
                            node: tag.node,
                            point_in_target: Point2D::new(
                                point.x - rect.origin.x,
                                point.y - rect.origin.y,
                            ),
                            cursor: cursor_from_style(&text_fragment.base.style),
                        });
                    }
                }
            }
            published::FragmentKind::SVGLeaf(svg_fragment) => {
                let Some(rect) = absolute_rect(generation, fragment_id, root_scroll_offset) else {
                    return;
                };
                if point.x < rect.origin.x ||
                    point.x > rect.origin.x + rect.size.width ||
                    point.y < rect.origin.y ||
                    point.y > rect.origin.y + rect.size.height
                {
                    return;
                }
                let hit = match &svg_fragment.kind {
                    published::SVGLeafKind::Path(path) => {
                        let svg_point = published::SVGPoint::new(
                            point.x - rect.origin.x + svg_fragment.bounds.visual_bounding_box.origin.x,
                            point.y - rect.origin.y + svg_fragment.bounds.visual_bounding_box.origin.y,
                        );
                        hit_test_svg_path(
                            &path.path,
                            svg_fragment.paint.stroke.as_ref(),
                            svg_point,
                        )
                        .hit
                    }
                    published::SVGLeafKind::Text(text) => {
                        let hits_text_run = text.runs.iter().any(|text_run| {
                            let run_rect = PhysicalRect::new(
                                generation.containing_block(fragment_id).origin +
                                    text_run.rect.origin.to_vector(),
                                text_run.rect.size,
                            );
                            let run_rect = if fragment_is_fixed_positioned(generation, fragment_id) {
                                run_rect
                            } else {
                                run_rect.translate(-root_scroll_offset)
                            };
                            let run_rect: Rect<f32, CSSPixel> = Rect::new(
                                Point2D::new(run_rect.origin.x.to_f32_px(), run_rect.origin.y.to_f32_px()),
                                Size2D::new(run_rect.size.width.to_f32_px(), run_rect.size.height.to_f32_px()),
                            );
                            point.x >= run_rect.origin.x &&
                                point.x <= run_rect.origin.x + run_rect.size.width &&
                                point.y >= run_rect.origin.y &&
                                point.y <= run_rect.origin.y + run_rect.size.height
                        });
                        hits_text_run ||
                            (text.runs.is_empty() &&
                                point.x >= rect.origin.x &&
                                point.x <= rect.origin.x + rect.size.width &&
                                point.y >= rect.origin.y &&
                                point.y <= rect.origin.y + rect.size.height)
                    }
                    published::SVGLeafKind::Image(_) => {
                        point.x >= rect.origin.x && point.x <= rect.origin.x + rect.size.width &&
                            point.y >= rect.origin.y && point.y <= rect.origin.y + rect.size.height
                    }
                };
                if hit {
                    push_svg_hit_test_result(
                        &svg_fragment.identity,
                        point,
                        rect,
                        &svg_fragment.base.style,
                        results,
                    );
                }
            }
            published::FragmentKind::Image(image_fragment) => {
                let Some(rect) = absolute_rect(generation, fragment_id, root_scroll_offset) else {
                    return;
                };
                if point.x >= rect.origin.x && point.x <= rect.origin.x + rect.size.width &&
                    point.y >= rect.origin.y && point.y <= rect.origin.y + rect.size.height
                {
                    if let Some(tag) = image_fragment.base.tag {
                        results.push(layout_api::ElementsFromPointResult {
                            node: tag.node,
                            point_in_target: Point2D::new(
                                point.x - rect.origin.x,
                                point.y - rect.origin.y,
                            ),
                            cursor: cursor_from_style(&image_fragment.base.style),
                        });
                    }
                }
            }
            published::FragmentKind::IFrame(iframe_fragment) => {
                let Some(rect) = absolute_rect(generation, fragment_id, root_scroll_offset) else {
                    return;
                };
                if point.x >= rect.origin.x && point.x <= rect.origin.x + rect.size.width &&
                    point.y >= rect.origin.y && point.y <= rect.origin.y + rect.size.height
                {
                    if let Some(tag) = iframe_fragment.base.tag {
                        results.push(layout_api::ElementsFromPointResult {
                            node: tag.node,
                            point_in_target: Point2D::new(
                                point.x - rect.origin.x,
                                point.y - rect.origin.y,
                            ),
                            cursor: Cursor::Default,
                        });
                    }
                }
            }

        }
    }

    let generation = fragment_tree.generation();
    let root_id = ExternalScrollId(0, scroll_offsets.pipeline_id);
    let root_scroll_offset = scroll_offsets
        .offsets
        .get(&root_id)
        .map(|offset| euclid::Vector2D::<Au, CSSPixel>::new(
            Au::from_f32_px(offset.x),
            Au::from_f32_px(offset.y),
        ))
        .unwrap_or_default();
    let point: Point2D<f32, CSSPixel> = Point2D::new(point.x, point.y);
    let mut results = Vec::new();
    for child in generation.paint_roots.iter().rev() {
        hit_test_paint_child(&generation, child, point, root_scroll_offset, &mut results);
    }
    results
}


pub(crate) fn process_effective_overflow_query(
    fragment_tree: &FragmentTree,
    node: ServoThreadSafeLayoutNode<'_>,
) -> Option<AxesOverflow> {
    let fragment_id = first_fragment_id(fragment_tree, node, None)?;
    match fragment_tree.generation().kind(fragment_id) {
        published::FragmentKind::Box(box_fragment) | published::FragmentKind::Float(box_fragment) => {
            Some(box_fragment.base.style.effective_overflow(FragmentFlags::from_bits_retain(
                box_fragment.base.flags.bits(),
            )))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use app_units::Au;
    use havi_types::fragment_tree as published;
    use style::dom::OpaqueNode;
    use style::properties::ComputedValues;
    use style::properties::style_structs::Font;

    use super::box_area_rect;
    use crate::geom::{PhysicalPoint, PhysicalRect, PhysicalSize};

    fn initial_style() -> servo_arc::Arc<ComputedValues> {
        ComputedValues::initial_values_with_font_override(Font::initial_values()).to_arc()
    }

    fn rect(x: i32, y: i32, width: i32, height: i32) -> PhysicalRect<Au> {
        PhysicalRect::new(
            PhysicalPoint::new(Au::from_px(x), Au::from_px(y)),
            PhysicalSize::new(Au::from_px(width), Au::from_px(height)),
        )
    }

    fn base_fragment(id: usize, rect: PhysicalRect<Au>) -> published::BaseFragment {
        published::BaseFragment::new(
            published::BaseFragmentInfo::new(OpaqueNode(id), None),
            initial_style(),
            rect,
        )
    }

    fn svg_identity(id: usize) -> published::SVGFragmentIdentity {
        published::SVGFragmentIdentity {
            source_tag: published::Tag {
                node: OpaqueNode(id),
                pseudo: None,
            },
            instance_chain: None,
        }
    }

    fn generation(nodes: Vec<published::FragmentNode>) -> published::FragmentArenaGeneration {
        let len = nodes.len();
        published::FragmentArenaGeneration {
            geometry_roots: Arc::from([]),
            paint_roots: Arc::from([]),
            nodes: nodes.into(),
            placements: Arc::from([]),
            derived: published::FragmentDerivedData {
                containing_blocks: vec![PhysicalRect::zero(); len],
                scrollable_overflow: vec![PhysicalRect::zero(); len],
                sticky_insets: vec![None; len],
                background_images: vec![Vec::new(); len],
            },
            node_fragments: HashMap::new(),
            svg_resources: Arc::from([]),
            initial_containing_block: PhysicalRect::zero(),
            scrollable_overflow: PhysicalRect::zero(),
        }
    }

    #[test]
    fn svg_box_area_accumulates_ancestor_container_transform() {
        let generation = generation(vec![
            published::FragmentNode {
                parent: None,
                kind: published::FragmentKind::SVGViewport(published::SVGViewportFragment {
                    base: base_fragment(1, rect(0, 0, 200, 100)),
                    identity: svg_identity(1),
                    geometry_children: vec![published::FragmentId(1)],
                    paint_children: Vec::new(),
                    viewport_rect: published::SVGRect::new(
                        published::SVGPoint::new(0.0, 0.0),
                        crate::geom::PhysicalSize::new(200.0, 100.0),
                    ),
                    view_box_rect: None,
                    local_to_parent_transform: published::SVGTransform::identity(),
                    overflow_clip: None,
                }),
            },
            published::FragmentNode {
                parent: Some(published::FragmentId(0)),
                kind: published::FragmentKind::SVGContainer(published::SVGContainerFragment {
                    base: base_fragment(2, rect(0, 0, 20, 20)),
                    identity: svg_identity(2),
                    kind: published::SVGContainerKind::Group,
                    geometry_children: vec![published::FragmentId(2)],
                    paint_children: Vec::new(),
                    local_transform: published::SVGTransform::new(1.0, 0.0, 0.0, 1.0, 60.0, 0.0),
                    effects: Default::default(),
                }),
            },
            published::FragmentNode {
                parent: Some(published::FragmentId(1)),
                kind: published::FragmentKind::SVGLeaf(published::SVGLeafFragment {
                    base: base_fragment(3, rect(0, 0, 20, 20)),
                    identity: svg_identity(3),
                    kind: published::SVGLeafKind::Path(published::SVGPathPayload {
                        path: published::SVGPathData {
                            fill_rule: published::SVGFillRule::NonZero,
                            commands: Vec::new(),
                        },
                    }),
                    bounds: Default::default(),
                    local_transform: published::SVGTransform::identity(),
                    paint: Default::default(),
                    effects: Default::default(),
                }),
            },
        ]);

        let rect = box_area_rect(&generation, published::FragmentId(2), layout_api::BoxAreaType::Border)
            .expect("svg leaf should have a border box");
        assert_eq!(rect.origin.x, Au::from_px(60));
        assert_eq!(rect.origin.y, Au::from_px(0));
        assert_eq!(rect.size.width, Au::from_px(20));
        assert_eq!(rect.size.height, Au::from_px(20));
    }

    #[test]
    fn svg_box_area_accumulates_viewport_mapper_transform() {
        let generation = generation(vec![
            published::FragmentNode {
                parent: None,
                kind: published::FragmentKind::SVGViewport(published::SVGViewportFragment {
                    base: base_fragment(1, rect(0, 0, 200, 100)),
                    identity: svg_identity(1),
                    geometry_children: vec![published::FragmentId(1)],
                    paint_children: Vec::new(),
                    viewport_rect: published::SVGRect::new(
                        published::SVGPoint::new(0.0, 0.0),
                        crate::geom::PhysicalSize::new(200.0, 100.0),
                    ),
                    view_box_rect: Some(published::SVGRect::new(
                        published::SVGPoint::new(0.0, 0.0),
                        crate::geom::PhysicalSize::new(100.0, 100.0),
                    )),
                    local_to_parent_transform: published::SVGTransform::new(1.0, 0.0, 0.0, 1.0, 50.0, 0.0),
                    overflow_clip: None,
                }),
            },
            published::FragmentNode {
                parent: Some(published::FragmentId(0)),
                kind: published::FragmentKind::SVGLeaf(published::SVGLeafFragment {
                    base: base_fragment(2, rect(0, 0, 100, 100)),
                    identity: svg_identity(2),
                    kind: published::SVGLeafKind::Path(published::SVGPathPayload {
                        path: published::SVGPathData {
                            fill_rule: published::SVGFillRule::NonZero,
                            commands: Vec::new(),
                        },
                    }),
                    bounds: Default::default(),
                    local_transform: published::SVGTransform::identity(),
                    paint: Default::default(),
                    effects: Default::default(),
                }),
            },
        ]);

        let rect = box_area_rect(&generation, published::FragmentId(1), layout_api::BoxAreaType::Border)
            .expect("svg leaf should have a border box");
        assert_eq!(rect.origin.x, Au::from_px(50));
        assert_eq!(rect.origin.y, Au::from_px(0));
        assert_eq!(rect.size.width, Au::from_px(100));
        assert_eq!(rect.size.height, Au::from_px(100));
    }
}
