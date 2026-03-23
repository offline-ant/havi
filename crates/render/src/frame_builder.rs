//! Legacy `RenderScene` builder used only by the fallback render path.
//!
//! This path remains only for `HAVI_DISABLE_BROWSER_SCENE=1` and direct-builder
//! fallback failures.

use app_units::Au;
use havi_fragment_semantics::fragment_tree::{BoxFragment, FragmentFlags};
use havi_fragment_semantics::{Fragment, IFrameFragment};
use havi_types::PhysicalRect;
use makepad_compositor::{MpBackfaceVisibility, MpTransformStyle};
use makepad_widgets::*;
use style::computed_values::mix_blend_mode::T as ComputedMixBlendMode;
use style::computed_values::overflow_x::T as ComputedOverflow;
use style::values::computed::basic_shape::ClipPath;
use style::values::computed::effects::Filter as ComputedFilter;
use style::values::computed::ClipRectOrAuto;

use crate::background::resolve_border_radii;
use crate::layout_stacking_context::StackingContextSection;
use crate::reference_frame::reference_frame_semantics;
use crate::scene::{
    RenderBlendMode, RenderClip, RenderClipGeometry, RenderClipId, RenderEffect, RenderEmbed,
    RenderFilterSet, RenderMask, RenderNodeId, RenderPaintItem, RenderPaintRun,
    RenderReferenceFrame, RenderReferenceFrameKind, RenderScene, RenderScrollInfo,
};
use crate::scene_builder::RenderSceneBuilder;

#[derive(Clone, Copy)]
struct BuildContext {
    parent_node_id: RenderNodeId,
    active_clip: Option<RenderClipId>,
    containing_block_origin: DVec2,
}

pub(crate) type BuiltScene<'a> = RenderScene<'a>;

pub(crate) fn build_scene<'a>(
    fragments: &'a [Fragment],
    scroll_state: &crate::ScrollState,
    viewport_size: DVec2,
) -> BuiltScene<'a> {
    let mut scene_builder = RenderSceneBuilder::new();
    scene_builder.root_reference_frame_mut().local_rect = Rect {
        pos: dvec2(0.0, 0.0),
        size: viewport_size,
    };

    let root = scene_builder.root_reference_frame_id();
    let cx = BuildContext {
        parent_node_id: root,
        active_clip: None,
        containing_block_origin: dvec2(0.0, 0.0),
    };
    build_fragment_list(fragments, scroll_state, &mut scene_builder, cx);
    scene_builder.build()
}

fn build_fragment_list<'a>(
    fragments: &'a [Fragment],
    scroll_state: &crate::ScrollState,
    scene_builder: &mut RenderSceneBuilder<'a>,
    cx: BuildContext,
) {
    for fragment in fragments {
        build_fragment(fragment, scroll_state, scene_builder, cx);
    }
}

fn build_fragment<'a>(
    fragment: &'a Fragment,
    scroll_state: &crate::ScrollState,
    scene_builder: &mut RenderSceneBuilder<'a>,
    cx: BuildContext,
) {
    match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => {
            build_box_fragment(fragment, bf, scroll_state, scene_builder, cx);
        }
        Fragment::Text(text) => {
            if !text.base.flags.intersects(FragmentFlags::DO_NOT_PAINT) {
                build_leaf_fragment(fragment, scene_builder, cx, StackingContextSection::Foreground);
            }
        }
        Fragment::Image(image) => {
            if !image.base.flags.intersects(FragmentFlags::DO_NOT_PAINT) {
                build_leaf_fragment(fragment, scene_builder, cx, StackingContextSection::Foreground);
            }
        }
        Fragment::IFrame(iframe) => {
            if !iframe.base.flags.intersects(FragmentFlags::DO_NOT_PAINT) {
                build_iframe_fragment(fragment, iframe, scroll_state, scene_builder, cx);
            }
        }
        Fragment::Positioning(positioning) => {
            build_fragment_list(&positioning.children, scroll_state, scene_builder, cx);
        }
        Fragment::AbsoluteOrFixedPositioned { resolved } => {
            build_fragment(resolved, scroll_state, scene_builder, cx);
        }
    }
}

fn build_box_fragment<'a>(
    fragment: &'a Fragment,
    bf: &'a BoxFragment,
    scroll_state: &crate::ScrollState,
    scene_builder: &mut RenderSceneBuilder<'a>,
    cx: BuildContext,
) {
    if bf.base.flags.intersects(FragmentFlags::DO_NOT_PAINT) {
        return;
    }

    let border_rect = physical_rect_to_rect(bf.border_rect());
    let content_rect = physical_rect_to_rect(bf.content_rect());
    let box_origin_in_parent = cx.containing_block_origin + border_rect.pos;
    let owner_node_id = owner_node_id_for_box(bf);

    let mut parent_node_id = cx.parent_node_id;
    let mut active_clip = cx.active_clip;
    let mut uses_box_local_basis = false;

    if let Some(semantics) = reference_frame_semantics(bf, box_origin_in_parent) {
        parent_node_id = scene_builder.push_reference_frame(RenderReferenceFrame {
            parent: Some(parent_node_id),
            clip: active_clip,
            local_rect: Rect {
                pos: dvec2(0.0, 0.0),
                size: border_rect.size,
            },
            placement_origin: box_origin_in_parent,
            transform: semantics.transform_matrix,
            perspective: semantics.perspective_matrix,
            transform_style: semantics.transform_style,
            flattens_descendants: semantics.flattens_descendants,
            backface_visibility: semantics.backface_visibility,
            kind: RenderReferenceFrameKind::Transform,
        });
        uses_box_local_basis = true;
    }

    if let Some(scroll_info) = scroll_info_for_box(bf, scroll_state, border_rect.size) {
        parent_node_id = scene_builder.push_reference_frame(RenderReferenceFrame {
            parent: Some(parent_node_id),
            clip: active_clip,
            local_rect: Rect {
                pos: dvec2(0.0, 0.0),
                size: border_rect.size,
            },
            placement_origin: if uses_box_local_basis {
                dvec2(0.0, 0.0)
            } else {
                box_origin_in_parent
            },
            transform: None,
            perspective: None,
            transform_style: MpTransformStyle::Flat,
            flattens_descendants: true,
            backface_visibility: MpBackfaceVisibility::Visible,
            kind: RenderReferenceFrameKind::Scroll(scroll_info),
        });
        uses_box_local_basis = true;
    }

    let box_local_bounds = if uses_box_local_basis {
        Rect {
            pos: dvec2(0.0, 0.0),
            size: border_rect.size,
        }
    } else {
        Rect {
            pos: box_origin_in_parent,
            size: border_rect.size,
        }
    };

    if let Some(rect) = css_clip_rect(bf) {
        active_clip = Some(scene_builder.push_clip(RenderClip {
            parent: Some(parent_node_id),
            prev: active_clip,
            geometry: RenderClipGeometry::Rect {
                rect: map_box_rect_to_parent_space(
                    rect,
                    cx.containing_block_origin,
                    border_rect.pos,
                    uses_box_local_basis,
                ),
            },
        }));
    }

    if needs_overflow_clip(bf) {
        if let Some(rect) = bf.scrollable_overflow.map(physical_rect_to_rect) {
            let radius = resolve_border_radii(&bf.base.style).max();
            let rect = map_box_rect_to_parent_space(
                rect,
                cx.containing_block_origin,
                border_rect.pos,
                uses_box_local_basis,
            );
            let geometry = if radius > 0.0 {
                RenderClipGeometry::RoundedRect { rect, radius }
            } else {
                RenderClipGeometry::Rect { rect }
            };
            active_clip = Some(scene_builder.push_clip(RenderClip {
                parent: Some(parent_node_id),
                prev: active_clip,
                geometry,
            }));
        }
    }

    if let Some(effect) = effect_for_box(bf, parent_node_id, active_clip, box_local_bounds) {
        parent_node_id = scene_builder.push_effect(effect);
    }

    let item_origin = if uses_box_local_basis {
        dvec2(-border_rect.pos.x, -border_rect.pos.y)
    } else {
        cx.containing_block_origin
    };
    push_single_item_run(
        scene_builder,
        parent_node_id,
        active_clip,
        owner_node_id,
        box_local_bounds,
        RenderPaintItem {
            section: StackingContextSection::OwnBackgroundsAndBorders,
            local_origin: item_origin,
            source: fragment,
        },
    );

    let child_cx = BuildContext {
        parent_node_id,
        active_clip,
        containing_block_origin: if uses_box_local_basis {
            content_rect.pos - border_rect.pos
        } else {
            cx.containing_block_origin + content_rect.pos
        },
    };
    build_fragment_list(&bf.children, scroll_state, scene_builder, child_cx);
}

fn build_leaf_fragment<'a>(
    fragment: &'a Fragment,
    scene_builder: &mut RenderSceneBuilder<'a>,
    cx: BuildContext,
    section: StackingContextSection,
) {
    let local_bounds = fragment_local_bounds(fragment, cx.containing_block_origin);
    push_single_item_run(
        scene_builder,
        cx.parent_node_id,
        cx.active_clip,
        fragment_owner_node_id(fragment),
        local_bounds,
        RenderPaintItem {
            section,
            local_origin: cx.containing_block_origin,
            source: fragment,
        },
    );
}

fn build_iframe_fragment<'a>(
    fragment: &'a Fragment,
    iframe: &'a IFrameFragment,
    scroll_state: &crate::ScrollState,
    scene_builder: &mut RenderSceneBuilder<'a>,
    cx: BuildContext,
) {
    let content_bounds = fragment_local_bounds(fragment, cx.containing_block_origin);
    let border_bounds = outset_rect(content_bounds, box_content_insets(&iframe.base.style));
    let iframe_rect = physical_rect_to_rect(iframe.base.rect);
    push_single_item_run(
        scene_builder,
        cx.parent_node_id,
        cx.active_clip,
        iframe.base.tag.map(|tag| tag.node.0),
        border_bounds,
        RenderPaintItem {
            section: StackingContextSection::Foreground,
            local_origin: border_bounds.pos - iframe_rect.pos,
            source: fragment,
        },
    );

    let child_scene = build_scene(iframe.child_fragments.as_ref(), scroll_state, content_bounds.size);
    scene_builder.push_embed(RenderEmbed {
        parent: cx.parent_node_id,
        clip: cx.active_clip,
        local_rect: content_bounds,
        owner_node_id: iframe.base.tag.map(|tag| tag.node.0),
        child_scene: Box::new(child_scene),
    });
}

fn push_single_item_run<'a>(
    scene_builder: &mut RenderSceneBuilder<'a>,
    parent: RenderNodeId,
    clip: Option<RenderClipId>,
    owner_node_id: Option<usize>,
    local_bounds: Rect,
    item: RenderPaintItem<'a>,
) {
    let run_id = scene_builder.push_paint_run(RenderPaintRun {
        parent,
        clip,
        owner_node_id,
        local_bounds,
        items: Vec::new(),
    });
    scene_builder.push_item(run_id, item);
}

fn fragment_local_bounds(fragment: &Fragment, containing_block_origin: DVec2) -> Rect {
    let rect = match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => physical_rect_to_rect(bf.border_rect()),
        Fragment::Text(text) => physical_rect_to_rect(text.base.rect),
        Fragment::Image(image) => physical_rect_to_rect(image.base.rect),
        Fragment::IFrame(iframe) => physical_rect_to_rect(iframe.base.rect),
        Fragment::Positioning(_) | Fragment::AbsoluteOrFixedPositioned { .. } => Rect {
            pos: dvec2(0.0, 0.0),
            size: dvec2(0.0, 0.0),
        },
    };
    Rect {
        pos: containing_block_origin + rect.pos,
        size: rect.size,
    }
}

fn serialize_filter_entry(filter: &ComputedFilter) -> String {
    match filter {
        ComputedFilter::Blur(radius) => format!("blur({})", radius.0.px()),
        ComputedFilter::Opacity(opacity) => format!("opacity({})", opacity.0),
        other => format!("unsupported:{other:?}"),
    }
}

fn effect_for_box(
    bf: &BoxFragment,
    parent: RenderNodeId,
    clip: Option<RenderClipId>,
    local_rect: Rect,
) -> Option<RenderEffect> {
    let effects = bf.base.style.get_effects();
    let svg = bf.base.style.get_svg();
    let opacity = effects.opacity;
    let filter_entries: Vec<String> = effects
        .filter
        .0
        .iter()
        .map(serialize_filter_entry)
        .collect();
    let blend_mode = if effects.mix_blend_mode == ComputedMixBlendMode::Normal {
        RenderBlendMode::Normal
    } else {
        RenderBlendMode::Named(format!("{:?}", effects.mix_blend_mode))
    };
    let mask = (svg.clip_path != ClipPath::None).then_some(RenderMask::Rect { rect: local_rect });
    let is_isolated = opacity != 1.0
        || !filter_entries.is_empty()
        || !matches!(blend_mode, RenderBlendMode::Normal)
        || mask.is_some();
    if !is_isolated {
        return None;
    }
    Some(RenderEffect {
        parent,
        clip,
        opacity,
        filter: RenderFilterSet {
            entries: filter_entries,
        },
        blend_mode,
        is_isolated,
        mask,
    })
}

fn needs_overflow_clip(bf: &BoxFragment) -> bool {
    let overflow = bf.base.style.get_box();
    !matches!(overflow.overflow_x, ComputedOverflow::Visible)
        || !matches!(overflow.overflow_y, ComputedOverflow::Visible)
}

fn scroll_info_for_box(
    bf: &BoxFragment,
    scroll_state: &crate::ScrollState,
    size: DVec2,
) -> Option<RenderScrollInfo> {
    if !needs_overflow_clip(bf) {
        return None;
    }
    bf.scrollable_overflow?;
    let scroll_offset = bf
        .base
        .tag
        .and_then(|tag| scroll_state.get(&tag.node.0).copied())
        .unwrap_or_else(|| dvec2(0.0, 0.0));
    let overflow = bf.base.style.get_box();
    Some(RenderScrollInfo {
        scroll_offset,
        scroll_frame_rect: Rect {
            pos: dvec2(0.0, 0.0),
            size,
        },
        sensitivity_x: matches!(overflow.overflow_x, ComputedOverflow::Auto | ComputedOverflow::Scroll),
        sensitivity_y: matches!(overflow.overflow_y, ComputedOverflow::Auto | ComputedOverflow::Scroll),
        external_scroll_node_id: bf.base.tag.map(|tag| tag.node.0),
    })
}

fn css_clip_rect(bf: &BoxFragment) -> Option<Rect> {
    if !bf.base.style.get_box().position.is_absolutely_positioned() {
        return None;
    }
    let clip_rect = match bf.base.style.get_effects().clip {
        ClipRectOrAuto::Rect(rect) => rect,
        _ => return None,
    };
    Some(physical_rect_to_rect(clip_rect.for_border_rect(bf.border_rect())))
}

fn map_box_rect_to_parent_space(
    rect: Rect,
    containing_block_origin: DVec2,
    border_box_origin: DVec2,
    uses_box_local_basis: bool,
) -> Rect {
    Rect {
        pos: if uses_box_local_basis {
            rect.pos - border_box_origin
        } else {
            containing_block_origin + rect.pos
        },
        size: rect.size,
    }
}

fn owner_node_id_for_box(bf: &BoxFragment) -> Option<usize> {
    let node_id = bf.base.tag.map(|tag| tag.node.0)?;
    let pseudo_key = match bf.base.style.pseudo() {
        Some(style::selector_parser::PseudoElement::Before) => 1,
        Some(style::selector_parser::PseudoElement::After) => 2,
        Some(style::selector_parser::PseudoElement::Marker) => 3,
        Some(style::selector_parser::PseudoElement::ServoAnonymousBox) => 4,
        Some(style::selector_parser::PseudoElement::ServoAnonymousTable) => 5,
        Some(style::selector_parser::PseudoElement::ServoAnonymousTableCell) => 6,
        Some(style::selector_parser::PseudoElement::ServoAnonymousTableRow) => 7,
        Some(_) => 15,
        None => 0,
    };
    Some((node_id << 8) ^ pseudo_key)
}

fn fragment_owner_node_id(fragment: &Fragment) -> Option<usize> {
    match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => owner_node_id_for_box(bf),
        Fragment::Text(text) => text.base.tag.map(|tag| tag.node.0),
        Fragment::Image(image) => image.base.tag.map(|tag| tag.node.0),
        Fragment::IFrame(iframe) => iframe.base.tag.map(|tag| tag.node.0),
        Fragment::Positioning(positioning) => positioning.base.tag.map(|tag| tag.node.0),
        Fragment::AbsoluteOrFixedPositioned { .. } => None,
    }
}

fn box_content_insets(style: &style::properties::ComputedValues) -> (f64, f64, f64, f64) {
    use style::values::specified::border::BorderStyle;

    let border = style.get_border();
    let border_width = |style: BorderStyle, width: style::values::computed::BorderSideWidth| -> f64 {
        if matches!(style, BorderStyle::None | BorderStyle::Hidden) {
            0.0
        } else {
            width.0.to_f32_px().max(0.0) as f64
        }
    };
    let padding = style.get_padding();
    (
        border_width(border.clone_border_left_style(), border.clone_border_left_width())
            + padding.padding_left.0.to_length().map_or(0.0, |l| l.px()) as f64,
        border_width(border.clone_border_top_style(), border.clone_border_top_width())
            + padding.padding_top.0.to_length().map_or(0.0, |l| l.px()) as f64,
        border_width(border.clone_border_right_style(), border.clone_border_right_width())
            + padding.padding_right.0.to_length().map_or(0.0, |l| l.px()) as f64,
        border_width(border.clone_border_bottom_style(), border.clone_border_bottom_width())
            + padding.padding_bottom.0.to_length().map_or(0.0, |l| l.px()) as f64,
    )
}

fn outset_rect(rect: Rect, insets: (f64, f64, f64, f64)) -> Rect {
    let (left, top, right, bottom) = insets;
    Rect {
        pos: rect.pos - dvec2(left, top),
        size: dvec2(rect.size.x + left + right, rect.size.y + top + bottom),
    }
}

fn physical_rect_to_rect(rect: PhysicalRect<Au>) -> Rect {
    Rect {
        pos: dvec2(rect.origin.x.to_f32_px() as f64, rect.origin.y.to_f32_px() as f64),
        size: dvec2(rect.size.width.to_f32_px() as f64, rect.size.height.to_f32_px() as f64),
    }
}
