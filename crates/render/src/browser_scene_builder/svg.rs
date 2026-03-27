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
    svg_cx.clip_chain_id = push_svg_clip_resource(
        generation,
        scene,
        svg.resources.clip_path,
        svg.base.rect,
        svg_cx.spatial_id,
        svg_cx.clip_chain_id,
    );
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
    path_cx.clip_chain_id = push_svg_clip_resource(
        generation,
        scene,
        svg.resources.clip_path,
        svg.base.rect,
        path_cx.spatial_id,
        path_cx.clip_chain_id,
    );
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

fn push_svg_clip_resource(
    generation: &published::FragmentArenaGeneration,
    scene: &mut MpScene,
    resource_id: Option<published::SVGResourceId>,
    reference_rect: havi_types::PhysicalRect<app_units::Au>,
    spatial_id: makepad_browser_scene::MpSpatialId,
    clip_chain_id: makepad_browser_scene::MpClipChainId,
) -> makepad_browser_scene::MpClipChainId {
    let Some(resource_id) = resource_id else {
        return clip_chain_id;
    };
    let Some(resource) = generation.svg_resource(resource_id) else {
        return clip_chain_id;
    };
    let published::SVGResourceKind::ClipPath(clip) = &resource.kind else {
        return clip_chain_id;
    };
    let Some(mut rect) = svg_clip_rect(clip, reference_rect) else {
        return clip_chain_id;
    };
    rect = transform_svg_rect(clip.transform, rect);
    push_clip_chain(scene, clip_chain_id, spatial_id, MpClipKind::Rect { rect })
}

fn svg_clip_rect(
    clip: &published::SVGClipPathResource,
    reference_rect: havi_types::PhysicalRect<app_units::Au>,
) -> Option<Rect> {
    let bounds = svg_path_bounds(&clip.paths)?;
    let rect = match clip.units {
        published::SVGCoordinateUnits::UserSpaceOnUse => bounds,
        published::SVGCoordinateUnits::ObjectBoundingBox => Rect {
            pos: dvec2(
                reference_rect.origin.x.to_f32_px() as f64 +
                    bounds.pos.x * reference_rect.size.width.to_f32_px() as f64,
                reference_rect.origin.y.to_f32_px() as f64 +
                    bounds.pos.y * reference_rect.size.height.to_f32_px() as f64,
            ),
            size: dvec2(
                bounds.size.x * reference_rect.size.width.to_f32_px() as f64,
                bounds.size.y * reference_rect.size.height.to_f32_px() as f64,
            ),
        },
    };
    Some(rect)
}

fn svg_path_bounds(paths: &[published::SVGPathData]) -> Option<Rect> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut saw_point = false;

    let mut update = |point: published::SVGPoint| {
        saw_point = true;
        min_x = min_x.min(point.x as f64);
        min_y = min_y.min(point.y as f64);
        max_x = max_x.max(point.x as f64);
        max_y = max_y.max(point.y as f64);
    };

    for path in paths {
        for command in &path.commands {
            match command {
                published::SVGPathCommand::MoveTo(point) | published::SVGPathCommand::LineTo(point) => update(*point),
                published::SVGPathCommand::QuadTo { ctrl, to } => {
                    update(*ctrl);
                    update(*to);
                }
                published::SVGPathCommand::CubicTo { ctrl1, ctrl2, to } => {
                    update(*ctrl1);
                    update(*ctrl2);
                    update(*to);
                }
                published::SVGPathCommand::Close => {}
            }
        }
    }

    saw_point.then(|| Rect {
        pos: dvec2(min_x, min_y),
        size: dvec2(max_x - min_x, max_y - min_y),
    })
}

fn transform_svg_rect(transform: published::SVGTransform, rect: Rect) -> Rect {
    let corners = [
        dvec2(rect.pos.x, rect.pos.y),
        dvec2(rect.pos.x + rect.size.x, rect.pos.y),
        dvec2(rect.pos.x, rect.pos.y + rect.size.y),
        dvec2(rect.pos.x + rect.size.x, rect.pos.y + rect.size.y),
    ];
    let transformed = corners.map(|point| {
        dvec2(
            transform.m11 as f64 * point.x + transform.m21 as f64 * point.y + transform.m31 as f64,
            transform.m12 as f64 * point.x + transform.m22 as f64 * point.y + transform.m32 as f64,
        )
    });
    let min_x = transformed.iter().map(|point| point.x).fold(f64::INFINITY, f64::min);
    let min_y = transformed.iter().map(|point| point.y).fold(f64::INFINITY, f64::min);
    let max_x = transformed.iter().map(|point| point.x).fold(f64::NEG_INFINITY, f64::max);
    let max_y = transformed.iter().map(|point| point.y).fold(f64::NEG_INFINITY, f64::max);
    Rect {
        pos: dvec2(min_x, min_y),
        size: dvec2(max_x - min_x, max_y - min_y),
    }
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
