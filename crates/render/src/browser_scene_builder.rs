use havi_fragment_semantics::fragment_tree::{BoxFragment, FragmentFlags};
use havi_fragment_semantics::Fragment;
use makepad_browser_scene::{
    MpDocument, MpDocumentId, MpReferenceFrame, MpResourceStore, MpScene, MpSceneId, MpSpatialId,
    MpSpatialKind, MpSpatialNode,
};
use makepad_widgets::{dvec2, Cx2d, DVec2, Rect};
use style::computed_values::mix_blend_mode::T as ComputedMixBlendMode;
use style::computed_values::overflow_x::T as ComputedOverflow;
use style::values::computed::basic_shape::ClipPath;
use style::values::computed::ClipRectOrAuto;

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

pub(crate) fn try_build_browser_document(
    cx: &mut Cx2d,
    fragments: &[Fragment],
    scroll_state: &crate::ScrollState,
    viewport_size: DVec2,
) -> Result<MpDocument, String> {
    let viewport_rect = Rect {
        pos: dvec2(0.0, 0.0),
        size: viewport_size,
    };
    let mut scene = MpScene::new(MpSceneId(0), viewport_rect);
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
        BuildContext {
            spatial_id: root_spatial_id,
            clip_chain_id: root_clip_chain_id,
            effect_id: None,
            containing_block_origin: dvec2(0.0, 0.0),
        },
    )?;
    Ok(MpDocument {
        id: MpDocumentId(0),
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
    build_cx: BuildContext,
) -> Result<(), String> {
    for fragment in fragments {
        build_fragment(cx, fragment, scroll_state, scene, state, build_cx)?;
    }
    Ok(())
}

fn build_fragment(
    cx: &mut Cx2d,
    fragment: &Fragment,
    scroll_state: &crate::ScrollState,
    scene: &mut MpScene,
    state: &mut AdapterState,
    build_cx: BuildContext,
) -> Result<(), String> {
    match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => {
            build_box_fragment(cx, fragment, bf, scroll_state, scene, state, build_cx)
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
            build_cx,
        ),
        Fragment::AbsoluteOrFixedPositioned { resolved } => {
            build_fragment(cx, resolved, scroll_state, scene, state, build_cx)
        }
        Fragment::IFrame(_) => Err("direct browser-scene builder does not lower iframes yet".to_string()),
    }
}

fn build_box_fragment(
    cx: &mut Cx2d,
    fragment: &Fragment,
    bf: &BoxFragment,
    scroll_state: &crate::ScrollState,
    scene: &mut MpScene,
    state: &mut AdapterState,
    build_cx: BuildContext,
) -> Result<(), String> {
    if bf.base.flags.intersects(FragmentFlags::DO_NOT_PAINT) {
        return Ok(());
    }

    if needs_overflow_clip(bf) {
        return Err("direct browser-scene builder does not lower overflow clips yet".to_string());
    }
    if css_clip_rect(bf).is_some() {
        return Err("direct browser-scene builder does not lower css clip yet".to_string());
    }
    if has_box_effects(bf) {
        return Err("direct browser-scene builder does not lower box effects yet".to_string());
    }
    if has_scroll_state(bf, scroll_state) {
        return Err("direct browser-scene builder does not lower scroll frames yet".to_string());
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

    let child_containing_block_origin = if uses_box_local_basis {
        content_rect.pos - border_rect.pos
    } else {
        build_cx.containing_block_origin + content_rect.pos
    };
    build_fragment_list(
        cx,
        &bf.children,
        scroll_state,
        scene,
        state,
        BuildContext {
            containing_block_origin: child_containing_block_origin,
            ..box_cx
        },
    )
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

fn has_box_effects(bf: &BoxFragment) -> bool {
    let effects = bf.base.style.get_effects();
    let svg = bf.base.style.get_svg();
    effects.opacity != 1.0
        || !effects.filter.0.is_empty()
        || effects.mix_blend_mode != ComputedMixBlendMode::Normal
        || svg.clip_path != ClipPath::None
}

fn has_scroll_state(bf: &BoxFragment, scroll_state: &crate::ScrollState) -> bool {
    let _ = scroll_state;
    bf.scrollable_overflow.is_some() && needs_overflow_clip(bf)
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
        Fragment::Positioning(positioning) => positioning.base.tag.map(|tag| tag.node.0),
        Fragment::AbsoluteOrFixedPositioned { .. } | Fragment::IFrame(_) => None,
    }
}

fn physical_rect_to_rect(rect: havi_types::PhysicalRect<app_units::Au>) -> Rect {
    Rect {
        pos: dvec2(rect.origin.x.to_f32_px() as f64, rect.origin.y.to_f32_px() as f64),
        size: dvec2(rect.size.width.to_f32_px() as f64, rect.size.height.to_f32_px() as f64),
    }
}
