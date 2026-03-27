use euclid::{point2, size2, Transform2D};
use havi_types::fragment_tree as published;
use makepad_browser_scene::{
    MpClipChain, MpClipKind, MpClipNode, MpEffectNode, MpFillRule, MpIsolation, MpMask,
    MpMaskSampleMode, MpReferenceFrame, MpScene, MpSpatialKind, MpSpatialNode,
    MpVectorMaskContent, MpVectorMaskPath, MpVectorPathCommand, ResourceRegistry,
};
use makepad_widgets::{dvec2, vec2, vec3, Cx2d, Mat4f, Rect};
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
    let clip = resolve_svg_clip_resource(
        generation,
        scene,
        svg.resources.clip_path,
        svg.base.rect,
        svg_cx.spatial_id,
        svg_cx.clip_chain_id,
    );
    svg_cx.clip_chain_id = clip.clip_chain_id;
    if svg.opacity < 0.999 || clip.mask.is_some() {
        svg_cx.effect_id = Some(scene.push_effect(MpEffectNode {
            spatial_id: svg_cx.spatial_id,
            clip_chain_id: svg_cx.clip_chain_id,
            opacity: svg.opacity,
            filters: Vec::new(),
            blend_mode: makepad_browser_scene::MpBlendMode::Normal,
            isolation: MpIsolation::Isolate,
            mask: clip.mask,
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
    let clip = resolve_svg_clip_resource(
        generation,
        scene,
        svg.resources.clip_path,
        svg.base.rect,
        path_cx.spatial_id,
        path_cx.clip_chain_id,
    );
    path_cx.clip_chain_id = clip.clip_chain_id;
    if let Some(mask) = clip.mask {
        path_cx.effect_id = Some(scene.push_effect(MpEffectNode {
            spatial_id: path_cx.spatial_id,
            clip_chain_id: path_cx.clip_chain_id,
            opacity: 1.0,
            filters: Vec::new(),
            blend_mode: makepad_browser_scene::MpBlendMode::Normal,
            isolation: MpIsolation::Isolate,
            mask: Some(mask),
        }));
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

struct ResolvedSvgClip {
    clip_chain_id: makepad_browser_scene::MpClipChainId,
    mask: Option<MpMask>,
}

fn resolve_svg_clip_resource(
    generation: &published::FragmentArenaGeneration,
    scene: &mut MpScene,
    resource_id: Option<published::SVGResourceId>,
    reference_rect: havi_types::PhysicalRect<app_units::Au>,
    spatial_id: makepad_browser_scene::MpSpatialId,
    clip_chain_id: makepad_browser_scene::MpClipChainId,
) -> ResolvedSvgClip {
    let Some(resource_id) = resource_id else {
        return ResolvedSvgClip {
            clip_chain_id,
            mask: None,
        };
    };
    let Some(resource) = generation.svg_resource(resource_id) else {
        return ResolvedSvgClip {
            clip_chain_id,
            mask: None,
        };
    };
    let published::SVGResourceKind::ClipPath(clip) = &resource.kind else {
        return ResolvedSvgClip {
            clip_chain_id,
            mask: None,
        };
    };
    if let Some(rect) = svg_clip_fast_rect(clip, reference_rect) {
        return ResolvedSvgClip {
            clip_chain_id: push_clip_chain(scene, clip_chain_id, spatial_id, MpClipKind::Rect { rect }),
            mask: None,
        };
    }
    let content = lower_svg_clip_mask_content(clip, reference_rect);
    if content.paths.is_empty() {
        return ResolvedSvgClip {
            clip_chain_id,
            mask: None,
        };
    }
    ResolvedSvgClip {
        clip_chain_id,
        mask: Some(MpMask::Vector {
            bounds: physical_rect_to_rect(reference_rect),
            mode: MpMaskSampleMode::Alpha,
            content,
        }),
    }
}

fn svg_clip_fast_rect(
    clip: &published::SVGClipPathResource,
    reference_rect: havi_types::PhysicalRect<app_units::Au>,
) -> Option<Rect> {
    if clip.paths.len() != 1 || !svg_transform_preserves_axis_alignment(clip.transform) {
        return None;
    }
    let rect = svg_rect_path_rect(&clip.paths[0])?;
    let rect = match clip.units {
        published::SVGCoordinateUnits::UserSpaceOnUse => rect,
        published::SVGCoordinateUnits::ObjectBoundingBox => rect_in_object_bounding_box(rect, reference_rect),
    };
    Some(transform_svg_rect(clip.transform, rect))
}

fn lower_svg_clip_mask_content(
    clip: &published::SVGClipPathResource,
    reference_rect: havi_types::PhysicalRect<app_units::Au>,
) -> MpVectorMaskContent {
    let units_transform = svg_clip_units_transform(clip.units, reference_rect);
    let clip_transform = svg_transform_to_mat4(clip.transform).unwrap_or(Mat4f::identity());
    let transform = Mat4f::mul(&clip_transform, &units_transform);
    MpVectorMaskContent {
        paths: clip
            .paths
            .iter()
            .map(|path| MpVectorMaskPath {
                fill_rule: match path.fill_rule {
                    published::SVGFillRule::NonZero => MpFillRule::NonZero,
                    published::SVGFillRule::EvenOdd => MpFillRule::EvenOdd,
                },
                commands: path
                    .commands
                    .iter()
                    .map(lower_svg_clip_path_command)
                    .collect(),
                transform,
            })
            .collect(),
    }
}

fn svg_clip_units_transform(
    units: published::SVGCoordinateUnits,
    reference_rect: havi_types::PhysicalRect<app_units::Au>,
) -> Mat4f {
    match units {
        published::SVGCoordinateUnits::UserSpaceOnUse => Mat4f::identity(),
        published::SVGCoordinateUnits::ObjectBoundingBox => Mat4f::nonuniform_scaled_translation(
            vec3(
                reference_rect.size.width.to_f32_px(),
                reference_rect.size.height.to_f32_px(),
                1.0,
            ),
            vec3(
                reference_rect.origin.x.to_f32_px(),
                reference_rect.origin.y.to_f32_px(),
                0.0,
            ),
        ),
    }
}

fn lower_svg_clip_path_command(command: &published::SVGPathCommand) -> MpVectorPathCommand {
    match command {
        published::SVGPathCommand::MoveTo(point) => MpVectorPathCommand::MoveTo(vec2(point.x, point.y)),
        published::SVGPathCommand::LineTo(point) => MpVectorPathCommand::LineTo(vec2(point.x, point.y)),
        published::SVGPathCommand::QuadTo { ctrl, to } => MpVectorPathCommand::QuadTo {
            ctrl: vec2(ctrl.x, ctrl.y),
            to: vec2(to.x, to.y),
        },
        published::SVGPathCommand::CubicTo { ctrl1, ctrl2, to } => MpVectorPathCommand::CubicTo {
            ctrl1: vec2(ctrl1.x, ctrl1.y),
            ctrl2: vec2(ctrl2.x, ctrl2.y),
            to: vec2(to.x, to.y),
        },
        published::SVGPathCommand::Close => MpVectorPathCommand::Close,
    }
}

fn rect_in_object_bounding_box(
    rect: Rect,
    reference_rect: havi_types::PhysicalRect<app_units::Au>,
) -> Rect {
    Rect {
        pos: dvec2(
            reference_rect.origin.x.to_f32_px() as f64
                + rect.pos.x * reference_rect.size.width.to_f32_px() as f64,
            reference_rect.origin.y.to_f32_px() as f64
                + rect.pos.y * reference_rect.size.height.to_f32_px() as f64,
        ),
        size: dvec2(
            rect.size.x * reference_rect.size.width.to_f32_px() as f64,
            rect.size.y * reference_rect.size.height.to_f32_px() as f64,
        ),
    }
}

fn svg_rect_path_rect(path: &published::SVGPathData) -> Option<Rect> {
    let [p0, p1, p2, p3] = svg_rect_path_points(path)?;
    let epsilon = 1e-6;
    let xs = [p0.x, p1.x, p2.x, p3.x];
    let ys = [p0.y, p1.y, p2.y, p3.y];
    let min_x = xs.into_iter().fold(f32::INFINITY, f32::min);
    let max_x = xs.into_iter().fold(f32::NEG_INFINITY, f32::max);
    let min_y = ys.into_iter().fold(f32::INFINITY, f32::min);
    let max_y = ys.into_iter().fold(f32::NEG_INFINITY, f32::max);
    if (max_x - min_x).abs() <= epsilon || (max_y - min_y).abs() <= epsilon {
        return None;
    }
    let corners = [
        (min_x, min_y),
        (max_x, min_y),
        (max_x, max_y),
        (min_x, max_y),
    ];
    let points = [p0, p1, p2, p3];
    if !points.iter().all(|point| {
        corners.iter().any(|corner| {
            (point.x - corner.0).abs() <= epsilon && (point.y - corner.1).abs() <= epsilon
        })
    }) {
        return None;
    }
    if !svg_path_segment_axis_aligned(p0, p1, epsilon)
        || !svg_path_segment_axis_aligned(p1, p2, epsilon)
        || !svg_path_segment_axis_aligned(p2, p3, epsilon)
        || !svg_path_segment_axis_aligned(p3, p0, epsilon)
    {
        return None;
    }
    Some(Rect {
        pos: dvec2(min_x as f64, min_y as f64),
        size: dvec2((max_x - min_x) as f64, (max_y - min_y) as f64),
    })
}

fn svg_rect_path_points(path: &published::SVGPathData) -> Option<[published::SVGPoint; 4]> {
    match path.commands.as_slice() {
        [
            published::SVGPathCommand::MoveTo(p0),
            published::SVGPathCommand::LineTo(p1),
            published::SVGPathCommand::LineTo(p2),
            published::SVGPathCommand::LineTo(p3),
            published::SVGPathCommand::Close,
        ] => Some([*p0, *p1, *p2, *p3]),
        _ => None,
    }
}

fn svg_path_segment_axis_aligned(
    from: published::SVGPoint,
    to: published::SVGPoint,
    epsilon: f32,
) -> bool {
    ((from.x - to.x).abs() <= epsilon) != ((from.y - to.y).abs() <= epsilon)
}

fn svg_transform_preserves_axis_alignment(transform: published::SVGTransform) -> bool {
    let corners = [
        transform_svg_point(transform, 0.0, 0.0),
        transform_svg_point(transform, 1.0, 0.0),
        transform_svg_point(transform, 0.0, 1.0),
    ];
    let epsilon = 1e-6;
    let top_horizontal = (corners[0].1 - corners[1].1).abs() <= epsilon;
    let top_vertical = (corners[0].0 - corners[1].0).abs() <= epsilon;
    let left_horizontal = (corners[0].1 - corners[2].1).abs() <= epsilon;
    let left_vertical = (corners[0].0 - corners[2].0).abs() <= epsilon;
    (top_horizontal || top_vertical)
        && (left_horizontal || left_vertical)
        && top_horizontal != left_horizontal
}

fn transform_svg_point(transform: published::SVGTransform, x: f32, y: f32) -> (f32, f32) {
    (
        transform.m11 * x + transform.m21 * y + transform.m31,
        transform.m12 * x + transform.m22 * y + transform.m32,
    )
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use app_units::Au;
    use havi_types::fragment_tree::{
        FragmentArenaGeneration, FragmentDerivedData, PaintChild, SVGClipPathResource,
        SVGCoordinateUnits, SVGFillRule, SVGPathCommand, SVGPathData, SVGPoint, SVGResourceKind,
        SVGResourceNode, SVGTransform,
    };
    use havi_types::{PhysicalPoint, PhysicalRect, PhysicalSize};
    use makepad_browser_scene::{MpMask, MpScene, MpSceneId};
    use makepad_widgets::{dvec2, Rect};

    use super::*;

    fn au_rect(x: i32, y: i32, width: i32, height: i32) -> PhysicalRect<Au> {
        PhysicalRect::new(
            PhysicalPoint::new(Au::from_px(x), Au::from_px(y)),
            PhysicalSize::new(Au::from_px(width), Au::from_px(height)),
        )
    }

    fn point(x: f32, y: f32) -> SVGPoint {
        SVGPoint::new(x, y)
    }

    fn rect_path() -> SVGPathData {
        SVGPathData {
            fill_rule: SVGFillRule::NonZero,
            commands: vec![
                SVGPathCommand::MoveTo(point(10.0, 20.0)),
                SVGPathCommand::LineTo(point(50.0, 20.0)),
                SVGPathCommand::LineTo(point(50.0, 60.0)),
                SVGPathCommand::LineTo(point(10.0, 60.0)),
                SVGPathCommand::Close,
            ],
        }
    }

    fn star_path() -> SVGPathData {
        SVGPathData {
            fill_rule: SVGFillRule::EvenOdd,
            commands: vec![
                SVGPathCommand::MoveTo(point(50.0, 0.0)),
                SVGPathCommand::LineTo(point(61.0, 35.0)),
                SVGPathCommand::LineTo(point(98.0, 35.0)),
                SVGPathCommand::LineTo(point(68.0, 57.0)),
                SVGPathCommand::LineTo(point(79.0, 91.0)),
                SVGPathCommand::LineTo(point(50.0, 70.0)),
                SVGPathCommand::LineTo(point(21.0, 91.0)),
                SVGPathCommand::LineTo(point(32.0, 57.0)),
                SVGPathCommand::LineTo(point(2.0, 35.0)),
                SVGPathCommand::LineTo(point(39.0, 35.0)),
                SVGPathCommand::Close,
            ],
        }
    }

    fn generation_with_clip(clip: SVGClipPathResource) -> FragmentArenaGeneration {
        FragmentArenaGeneration {
            geometry_roots: Arc::from([]),
            paint_roots: Arc::<[PaintChild]>::from([]),
            nodes: Arc::from([]),
            placements: Arc::from([]),
            derived: FragmentDerivedData {
                containing_blocks: Vec::new(),
                scrollable_overflow: Vec::new(),
                sticky_insets: Vec::new(),
                background_images: Vec::new(),
            },
            node_fragments: HashMap::new(),
            svg_resources: Arc::from([SVGResourceNode {
                kind: SVGResourceKind::ClipPath(clip),
            }]),
            initial_containing_block: au_rect(0, 0, 0, 0),
            scrollable_overflow: au_rect(0, 0, 0, 0),
        }
    }

    #[test]
    fn svg_rect_path_points_detects_exact_rect_paths() {
        let rect = svg_rect_path_points(&rect_path()).expect("rect path should classify");
        assert_eq!(rect[0], point(10.0, 20.0));
        assert_eq!(rect[2], point(50.0, 60.0));
        assert!(svg_rect_path_points(&star_path()).is_none());
    }

    #[test]
    fn svg_transform_axis_alignment_detection_matches_expected_cases() {
        assert!(svg_transform_preserves_axis_alignment(SVGTransform::identity()));
        assert!(svg_transform_preserves_axis_alignment(SVGTransform::new(
            2.0, 0.0, 0.0, 3.0, 5.0, 7.0,
        )));
        assert!(!svg_transform_preserves_axis_alignment(SVGTransform::new(
            1.0, 0.5, 0.0, 1.0, 0.0, 0.0,
        )));
    }

    #[test]
    fn resolve_svg_clip_resource_uses_fast_rect_clip_for_axis_aligned_rect_paths() {
        let generation = generation_with_clip(SVGClipPathResource {
            units: SVGCoordinateUnits::UserSpaceOnUse,
            transform: SVGTransform::identity(),
            paths: vec![rect_path()],
        });
        let mut scene = MpScene::new(
            MpSceneId(1),
            Rect {
                pos: dvec2(0.0, 0.0),
                size: dvec2(200.0, 200.0),
            },
        );

        let root_spatial_id = scene.root_spatial_id;
        let root_clip_chain_id = scene.root_clip_chain_id;
        let resolved = resolve_svg_clip_resource(
            &generation,
            &mut scene,
            Some(havi_types::fragment_tree::SVGResourceId(0)),
            au_rect(0, 0, 100, 100),
            root_spatial_id,
            root_clip_chain_id,
        );

        assert!(resolved.mask.is_none());
        assert_ne!(resolved.clip_chain_id, scene.root_clip_chain_id);
    }

    #[test]
    fn resolve_svg_clip_resource_routes_non_rect_and_multi_path_clips_to_vector_masks() {
        let star_generation = generation_with_clip(SVGClipPathResource {
            units: SVGCoordinateUnits::UserSpaceOnUse,
            transform: SVGTransform::identity(),
            paths: vec![star_path()],
        });
        let mut star_scene = MpScene::new(
            MpSceneId(2),
            Rect {
                pos: dvec2(0.0, 0.0),
                size: dvec2(200.0, 200.0),
            },
        );
        let star_root_spatial_id = star_scene.root_spatial_id;
        let star_root_clip_chain_id = star_scene.root_clip_chain_id;
        let star_resolved = resolve_svg_clip_resource(
            &star_generation,
            &mut star_scene,
            Some(havi_types::fragment_tree::SVGResourceId(0)),
            au_rect(0, 0, 100, 100),
            star_root_spatial_id,
            star_root_clip_chain_id,
        );
        match star_resolved.mask {
            Some(MpMask::Vector { content, .. }) => assert_eq!(content.paths.len(), 1),
            other => panic!("expected vector mask, got {other:?}"),
        }

        let multi_generation = generation_with_clip(SVGClipPathResource {
            units: SVGCoordinateUnits::UserSpaceOnUse,
            transform: SVGTransform::identity(),
            paths: vec![rect_path(), star_path()],
        });
        let mut multi_scene = MpScene::new(
            MpSceneId(3),
            Rect {
                pos: dvec2(0.0, 0.0),
                size: dvec2(200.0, 200.0),
            },
        );
        let multi_root_spatial_id = multi_scene.root_spatial_id;
        let multi_root_clip_chain_id = multi_scene.root_clip_chain_id;
        let multi_resolved = resolve_svg_clip_resource(
            &multi_generation,
            &mut multi_scene,
            Some(havi_types::fragment_tree::SVGResourceId(0)),
            au_rect(0, 0, 100, 100),
            multi_root_spatial_id,
            multi_root_clip_chain_id,
        );
        match multi_resolved.mask {
            Some(MpMask::Vector { content, .. }) => assert_eq!(content.paths.len(), 2),
            other => panic!("expected vector mask, got {other:?}"),
        }
    }

    #[test]
    fn resolve_svg_clip_resource_routes_non_axis_aligned_rect_transform_to_vector_mask() {
        let generation = generation_with_clip(SVGClipPathResource {
            units: SVGCoordinateUnits::UserSpaceOnUse,
            transform: SVGTransform::new(1.0, 0.5, 0.0, 1.0, 0.0, 0.0),
            paths: vec![rect_path()],
        });
        let mut scene = MpScene::new(
            MpSceneId(4),
            Rect {
                pos: dvec2(0.0, 0.0),
                size: dvec2(200.0, 200.0),
            },
        );
        let root_spatial_id = scene.root_spatial_id;
        let root_clip_chain_id = scene.root_clip_chain_id;
        let resolved = resolve_svg_clip_resource(
            &generation,
            &mut scene,
            Some(havi_types::fragment_tree::SVGResourceId(0)),
            au_rect(0, 0, 100, 100),
            root_spatial_id,
            root_clip_chain_id,
        );
        assert!(matches!(resolved.mask, Some(MpMask::Vector { .. })));
    }
}
