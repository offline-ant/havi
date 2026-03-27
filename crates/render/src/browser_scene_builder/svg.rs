use euclid::{point2, size2, Transform2D};
use havi_types::fragment_tree as published;
use makepad_browser_scene::{
    MpClipChain, MpClipKind, MpClipNode, MpEffectNode, MpIsolation, MpReferenceFrame, MpScene,
    MpSpatialKind, MpSpatialNode, ResourceRegistry,
};
use makepad_widgets::{dvec2, Cx2d, Mat4f, Rect};
use style_traits::CSSPixel;

use super::geometry::physical_rect_to_rect;
use super::traversal::{build_paint_list, push_fragment_primitives};
use super::{
    BrowserDocumentScrollNodes, BuildContext, BuildState, DirectBuilderIds,
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
    svg_cx.spatial_id = push_svg_reference_frame(
        scene,
        build_cx.spatial_id,
        svg.base.rect,
        build_cx.containing_block_origin,
        Some(svg.local_to_parent_transform),
        false,
    );
    if let Some(overflow_clip) = &svg.overflow_clip {
        let rect = Rect {
            pos: dvec2(overflow_clip.rect.origin.x as f64, overflow_clip.rect.origin.y as f64),
            size: dvec2(overflow_clip.rect.size.width as f64, overflow_clip.rect.size.height as f64),
        };
        svg_cx.clip_chain_id = push_clip_chain(
            scene,
            svg_cx.clip_chain_id,
            svg_cx.spatial_id,
            MpClipKind::Rect { rect },
        );
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
    if !svg_transform_is_identity(svg.local_transform) {
        svg_cx.spatial_id = push_svg_reference_frame(
            scene,
            build_cx.spatial_id,
            svg.base.rect,
            build_cx.containing_block_origin,
            Some(svg.local_transform),
            true,
        );
        svg_cx.containing_block_origin = dvec2(0.0, 0.0);
    }
    if svg.opacity < 0.999 {
        svg_cx.effect_id = Some(scene.push_effect(MpEffectNode {
            spatial_id: svg_cx.spatial_id,
            clip_chain_id: svg_cx.clip_chain_id,
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

pub(super) fn build_svg_path_fragment(
    cx: &mut Cx2d,
    generation: &published::FragmentArenaGeneration,
    fragment_id: published::FragmentId,
    svg: &published::SVGPathFragment,
    scene: &mut MpScene,
    registry: &mut ResourceRegistry,
    state: &mut BuildState,
    build_cx: BuildContext,
) -> Result<(), String> {
    let mut path_cx = build_cx;
    if !svg_transform_is_identity(svg.local_transform) {
        let rect = havi_types::PhysicalRect::new(
            point2(
                app_units::Au::from_f32_px(svg.decorated_bounding_box.origin.x),
                app_units::Au::from_f32_px(svg.decorated_bounding_box.origin.y),
            ),
            size2(
                app_units::Au::from_f32_px(svg.decorated_bounding_box.size.width),
                app_units::Au::from_f32_px(svg.decorated_bounding_box.size.height),
            ),
        );
        path_cx.spatial_id = push_svg_reference_frame(
            scene,
            build_cx.spatial_id,
            rect,
            build_cx.containing_block_origin,
            Some(svg.local_transform),
            true,
        );
        path_cx.containing_block_origin = dvec2(0.0, 0.0);
    }
    push_fragment_primitives(
        cx,
        generation,
        scene,
        registry,
        state,
        &RenderPaintItem {
            section: StackingContextSection::Foreground,
            local_origin: path_cx.containing_block_origin,
            fragment_id,
        },
        owner_node_id_for_fragment(generation, fragment_id),
        path_cx,
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
    let mut foreign_object_cx = build_cx;
    if !svg_transform_is_identity(svg.local_transform) {
        foreign_object_cx.spatial_id = push_svg_reference_frame(
            scene,
            build_cx.spatial_id,
            svg.base.rect,
            build_cx.containing_block_origin,
            Some(svg.local_transform),
            true,
        );
        foreign_object_cx.containing_block_origin = dvec2(0.0, 0.0);
    }
    push_fragment_primitives(
        cx,
        generation,
        scene,
        registry,
        state,
        &RenderPaintItem {
            section: StackingContextSection::Foreground,
            local_origin: foreign_object_cx.containing_block_origin,
            fragment_id,
        },
        owner_node_id_for_fragment(generation, fragment_id),
        foreign_object_cx,
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
        foreign_object_cx,
        scroll_nodes,
        previous_document,
    )
}

fn push_svg_reference_frame(
    scene: &mut MpScene,
    parent: makepad_browser_scene::MpSpatialId,
    rect: havi_types::PhysicalRect<app_units::Au>,
    containing_block_origin: makepad_widgets::DVec2,
    transform: Option<published::SVGTransform>,
    relative_origin: bool,
) -> makepad_browser_scene::MpSpatialId {
    let border_rect = physical_rect_to_rect(rect);
    scene.push_spatial_node(MpSpatialNode {
        parent: Some(parent),
        kind: MpSpatialKind::ReferenceFrame(MpReferenceFrame {
            viewport_rect: Rect {
                pos: dvec2(0.0, 0.0),
                size: border_rect.size,
            },
            placement_origin: if relative_origin {
                containing_block_origin
            } else {
                containing_block_origin + border_rect.pos
            },
            transform: transform.and_then(svg_transform_to_mat4),
            perspective: None,
            transform_style: makepad_browser_scene::MpTransformStyle::Flat,
            backface_visibility: makepad_browser_scene::MpBackfaceVisibility::Visible,
            flattens_descendants: true,
        }),
    })
}

fn svg_transform_to_mat4(
    transform: Transform2D<f32, CSSPixel, CSSPixel>,
) -> Option<Mat4f> {
    if svg_transform_is_identity(transform) {
        return None;
    }
    Some(Mat4f {
        v: [
            transform.m11, transform.m12, 0.0, 0.0,
            transform.m21, transform.m22, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            transform.m31, transform.m32, 0.0, 1.0,
        ],
    })
}

fn svg_transform_is_identity(transform: Transform2D<f32, CSSPixel, CSSPixel>) -> bool {
    transform == Transform2D::identity()
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
