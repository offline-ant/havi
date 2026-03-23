use havi_fragment_semantics::fragment_tree::{BoxFragment, FragmentFlags};
use havi_fragment_semantics::{Fragment, IFrameFragment};
use makepad_browser_scene::{
    MpBlendMode, MpChildDocument, MpClipChain, MpClipKind, MpClipNode, MpDocument, MpDocumentId,
    MpEffectNode, MpEmbed, MpFilter, MpHitTestTag, MpIsolation, MpPerCornerRadius,
    MpPipelineId, MpReferenceFrame, MpResourceStore, MpScene, MpSceneId, MpScrollFrame,
    MpSpatialId, MpSpatialKind, MpSpatialNode,
};
use makepad_widgets::{dvec2, Cx2d, DVec2, Rect};
use style::computed_values::mix_blend_mode::T as ComputedMixBlendMode;
use style::computed_values::overflow_x::T as ComputedOverflow;
use style::values::computed::basic_shape::ClipPath;
use style::values::computed::effects::Filter as ComputedFilter;
use style::values::computed::ClipRectOrAuto;

use crate::background::resolve_border_radii;
use crate::browser_scene_adapter::{paint_run_item_to_primitives, AdapterState};
use crate::layout_stacking_context::StackingContextSection;
use crate::reference_frame::reference_frame_semantics;
use crate::scene::RenderPaintItem;

#[derive(Clone, Copy)]
struct BuildContext {
    spatial_id: MpSpatialId,
    clip_chain_id: makepad_browser_scene::MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
    containing_block_origin: DVec2,
}

#[derive(Default)]
struct DirectBuilderIds {
    next_document_id: u64,
    next_scene_id: u64,
    next_pipeline_id: u64,
}

impl DirectBuilderIds {
    fn alloc_document_id(&mut self) -> MpDocumentId {
        self.next_document_id += 1;
        MpDocumentId(self.next_document_id)
    }

    fn alloc_scene_id(&mut self) -> MpSceneId {
        self.next_scene_id += 1;
        MpSceneId(self.next_scene_id)
    }

    fn alloc_pipeline_id(&mut self) -> MpPipelineId {
        self.next_pipeline_id += 1;
        MpPipelineId(self.next_pipeline_id)
    }
}

pub(crate) fn try_build_browser_document(
    cx: &mut Cx2d,
    fragments: &[Fragment],
    scroll_state: &crate::ScrollState,
    viewport_size: DVec2,
) -> Result<MpDocument, String> {
    build_browser_document(cx, fragments, scroll_state, viewport_size, &mut DirectBuilderIds::default())
}

fn build_browser_document(
    cx: &mut Cx2d,
    fragments: &[Fragment],
    scroll_state: &crate::ScrollState,
    viewport_size: DVec2,
    ids: &mut DirectBuilderIds,
) -> Result<MpDocument, String> {
    let viewport_rect = Rect {
        pos: dvec2(0.0, 0.0),
        size: viewport_size,
    };
    let mut scene = MpScene::new(ids.alloc_scene_id(), viewport_rect);
    let root_spatial_id = scene.root_spatial_id;
    let root_clip_chain_id = scene.root_clip_chain_id;
    let mut state = AdapterState {
        resources: MpResourceStore::default(),
        child_documents: Vec::new(),
    };
    build_fragment_list(
        cx,
        fragments,
        scroll_state,
        &mut scene,
        &mut state,
        ids,
        BuildContext {
            spatial_id: root_spatial_id,
            clip_chain_id: root_clip_chain_id,
            effect_id: None,
            containing_block_origin: dvec2(0.0, 0.0),
        },
    )?;
    Ok(MpDocument {
        id: ids.alloc_document_id(),
        epoch: 0,
        scene,
        resources: state.resources,
        child_documents: state.child_documents,
    })
}

fn build_fragment_list(
    cx: &mut Cx2d,
    fragments: &[Fragment],
    scroll_state: &crate::ScrollState,
    scene: &mut MpScene,
    state: &mut AdapterState,
    ids: &mut DirectBuilderIds,
    build_cx: BuildContext,
) -> Result<(), String> {
    for fragment in fragments {
        build_fragment(cx, fragment, scroll_state, scene, state, ids, build_cx)?;
    }
    Ok(())
}

fn build_fragment(
    cx: &mut Cx2d,
    fragment: &Fragment,
    scroll_state: &crate::ScrollState,
    scene: &mut MpScene,
    state: &mut AdapterState,
    ids: &mut DirectBuilderIds,
    build_cx: BuildContext,
) -> Result<(), String> {
    match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => {
            build_box_fragment(cx, fragment, bf, scroll_state, scene, state, ids, build_cx)
        }
        Fragment::Text(tf) => {
            if tf.base.flags.intersects(FragmentFlags::DO_NOT_PAINT) {
                return Ok(());
            }
            push_fragment_primitives(
                cx,
                scene,
                state,
                &RenderPaintItem {
                    section: StackingContextSection::Foreground,
                    local_origin: build_cx.containing_block_origin,
                    source: fragment,
                },
                owner_node_id_for_fragment(fragment),
                build_cx,
            )
        }
        Fragment::Image(image) => {
            if image.base.flags.intersects(FragmentFlags::DO_NOT_PAINT) {
                return Ok(());
            }
            push_fragment_primitives(
                cx,
                scene,
                state,
                &RenderPaintItem {
                    section: StackingContextSection::Foreground,
                    local_origin: build_cx.containing_block_origin,
                    source: fragment,
                },
                owner_node_id_for_fragment(fragment),
                build_cx,
            )
        }
        Fragment::Positioning(positioning) => build_fragment_list(
            cx,
            &positioning.children,
            scroll_state,
            scene,
            state,
            ids,
            build_cx,
        ),
        Fragment::AbsoluteOrFixedPositioned { resolved } => {
            build_fragment(cx, resolved, scroll_state, scene, state, ids, build_cx)
        }
        Fragment::IFrame(iframe) => build_iframe_fragment(
            cx,
            fragment,
            iframe,
            scroll_state,
            scene,
            state,
            ids,
            build_cx,
        ),
    }
}

fn build_box_fragment(
    cx: &mut Cx2d,
    fragment: &Fragment,
    bf: &BoxFragment,
    scroll_state: &crate::ScrollState,
    scene: &mut MpScene,
    state: &mut AdapterState,
    ids: &mut DirectBuilderIds,
    build_cx: BuildContext,
) -> Result<(), String> {
    if bf.base.flags.intersects(FragmentFlags::DO_NOT_PAINT) {
        return Ok(());
    }

    if has_sticky_frame(bf) {
        return Err("direct browser-scene builder does not lower sticky frames yet".to_string());
    }

    let border_rect = physical_rect_to_rect(bf.border_rect());
    let content_rect = physical_rect_to_rect(bf.content_rect());
    let box_origin_in_parent = build_cx.containing_block_origin + border_rect.pos;

    let mut box_cx = build_cx;
    let mut uses_box_local_basis = false;
    if let Some(semantics) = reference_frame_semantics(bf, box_origin_in_parent) {
        box_cx.spatial_id = scene.push_spatial_node(MpSpatialNode {
            parent: Some(box_cx.spatial_id),
            kind: MpSpatialKind::ReferenceFrame(MpReferenceFrame {
                viewport_rect: Rect {
                    pos: dvec2(0.0, 0.0),
                    size: border_rect.size,
                },
                placement_origin: semantics.placement_origin,
                transform: semantics.transform_matrix,
                perspective: semantics.perspective_matrix,
                transform_style: semantics.transform_style,
                backface_visibility: semantics.backface_visibility,
                flattens_descendants: semantics.flattens_descendants,
            }),
        });
        uses_box_local_basis = true;
    }

    if let Some(rect) = css_clip_rect(bf) {
        box_cx.clip_chain_id = push_clip_chain(
            scene,
            box_cx.clip_chain_id,
            box_cx.spatial_id,
            MpClipKind::Rect {
                rect: map_box_rect_to_spatial_space(
                    rect,
                    build_cx.containing_block_origin,
                    border_rect.pos,
                    uses_box_local_basis,
                ),
            },
        );
    }

    if let Some(effect) = lower_box_effect_node(bf, box_cx.spatial_id, box_cx.clip_chain_id)? {
        box_cx.effect_id = Some(scene.push_effect(effect));
    }

    let item_origin = if uses_box_local_basis {
        dvec2(-border_rect.pos.x, -border_rect.pos.y)
    } else {
        build_cx.containing_block_origin
    };
    push_fragment_primitives(
        cx,
        scene,
        state,
        &RenderPaintItem {
            section: StackingContextSection::OwnBackgroundsAndBorders,
            local_origin: item_origin,
            source: fragment,
        },
        owner_node_id_for_fragment(fragment),
        box_cx,
    )?;

    let mut child_cx = box_cx;
    if needs_overflow_clip(bf) {
        if let Some(rect) = bf.scrollable_overflow.map(physical_rect_to_rect) {
            let radius = resolve_border_radii(&bf.base.style).max();
            let rect = map_box_rect_to_spatial_space(
                rect,
                build_cx.containing_block_origin,
                border_rect.pos,
                uses_box_local_basis,
            );
            child_cx.clip_chain_id = push_clip_chain(
                scene,
                child_cx.clip_chain_id,
                box_cx.spatial_id,
                if radius > 0.0 {
                    MpClipKind::RoundedRect {
                        rect,
                        radius: MpPerCornerRadius::uniform(radius),
                    }
                } else {
                    MpClipKind::Rect { rect }
                },
            );
        }
    }

    if let Some(scroll_offset) = scroll_offset_for_box(bf, scroll_state) {
        child_cx.spatial_id = scene.push_spatial_node(MpSpatialNode {
            parent: Some(box_cx.spatial_id),
            kind: MpSpatialKind::ScrollFrame(MpScrollFrame {
                viewport_rect: Rect {
                    pos: if uses_box_local_basis {
                        dvec2(0.0, 0.0)
                    } else {
                        box_origin_in_parent
                    },
                    size: border_rect.size,
                },
                content_rect: Rect {
                    pos: dvec2(0.0, 0.0),
                    size: border_rect.size,
                },
                scroll_offset,
            }),
        });
        uses_box_local_basis = true;
    }

    child_cx.containing_block_origin = if uses_box_local_basis {
        content_rect.pos - border_rect.pos
    } else {
        build_cx.containing_block_origin + content_rect.pos
    };
    build_fragment_list(cx, &bf.children, scroll_state, scene, state, ids, child_cx)
}

fn build_iframe_fragment(
    cx: &mut Cx2d,
    fragment: &Fragment,
    iframe: &IFrameFragment,
    scroll_state: &crate::ScrollState,
    scene: &mut MpScene,
    state: &mut AdapterState,
    ids: &mut DirectBuilderIds,
    build_cx: BuildContext,
) -> Result<(), String> {
    if iframe.base.flags.intersects(FragmentFlags::DO_NOT_PAINT) {
        return Ok(());
    }

    let content_bounds = fragment_local_bounds(fragment, build_cx.containing_block_origin);
    let border_bounds = outset_rect(content_bounds, box_content_insets(&iframe.base.style));
    let iframe_rect = physical_rect_to_rect(iframe.base.rect);
    push_fragment_primitives(
        cx,
        scene,
        state,
        &RenderPaintItem {
            section: StackingContextSection::Foreground,
            local_origin: border_bounds.pos - iframe_rect.pos,
            source: fragment,
        },
        iframe.base.tag.map(|tag| tag.node.0),
        build_cx,
    )?;

    let child_document = build_browser_document(
        cx,
        iframe.child_fragments.as_ref(),
        scroll_state,
        content_bounds.size,
        ids,
    )?;
    let pipeline_id = ids.alloc_pipeline_id();
    scene.push_embed(MpEmbed {
        scene_id: child_document.scene.id,
        pipeline_id,
        spatial_id: build_cx.spatial_id,
        clip_chain_id: build_cx.clip_chain_id,
        effect_id: build_cx.effect_id,
        bounds: content_bounds,
        hit_test_tag: iframe
            .base
            .tag
            .map(|tag| MpHitTestTag(tag.node.0 as u64)),
    });
    state.child_documents.push(MpChildDocument {
        pipeline_id,
        document: Box::new(child_document),
    });
    Ok(())
}

fn push_fragment_primitives(
    cx: &mut Cx2d,
    scene: &mut MpScene,
    state: &mut AdapterState,
    item: &RenderPaintItem<'_>,
    owner_node_id: Option<usize>,
    build_cx: BuildContext,
) -> Result<(), String> {
    for primitive in paint_run_item_to_primitives(
        cx,
        scene,
        state,
        item,
        owner_node_id,
        build_cx.spatial_id,
        build_cx.clip_chain_id,
        build_cx.effect_id,
    )? {
        scene.push_primitive(primitive);
    }
    Ok(())
}

fn lower_box_effect_node(
    bf: &BoxFragment,
    spatial_id: MpSpatialId,
    clip_chain_id: makepad_browser_scene::MpClipChainId,
) -> Result<Option<MpEffectNode>, String> {
    let effects = bf.base.style.get_effects();
    let svg = bf.base.style.get_svg();
    if svg.clip_path != ClipPath::None {
        return Err("direct browser-scene builder does not lower clip-path masks yet".to_string());
    }
    let opacity = effects.opacity;
    let filters = lower_box_effect_filters(&effects.filter.0)?;
    let blend_mode = if effects.mix_blend_mode == ComputedMixBlendMode::Normal {
        MpBlendMode::Normal
    } else {
        MpBlendMode::Named(format!("{:?}", effects.mix_blend_mode))
    };
    let isolated = opacity != 1.0 || !filters.is_empty() || !matches!(blend_mode, MpBlendMode::Normal);
    if !isolated {
        return Ok(None);
    }
    Ok(Some(MpEffectNode {
        spatial_id,
        clip_chain_id,
        opacity,
        filters,
        blend_mode,
        isolation: MpIsolation::Isolate,
        mask: None,
    }))
}

fn lower_box_effect_filters(filters: &[ComputedFilter]) -> Result<Vec<MpFilter>, String> {
    let mut lowered = Vec::new();
    for filter in filters {
        match filter {
            ComputedFilter::Blur(radius) => lowered.push(MpFilter::Blur(radius.0.px().max(0.0))),
            ComputedFilter::Opacity(opacity) => {
                lowered.push(MpFilter::Opacity(opacity.0.clamp(0.0, 1.0)))
            }
            other => {
                return Err(format!(
                    "direct browser-scene builder does not lower filter yet: {other:?}"
                ))
            }
        }
    }
    Ok(lowered)
}

fn scroll_offset_for_box(
    bf: &BoxFragment,
    scroll_state: &crate::ScrollState,
) -> Option<DVec2> {
    if !needs_overflow_clip(bf) {
        return None;
    }
    bf.scrollable_overflow?;
    Some(
        bf.base
            .tag
            .and_then(|tag| scroll_state.get(&tag.node.0).copied())
            .unwrap_or_else(|| dvec2(0.0, 0.0)),
    )
}

fn has_sticky_frame(bf: &BoxFragment) -> bool {
    bf.base.style.get_box().position == style::computed_values::position::T::Sticky
}

fn needs_overflow_clip(bf: &BoxFragment) -> bool {
    let overflow = bf.base.style.get_box();
    !matches!(overflow.overflow_x, ComputedOverflow::Visible)
        || !matches!(overflow.overflow_y, ComputedOverflow::Visible)
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

fn push_clip_chain(
    scene: &mut MpScene,
    parent: makepad_browser_scene::MpClipChainId,
    spatial_id: MpSpatialId,
    kind: MpClipKind,
) -> makepad_browser_scene::MpClipChainId {
    let clip_id = scene.push_clip(MpClipNode { spatial_id, kind });
    scene.push_clip_chain(MpClipChain {
        parent: Some(parent),
        clips: vec![clip_id],
    })
}

fn map_box_rect_to_spatial_space(
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

fn owner_node_id_for_fragment(fragment: &Fragment) -> Option<usize> {
    match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => owner_node_id_for_box(bf),
        Fragment::Text(tf) => tf.base.tag.map(|tag| tag.node.0),
        Fragment::Image(image) => image.base.tag.map(|tag| tag.node.0),
        Fragment::IFrame(iframe) => iframe.base.tag.map(|tag| tag.node.0),
        Fragment::Positioning(positioning) => positioning.base.tag.map(|tag| tag.node.0),
        Fragment::AbsoluteOrFixedPositioned { .. } => None,
    }
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

fn physical_rect_to_rect(rect: havi_types::PhysicalRect<app_units::Au>) -> Rect {
    Rect {
        pos: dvec2(rect.origin.x.to_f32_px() as f64, rect.origin.y.to_f32_px() as f64),
        size: dvec2(rect.size.width.to_f32_px() as f64, rect.size.height.to_f32_px() as f64),
    }
}
