use havi_types::fragment_tree as published;
use makepad_browser_scene::{
    MpClipChain, MpClipKind, MpClipNode, MpEffectNode, MpIsolation, MpReferenceFrame, MpScene,
    MpSpatialKind, MpSpatialNode, ResourceRegistry,
};
use makepad_widgets::{dvec2, Cx2d, Rect};

use super::geometry::physical_rect_to_rect;
use super::traversal::{build_paint_list, push_fragment_primitives};
use super::{
    BuildContext, BuildState, BrowserDocumentScrollNodes, DirectBuilderIds,
};
use crate::browser_scene_builder::traversal::owner_node_id_for_fragment;
use crate::layout_stacking_context::StackingContextSection;
use crate::paint_items::RenderPaintItem;

pub(super) fn build_svg_viewport_fragment(
    cx: &mut Cx2d,
    generation: &published::FragmentArenaGeneration,
    _fragment_id: published::FragmentId,
    svg: &published::SVGViewportFragment,
    scroll_state: &crate::ScrollState,
    scene: &mut MpScene,
    registry: &mut ResourceRegistry,
    state: &mut BuildState,
    ids: &mut DirectBuilderIds,
    build_cx: BuildContext,
    scroll_nodes: &mut BrowserDocumentScrollNodes,
    previous_document: Option<&makepad_browser_scene::MpDocument>,
) -> Result<(), String> {
    let mut svg_cx = build_cx;
    svg_cx.spatial_id = push_svg_reference_frame(scene, build_cx.spatial_id, svg.base.rect, build_cx.containing_block_origin);
    if let Some(overflow_clip) = &svg.overflow_clip {
        let rect = Rect {
            pos: dvec2(overflow_clip.rect.origin.x as f64, overflow_clip.rect.origin.y as f64),
            size: dvec2(overflow_clip.rect.size.width as f64, overflow_clip.rect.size.height as f64),
        };
        svg_cx.clip_chain_id = push_clip_chain(scene, svg_cx.clip_chain_id, svg_cx.spatial_id, MpClipKind::Rect { rect });
    }
    svg_cx.containing_block_origin = dvec2(0.0, 0.0);
    build_paint_list(
        cx,
        generation,
        &svg.paint_children,
        scroll_state,
        scene,
        registry,
        state,
        ids,
        svg_cx,
        scroll_nodes,
        previous_document,
    )
}

pub(super) fn build_svg_group_fragment(
    cx: &mut Cx2d,
    generation: &published::FragmentArenaGeneration,
    _fragment_id: published::FragmentId,
    svg: &published::SVGGroupFragment,
    scroll_state: &crate::ScrollState,
    scene: &mut MpScene,
    registry: &mut ResourceRegistry,
    state: &mut BuildState,
    ids: &mut DirectBuilderIds,
    build_cx: BuildContext,
    scroll_nodes: &mut BrowserDocumentScrollNodes,
    previous_document: Option<&makepad_browser_scene::MpDocument>,
) -> Result<(), String> {
    let mut svg_cx = build_cx;
    if svg.opacity < 0.999 {
        svg_cx.effect_id = Some(scene.push_effect(MpEffectNode {
            spatial_id: build_cx.spatial_id,
            clip_chain_id: build_cx.clip_chain_id,
            opacity: svg.opacity,
            filters: Vec::new(),
            blend_mode: makepad_browser_scene::MpBlendMode::Normal,
            isolation: MpIsolation::Isolate,
            mask: None,
        }));
    }
    build_paint_list(
        cx,
        generation,
        &svg.paint_children,
        scroll_state,
        scene,
        registry,
        state,
        ids,
        svg_cx,
        scroll_nodes,
        previous_document,
    )
}

pub(super) fn build_svg_foreign_object_fragment(
    cx: &mut Cx2d,
    generation: &published::FragmentArenaGeneration,
    fragment_id: published::FragmentId,
    svg: &published::SVGForeignObjectFragment,
    scroll_state: &crate::ScrollState,
    scene: &mut MpScene,
    registry: &mut ResourceRegistry,
    state: &mut BuildState,
    ids: &mut DirectBuilderIds,
    build_cx: BuildContext,
    scroll_nodes: &mut BrowserDocumentScrollNodes,
    previous_document: Option<&makepad_browser_scene::MpDocument>,
) -> Result<(), String> {
    push_fragment_primitives(
        cx,
        generation,
        scene,
        registry,
        state,
        &RenderPaintItem {
            section: StackingContextSection::Foreground,
            local_origin: build_cx.containing_block_origin,
            fragment_id,
        },
        owner_node_id_for_fragment(generation, fragment_id),
        build_cx,
    )?;
    build_paint_list(
        cx,
        generation,
        &svg.paint_children,
        scroll_state,
        scene,
        registry,
        state,
        ids,
        build_cx,
        scroll_nodes,
        previous_document,
    )
}

fn push_svg_reference_frame(
    scene: &mut MpScene,
    parent: makepad_browser_scene::MpSpatialId,
    rect: havi_types::PhysicalRect<app_units::Au>,
    containing_block_origin: makepad_widgets::DVec2,
) -> makepad_browser_scene::MpSpatialId {
    let border_rect = physical_rect_to_rect(rect);
    scene.push_spatial_node(MpSpatialNode {
        parent: Some(parent),
        kind: MpSpatialKind::ReferenceFrame(MpReferenceFrame {
            viewport_rect: Rect {
                pos: dvec2(0.0, 0.0),
                size: border_rect.size,
            },
            placement_origin: containing_block_origin + border_rect.pos,
            transform: None,
            perspective: None,
            transform_style: makepad_browser_scene::MpTransformStyle::Flat,
            backface_visibility: makepad_browser_scene::MpBackfaceVisibility::Visible,
            flattens_descendants: true,
        }),
    })
}

fn push_clip_chain(
    scene: &mut MpScene,
    parent: makepad_browser_scene::MpClipChainId,
    spatial_id: makepad_browser_scene::MpSpatialId,
    kind: MpClipKind,
) -> makepad_browser_scene::MpClipChainId {
    let clip_id = scene.push_clip(MpClipNode { spatial_id, kind });
    scene.push_clip_chain(MpClipChain {
        parent: Some(parent),
        clips: vec![clip_id],
    })
}
