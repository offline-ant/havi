use havi_fragment_semantics::fragment_tree::{BoxFragment, FragmentFlags};
use havi_fragment_semantics::Fragment;
use makepad_browser_scene::{MpDocument, MpDocumentId, MpResourceStore, MpScene, MpSceneId};
use makepad_widgets::{dvec2, Cx2d, DVec2, Rect};
use style::computed_values::mix_blend_mode::T as ComputedMixBlendMode;
use style::computed_values::overflow_x::T as ComputedOverflow;
use style::values::computed::basic_shape::ClipPath;
use style::values::computed::ClipRectOrAuto;

use crate::browser_scene_adapter::{paint_run_item_to_primitives, AdapterState};
use crate::layout_stacking_context::StackingContextSection;
use crate::reference_frame::reference_frame_semantics;
use crate::scene::RenderPaintItem;

pub(crate) fn try_build_browser_document(
    cx: &mut Cx2d,
    fragments: &[Fragment],
    scroll_state: &crate::ScrollState,
    viewport_size: DVec2,
) -> Result<MpDocument, String> {
    let mut scene = MpScene::new(
        MpSceneId(0),
        Rect {
            pos: dvec2(0.0, 0.0),
            size: viewport_size,
        },
    );
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
        dvec2(0.0, 0.0),
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
    containing_block_origin: DVec2,
) -> Result<(), String> {
    for fragment in fragments {
        build_fragment(
            cx,
            fragment,
            scroll_state,
            scene,
            state,
            containing_block_origin,
        )?;
    }
    Ok(())
}

fn build_fragment(
    cx: &mut Cx2d,
    fragment: &Fragment,
    scroll_state: &crate::ScrollState,
    scene: &mut MpScene,
    state: &mut AdapterState,
    containing_block_origin: DVec2,
) -> Result<(), String> {
    match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => {
            if bf.base.flags.intersects(FragmentFlags::DO_NOT_PAINT) {
                return Ok(());
            }
            ensure_direct_box_supported(fragment, bf, scroll_state, containing_block_origin)?;
            let item = RenderPaintItem {
                section: StackingContextSection::OwnBackgroundsAndBorders,
                local_origin: containing_block_origin,
                source: fragment,
            };
            for primitive in paint_run_item_to_primitives(
                cx,
                scene,
                state,
                &item,
                owner_node_id_for_fragment(fragment),
                scene.root_spatial_id,
                scene.root_clip_chain_id,
                None,
            )? {
                scene.push_primitive(primitive);
            }
            let content_rect = physical_rect_to_rect(bf.content_rect());
            build_fragment_list(
                cx,
                &bf.children,
                scroll_state,
                scene,
                state,
                containing_block_origin + content_rect.pos,
            )
        }
        Fragment::Text(tf) => {
            if tf.base.flags.intersects(FragmentFlags::DO_NOT_PAINT) {
                return Ok(());
            }
            let item = RenderPaintItem {
                section: StackingContextSection::Foreground,
                local_origin: containing_block_origin,
                source: fragment,
            };
            for primitive in paint_run_item_to_primitives(
                cx,
                scene,
                state,
                &item,
                owner_node_id_for_fragment(fragment),
                scene.root_spatial_id,
                scene.root_clip_chain_id,
                None,
            )? {
                scene.push_primitive(primitive);
            }
            Ok(())
        }
        Fragment::Image(image) => {
            if image.base.flags.intersects(FragmentFlags::DO_NOT_PAINT) {
                return Ok(());
            }
            let item = RenderPaintItem {
                section: StackingContextSection::Foreground,
                local_origin: containing_block_origin,
                source: fragment,
            };
            for primitive in paint_run_item_to_primitives(
                cx,
                scene,
                state,
                &item,
                owner_node_id_for_fragment(fragment),
                scene.root_spatial_id,
                scene.root_clip_chain_id,
                None,
            )? {
                scene.push_primitive(primitive);
            }
            Ok(())
        }
        Fragment::Positioning(positioning) => build_fragment_list(
            cx,
            &positioning.children,
            scroll_state,
            scene,
            state,
            containing_block_origin,
        ),
        Fragment::AbsoluteOrFixedPositioned { resolved } => build_fragment(
            cx,
            resolved,
            scroll_state,
            scene,
            state,
            containing_block_origin,
        ),
        Fragment::IFrame(_) => Err("direct browser-scene builder does not lower iframes yet".to_string()),
    }
}

fn ensure_direct_box_supported(
    fragment: &Fragment,
    bf: &BoxFragment,
    scroll_state: &crate::ScrollState,
    containing_block_origin: DVec2,
) -> Result<(), String> {
    let border_rect = physical_rect_to_rect(bf.border_rect());
    let box_origin_in_parent = containing_block_origin + border_rect.pos;
    if reference_frame_semantics(bf, box_origin_in_parent).is_some() {
        return Err("direct browser-scene builder does not lower transformed boxes yet".to_string());
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
    if matches!(fragment, Fragment::IFrame(_)) {
        return Err("direct browser-scene builder does not lower iframes yet".to_string());
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

fn owner_node_id_for_fragment(fragment: &Fragment) -> Option<usize> {
    match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => bf.base.tag.map(|tag| tag.node.0),
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
