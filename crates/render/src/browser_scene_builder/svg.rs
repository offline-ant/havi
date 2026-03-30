use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use euclid::Transform2D;
use havi_types::fragment_tree as published;
use makepad_browser_scene::{
    MpClipChain, MpClipKind, MpClipNode, MpDocument, MpEffectNode, MpFillRule,
    MpFilter, MpIsolation, MpMask, MpMaskSampleMode, MpPatternTileId, MpPatternTileSource,
    MpPrimitive, MpPrimitiveId, MpPrimitiveKind, MpReferenceFrame, MpScene,
    MpSpatialKind, MpSpatialNode, MpVectorDashPattern, MpVectorDraw, MpVectorMaskContent,
    MpVectorMaskPath, MpVectorPaint, MpVectorPathCommand, MpVectorPathPrimitive,
    MpVectorPatternPaint, MpVectorStrokeStyle, ResourceRegistry,
};
use makepad_widgets::{dvec2, vec2, vec3, Cx2d, DVec2, Mat4f, Rect};
use style_traits::CSSPixel;

use super::geometry::physical_rect_to_rect;
use super::traversal::{build_fragment, build_paint_list, push_fragment_primitives};
use super::{
    log_builder_skip_once, BrowserDocumentScrollNodes, BuildContext, BuildState,
    DirectBuilderIds,
};
use crate::browser_scene_builder::traversal::owner_node_id_for_fragment;
use crate::browser_scene_primitives::svg::{
    convert_path_command, normalize_svg_dash_pattern, resolve_svg_pattern_resource_id,
    svg_leaf_vector_shapes, svg_paint_context, svg_paint_phases, SVGPaintContext,
    SVGPaintPhase,
};
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
    let mut svg_cx = build_cx.clone();
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

pub(super) fn build_svg_container_fragment(
    cx: &mut Cx2d,
    generation: &published::FragmentArenaGeneration,
    _fragment_id: published::FragmentId,
    svg: &published::SVGContainerFragment,
    scroll_state: &crate::ScrollState,
    scene: &mut MpScene,
    registry: &mut ResourceRegistry,
    state: &mut BuildState,
    ids: &mut DirectBuilderIds,
    build_cx: BuildContext,
    scroll_nodes: &mut BrowserDocumentScrollNodes,
    previous_document: Option<&makepad_browser_scene::MpDocument>,
) -> Result<(), String> {
    let mut svg_cx = build_cx.clone();
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
        svg.effects.clip_path,
        svg.base.rect,
        svg_cx.spatial_id,
        svg_cx.clip_chain_id,
    );
    svg_cx.clip_chain_id = clip.clip_chain_id;
    let opacity = svg.base.style.get_effects().opacity;
    let filters = resolve_svg_filter_effects(generation, svg.effects.filter);
    let svg_mask = resolve_svg_mask_resource(
        cx,
        generation,
        svg.effects.mask,
        physical_rect_to_svg_rect(svg.base.rect),
        physical_rect_to_rect(svg.base.rect),
        &SVGPaintContext {
            current_color: published::SVGColor::default(),
            fill: crate::browser_scene_primitives::svg::SVGContextPaint {
                paint: published::SVGPaint::None,
                opacity: 1.0,
            },
            stroke: None,
        },
        registry,
        ids,
        svg_cx.clone(),
    )?;
    let mask = combine_svg_effect_masks(clip.mask, svg_mask);
    if opacity < 0.999 || !filters.is_empty() || mask.is_some() {
        svg_cx.effect_id = Some(scene.push_effect(MpEffectNode {
            spatial_id: svg_cx.spatial_id,
            clip_chain_id: svg_cx.clip_chain_id,
            opacity,
            filters,
            blend_mode: makepad_browser_scene::MpBlendMode::Normal,
            isolation: MpIsolation::Isolate,
            mask,
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

pub(super) fn build_svg_leaf_fragment(
    cx: &mut Cx2d,
    generation: &published::FragmentArenaGeneration,
    fragment_id: published::FragmentId,
    svg: &published::SVGLeafFragment,
    scene: &mut MpScene,
    registry: &mut ResourceRegistry,
    state: &mut BuildState,
    ids: &mut DirectBuilderIds,
    build_cx: BuildContext,
) -> Result<(), String> {
    let mut leaf_cx = build_cx.clone();
    if !svg_transform_is_identity(svg.local_transform) {
        leaf_cx.spatial_id = push_svg_reference_frame(
            scene,
            build_cx.spatial_id,
            svg_rect_to_physical_rect(svg.bounds.visual_bounding_box),
            build_cx.containing_block_origin,
            Some(svg.local_transform),
            true,
        );
        leaf_cx.containing_block_origin = dvec2(0.0, 0.0);
    }
    let clip = resolve_svg_clip_resource(
        generation,
        scene,
        svg.effects.clip_path,
        svg_rect_to_physical_rect(svg.bounds.object_bounding_box),
        leaf_cx.spatial_id,
        leaf_cx.clip_chain_id,
    );
    leaf_cx.clip_chain_id = clip.clip_chain_id;
    let paint_context = build_cx
        .svg_paint_context
        .as_ref()
        .cloned()
        .unwrap_or_else(|| svg_paint_context(svg));
    let filters = resolve_svg_filter_effects(generation, svg.effects.filter);
    let svg_mask = resolve_svg_mask_resource(
        cx,
        generation,
        svg.effects.mask,
        svg.bounds.object_bounding_box,
        svg_visual_bounds(svg),
        &paint_context,
        registry,
        ids,
        leaf_cx.clone(),
    )?;
    let mask = combine_svg_effect_masks(clip.mask, svg_mask);
    if svg.paint.opacity < 0.999 || !filters.is_empty() || mask.is_some() {
        leaf_cx.effect_id = Some(scene.push_effect(MpEffectNode {
            spatial_id: leaf_cx.spatial_id,
            clip_chain_id: leaf_cx.clip_chain_id,
            opacity: svg.paint.opacity,
            filters,
            blend_mode: makepad_browser_scene::MpBlendMode::Normal,
            isolation: MpIsolation::Isolate,
            mask,
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
            local_origin: leaf_cx.containing_block_origin,
            fragment_id,
        },
        owner_node_id_for_fragment(generation, fragment_id),
        leaf_cx.clone(),
    )?;
    emit_svg_pattern_primitives(
        cx,
        generation,
        fragment_id,
        svg,
        scene,
        registry,
        state,
        ids,
        leaf_cx,
    )
}

fn emit_svg_pattern_primitives(
    cx: &mut Cx2d,
    generation: &published::FragmentArenaGeneration,
    fragment_id: published::FragmentId,
    svg: &published::SVGLeafFragment,
    scene: &mut MpScene,
    registry: &mut ResourceRegistry,
    state: &mut BuildState,
    ids: &mut DirectBuilderIds,
    build_cx: BuildContext,
) -> Result<(), String> {
    let shapes = svg_leaf_vector_shapes(svg);
    if shapes.is_empty() {
        return Ok(());
    }
    let paint_context = build_cx
        .svg_paint_context
        .as_ref()
        .cloned()
        .unwrap_or_else(|| svg_paint_context(svg));
    let object_bounding_box = svg.bounds.object_bounding_box;
    for phase in svg_paint_phases(svg.paint.paint_order) {
        match phase {
            SVGPaintPhase::Fill => {
                let Some(pattern_resource_id) = resolve_svg_pattern_resource_id(
                    generation,
                    &svg.paint.fill,
                    build_cx.svg_paint_context.as_ref(),
                ) else {
                    continue;
                };
                let Some((tile_id, uv_from_world)) = build_pattern_tile_source(
                    cx,
                    generation,
                    pattern_resource_id,
                    &object_bounding_box,
                    &paint_context,
                    svg.paint.fill_opacity * svg.paint.opacity,
                    scene,
                    registry,
                    state,
                    build_cx.clone(),
                    fragment_id,
                    0,
                )? else {
                    continue;
                };
                for shape in &shapes {
                    scene.push_primitive(MpPrimitive {
                        id: MpPrimitiveId(0),
                        spatial_id: build_cx.spatial_id,
                        clip_chain_id: build_cx.clip_chain_id,
                        effect_id: build_cx.effect_id,
                        bounds: svg_visual_bounds(svg),
                        kind: MpPrimitiveKind::VectorPath(MpVectorPathPrimitive {
                            commands: shape.commands.clone(),
                            draw: MpVectorDraw::Fill {
                                fill_rule: shape.fill_rule,
                            },
                            paint: MpVectorPaint::Pattern(MpVectorPatternPaint {
                                tile_id,
                                uv_from_world,
                            }),
                        }),
                        hit_test_tag: owner_node_id_for_fragment(generation, fragment_id)
                            .map(|id| makepad_browser_scene::MpHitTestTag(id as u64)),
                    });
                }
            }
            SVGPaintPhase::Stroke => {
                let Some(stroke) = &svg.paint.stroke else {
                    continue;
                };
                let Some(pattern_resource_id) = resolve_svg_pattern_resource_id(
                    generation,
                    &stroke.paint,
                    build_cx.svg_paint_context.as_ref(),
                ) else {
                    continue;
                };
                let Some((tile_id, uv_from_world)) = build_pattern_tile_source(
                    cx,
                    generation,
                    pattern_resource_id,
                    &object_bounding_box,
                    &paint_context,
                    stroke.opacity * svg.paint.opacity,
                    scene,
                    registry,
                    state,
                    build_cx.clone(),
                    fragment_id,
                    1,
                )? else {
                    continue;
                };
                let stroke_style = MpVectorStrokeStyle {
                    width: stroke.width.max(0.0),
                    line_cap: match stroke.line_cap {
                        published::SVGLineCap::Butt => makepad_browser_scene::MpVectorLineCap::Butt,
                        published::SVGLineCap::Round => makepad_browser_scene::MpVectorLineCap::Round,
                        published::SVGLineCap::Square => makepad_browser_scene::MpVectorLineCap::Square,
                    },
                    line_join: match stroke.line_join {
                        published::SVGLineJoin::Miter => makepad_browser_scene::MpVectorLineJoin::Miter,
                        published::SVGLineJoin::Round => makepad_browser_scene::MpVectorLineJoin::Round,
                        published::SVGLineJoin::Bevel => makepad_browser_scene::MpVectorLineJoin::Bevel,
                    },
                    miter_limit: stroke.miter_limit,
                    non_scaling: matches!(
                        stroke.vector_effect,
                        published::SVGVectorEffect::NonScalingStroke
                    ),
                    dash_pattern: normalize_svg_dash_pattern(&stroke.dash_array, stroke.dash_offset)
                        .map(|pattern| MpVectorDashPattern {
                            segments: pattern.segments,
                            offset: pattern.offset,
                        }),
                };
                for shape in &shapes {
                    scene.push_primitive(MpPrimitive {
                        id: MpPrimitiveId(0),
                        spatial_id: build_cx.spatial_id,
                        clip_chain_id: build_cx.clip_chain_id,
                        effect_id: build_cx.effect_id,
                        bounds: svg_visual_bounds(svg),
                        kind: MpPrimitiveKind::VectorPath(MpVectorPathPrimitive {
                            commands: shape.commands.clone(),
                            draw: MpVectorDraw::Stroke(stroke_style.clone()),
                            paint: MpVectorPaint::Pattern(MpVectorPatternPaint {
                                tile_id,
                                uv_from_world,
                            }),
                        }),
                        hit_test_tag: owner_node_id_for_fragment(generation, fragment_id)
                            .map(|id| makepad_browser_scene::MpHitTestTag(id as u64)),
                    });
                }
            }
            SVGPaintPhase::Markers => {
                emit_svg_marker_primitives(
                    cx,
                    generation,
                    fragment_id,
                    svg,
                    scene,
                    registry,
                    state,
                    ids,
                    build_cx.clone(),
                    &paint_context,
                )?;
            }
        }
    }
    Ok(())
}

fn resolve_svg_filter_effects(
    generation: &published::FragmentArenaGeneration,
    resource_id: Option<published::SVGResourceId>,
) -> Vec<MpFilter> {
    let Some(resource_id) = resource_id else {
        return Vec::new();
    };
    let Some(resource) = generation.svg_resource(resource_id) else {
        return Vec::new();
    };
    let published::SVGResourceKind::Filter(_filter) = &resource.kind else {
        return Vec::new();
    };
    log_builder_skip_once("svg filter resources do not lower primitives yet; keeping SVG filter reference as a no-op");
    Vec::new()
}

fn combine_svg_effect_masks(clip_mask: Option<MpMask>, svg_mask: Option<MpMask>) -> Option<MpMask> {
    match (clip_mask, svg_mask) {
        (Some(clip_mask), Some(_)) => {
            log_builder_skip_once(
                "svg mask composition with vector clip-path masks is not lowered yet; using clip-path mask only",
            );
            Some(clip_mask)
        }
        (Some(mask), None) | (None, Some(mask)) => Some(mask),
        (None, None) => None,
    }
}

fn resolve_svg_mask_resource(
    _cx: &mut Cx2d,
    generation: &published::FragmentArenaGeneration,
    resource_id: Option<published::SVGResourceId>,
    object_bounding_box: published::SVGRect,
    task_bounds: Rect,
    _paint_context: &SVGPaintContext,
    _registry: &mut ResourceRegistry,
    _ids: &mut DirectBuilderIds,
    _build_cx: BuildContext,
) -> Result<Option<MpMask>, String> {
    let Some(resource_id) = resource_id else {
        return Ok(None);
    };
    let Some(resource) = generation.svg_resource(resource_id) else {
        return Ok(None);
    };
    let published::SVGResourceKind::Mask(mask) = &resource.kind else {
        return Ok(None);
    };
    if mask.paths.is_empty() {
        return Ok(None);
    }
    let mask_rect = resolve_svg_mask_rect(mask, object_bounding_box);
    if mask_rect.size.width <= 0.0 || mask_rect.size.height <= 0.0 {
        return Ok(None);
    }
    let content = lower_svg_mask_resource_content(mask, object_bounding_box);
    if content.paths.is_empty() {
        return Ok(None);
    }
    Ok(Some(MpMask::Vector {
        bounds: task_bounds,
        mode: MpMaskSampleMode::Alpha,
        content,
    }))
}

fn lower_svg_mask_resource_content(
    mask: &published::SVGMaskResource,
    object_bounding_box: published::SVGRect,
) -> MpVectorMaskContent {
    let transform = affine_to_mat4(svg_mask_content_affine(mask.content_units, object_bounding_box));
    MpVectorMaskContent {
        paths: mask
            .paths
            .iter()
            .map(|path| MpVectorMaskPath {
                fill_rule: match path.fill_rule {
                    published::SVGFillRule::NonZero => MpFillRule::NonZero,
                    published::SVGFillRule::EvenOdd => MpFillRule::EvenOdd,
                },
                commands: path.commands.iter().map(lower_svg_clip_path_command).collect(),
                transform,
            })
            .collect(),
    }
}

fn resolve_svg_mask_rect(
    mask: &published::SVGMaskResource,
    object_bounding_box: published::SVGRect,
) -> published::SVGRect {
    match mask.units {
        published::SVGCoordinateUnits::UserSpaceOnUse => mask.rect,
        published::SVGCoordinateUnits::ObjectBoundingBox => published::SVGRect::new(
            euclid::point2(
                object_bounding_box.origin.x + mask.rect.origin.x * object_bounding_box.size.width,
                object_bounding_box.origin.y + mask.rect.origin.y * object_bounding_box.size.height,
            ),
            euclid::size2(
                mask.rect.size.width * object_bounding_box.size.width,
                mask.rect.size.height * object_bounding_box.size.height,
            ),
        ),
    }
}

fn svg_mask_content_affine(
    units: published::SVGCoordinateUnits,
    object_bounding_box: published::SVGRect,
) -> [f32; 6] {
    match units {
        published::SVGCoordinateUnits::UserSpaceOnUse => identity_affine(),
        published::SVGCoordinateUnits::ObjectBoundingBox => [
            object_bounding_box.size.width,
            0.0,
            0.0,
            object_bounding_box.size.height,
            object_bounding_box.origin.x,
            object_bounding_box.origin.y,
        ],
    }
}

fn emit_svg_marker_primitives(
    cx: &mut Cx2d,
    generation: &published::FragmentArenaGeneration,
    _fragment_id: published::FragmentId,
    svg: &published::SVGLeafFragment,
    scene: &mut MpScene,
    registry: &mut ResourceRegistry,
    state: &mut BuildState,
    ids: &mut DirectBuilderIds,
    build_cx: BuildContext,
    paint_context: &SVGPaintContext,
) -> Result<(), String> {
    let Some(placements) = svg_marker_placements(svg) else {
        return Ok(());
    };
    emit_svg_marker_resource(
        cx,
        generation,
        svg.effects.marker_start,
        &placements.start,
        scene,
        registry,
        state,
        ids,
        build_cx.clone(),
        paint_context,
    )?;
    emit_svg_marker_resource(
        cx,
        generation,
        svg.effects.marker_mid,
        &placements.mid,
        scene,
        registry,
        state,
        ids,
        build_cx.clone(),
        paint_context,
    )?;
    emit_svg_marker_resource(
        cx,
        generation,
        svg.effects.marker_end,
        &placements.end,
        scene,
        registry,
        state,
        ids,
        build_cx,
        paint_context,
    )
}

fn emit_svg_marker_resource(
    _cx: &mut Cx2d,
    generation: &published::FragmentArenaGeneration,
    resource_id: Option<published::SVGResourceId>,
    placements: &[SVGMarkerPlacement],
    scene: &mut MpScene,
    _registry: &mut ResourceRegistry,
    _state: &mut BuildState,
    _ids: &mut DirectBuilderIds,
    build_cx: BuildContext,
    paint_context: &SVGPaintContext,
) -> Result<(), String> {
    if placements.is_empty() {
        return Ok(());
    }
    let Some(resource_id) = resource_id else {
        return Ok(());
    };
    let Some(resource) = generation.svg_resource(resource_id) else {
        return Ok(());
    };
    let published::SVGResourceKind::Marker(marker) = &resource.kind else {
        return Ok(());
    };
    if marker.paths.is_empty() {
        return Ok(());
    }
    for placement in placements {
        let spatial_id = push_svg_marker_reference_frame(
            scene,
            build_cx.spatial_id,
            placement.position,
            if marker.orient_auto { placement.angle_radians } else { 0.0 },
        );
        for marker_path in &marker.paths {
            emit_svg_marker_path_primitives(
                scene,
                spatial_id,
                build_cx.clip_chain_id,
                build_cx.effect_id,
                marker_path,
                paint_context,
            );
        }
    }
    Ok(())
}

fn emit_svg_marker_path_primitives(
    scene: &mut MpScene,
    spatial_id: makepad_browser_scene::MpSpatialId,
    clip_chain_id: makepad_browser_scene::MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
    marker_path: &published::SVGMarkerPathResource,
    paint_context: &SVGPaintContext,
) {
    let bounds = Rect {
        pos: dvec2(
            marker_path.bounds.decorated_bounding_box.origin.x as f64,
            marker_path.bounds.decorated_bounding_box.origin.y as f64,
        ),
        size: dvec2(
            marker_path.bounds.decorated_bounding_box.size.width as f64,
            marker_path.bounds.decorated_bounding_box.size.height as f64,
        ),
    };
    let commands: std::sync::Arc<[MpVectorPathCommand]> = std::sync::Arc::from(
        marker_path
            .path
            .commands
            .iter()
            .map(convert_path_command)
            .collect::<Vec<_>>(),
    );
    let fill_rule = match marker_path.path.fill_rule {
        published::SVGFillRule::NonZero => makepad_browser_scene::MpVectorFillRule::NonZero,
        published::SVGFillRule::EvenOdd => makepad_browser_scene::MpVectorFillRule::EvenOdd,
    };
    for phase in svg_paint_phases(marker_path.paint.paint_order) {
        match phase {
            SVGPaintPhase::Fill => {
                let Some(paint) = lower_svg_marker_paint(
                    &marker_path.paint.fill,
                    paint_context.current_color,
                    marker_path.paint.fill_opacity * marker_path.paint.opacity,
                    Some(paint_context),
                ) else {
                    continue;
                };
                scene.push_primitive(MpPrimitive {
                    id: MpPrimitiveId(0),
                    spatial_id,
                    clip_chain_id,
                    effect_id,
                    bounds,
                    kind: MpPrimitiveKind::VectorPath(MpVectorPathPrimitive {
                        commands: commands.clone(),
                        draw: MpVectorDraw::Fill { fill_rule },
                        paint,
                    }),
                    hit_test_tag: None,
                });
            }
            SVGPaintPhase::Stroke => {
                let Some(stroke) = &marker_path.paint.stroke else {
                    continue;
                };
                let Some(paint) = lower_svg_marker_paint(
                    &stroke.paint,
                    paint_context.current_color,
                    stroke.opacity * marker_path.paint.opacity,
                    Some(paint_context),
                ) else {
                    continue;
                };
                scene.push_primitive(MpPrimitive {
                    id: MpPrimitiveId(0),
                    spatial_id,
                    clip_chain_id,
                    effect_id,
                    bounds,
                    kind: MpPrimitiveKind::VectorPath(MpVectorPathPrimitive {
                        commands: commands.clone(),
                        draw: MpVectorDraw::Stroke(MpVectorStrokeStyle {
                            width: stroke.width.max(0.0),
                            line_cap: match stroke.line_cap {
                                published::SVGLineCap::Butt => makepad_browser_scene::MpVectorLineCap::Butt,
                                published::SVGLineCap::Round => makepad_browser_scene::MpVectorLineCap::Round,
                                published::SVGLineCap::Square => makepad_browser_scene::MpVectorLineCap::Square,
                            },
                            line_join: match stroke.line_join {
                                published::SVGLineJoin::Miter => makepad_browser_scene::MpVectorLineJoin::Miter,
                                published::SVGLineJoin::Round => makepad_browser_scene::MpVectorLineJoin::Round,
                                published::SVGLineJoin::Bevel => makepad_browser_scene::MpVectorLineJoin::Bevel,
                            },
                            miter_limit: stroke.miter_limit,
                            non_scaling: matches!(
                                stroke.vector_effect,
                                published::SVGVectorEffect::NonScalingStroke
                            ),
                            dash_pattern: normalize_svg_dash_pattern(&stroke.dash_array, stroke.dash_offset)
                                .map(|pattern| MpVectorDashPattern {
                                    segments: pattern.segments,
                                    offset: pattern.offset,
                                }),
                        }),
                        paint,
                    }),
                    hit_test_tag: None,
                });
            }
            SVGPaintPhase::Markers => {}
        }
    }
}

fn lower_svg_marker_paint(
    paint: &published::SVGPaint,
    current_color: published::SVGColor,
    opacity: f32,
    context: Option<&SVGPaintContext>,
) -> Option<MpVectorPaint> {
    match paint {
        published::SVGPaint::None => None,
        published::SVGPaint::SolidColor(color) => Some(MpVectorPaint::Solid {
            color: makepad_widgets::vec4(
                color.red,
                color.green,
                color.blue,
                (color.alpha * opacity).clamp(0.0, 1.0),
            ),
        }),
        published::SVGPaint::CurrentColor => Some(MpVectorPaint::Solid {
            color: makepad_widgets::vec4(
                current_color.red,
                current_color.green,
                current_color.blue,
                (current_color.alpha * opacity).clamp(0.0, 1.0),
            ),
        }),
        published::SVGPaint::ContextFill => {
            let context = context?;
            lower_svg_marker_paint(
                &context.fill.paint,
                context.current_color,
                opacity * context.fill.opacity,
                None,
            )
        }
        published::SVGPaint::ContextStroke => {
            let context = context?;
            let stroke = context.stroke.as_ref()?;
            lower_svg_marker_paint(
                &stroke.paint,
                context.current_color,
                opacity * stroke.opacity,
                None,
            )
        }
        published::SVGPaint::Server(_) => None,
    }
}

#[derive(Clone, Copy)]
struct SVGMarkerPlacement {
    position: published::SVGPoint,
    angle_radians: f32,
}

struct SVGMarkerPlacements {
    start: Vec<SVGMarkerPlacement>,
    mid: Vec<SVGMarkerPlacement>,
    end: Vec<SVGMarkerPlacement>,
}

fn svg_marker_placements(svg: &published::SVGLeafFragment) -> Option<SVGMarkerPlacements> {
    let published::SVGLeafKind::Path(path) = &svg.kind else {
        return None;
    };
    marker_placements_for_path(&path.path)
}

fn marker_placements_for_path(path: &published::SVGPathData) -> Option<SVGMarkerPlacements> {
    let mut placements = SVGMarkerPlacements {
        start: Vec::new(),
        mid: Vec::new(),
        end: Vec::new(),
    };
    let mut current = None;
    let mut subpath_start = None;
    let mut segments = Vec::new();

    for command in &path.commands {
        match *command {
            published::SVGPathCommand::MoveTo(point) => {
                append_subpath_marker_placements(&mut placements, &segments);
                segments.clear();
                current = Some(point);
                subpath_start = Some(point);
            }
            published::SVGPathCommand::LineTo(to) => {
                let Some(from) = current else { continue; };
                if let Some(segment) = marker_segment_line(from, to) {
                    segments.push(segment);
                }
                current = Some(to);
            }
            published::SVGPathCommand::QuadTo { ctrl, to } => {
                let Some(from) = current else { continue; };
                if let Some(segment) = marker_segment_quad(from, ctrl, to) {
                    segments.push(segment);
                }
                current = Some(to);
            }
            published::SVGPathCommand::CubicTo { ctrl1, ctrl2, to } => {
                let Some(from) = current else { continue; };
                if let Some(segment) = marker_segment_cubic(from, ctrl1, ctrl2, to) {
                    segments.push(segment);
                }
                current = Some(to);
            }
            published::SVGPathCommand::Close => {
                let (Some(from), Some(to)) = (current, subpath_start) else { continue; };
                if let Some(segment) = marker_segment_line(from, to) {
                    segments.push(segment);
                }
                current = Some(to);
            }
        }
    }

    append_subpath_marker_placements(&mut placements, &segments);
    if placements.start.is_empty() && placements.mid.is_empty() && placements.end.is_empty() {
        return None;
    }
    Some(placements)
}

#[derive(Clone, Copy)]
struct SVGMarkerSegment {
    from: published::SVGPoint,
    to: published::SVGPoint,
    start_tangent: makepad_widgets::Vec2f,
    end_tangent: makepad_widgets::Vec2f,
}

fn marker_segment_line(
    from: published::SVGPoint,
    to: published::SVGPoint,
) -> Option<SVGMarkerSegment> {
    let tangent = vec2(to.x - from.x, to.y - from.y);
    tangent_to_angle(tangent)?;
    Some(SVGMarkerSegment {
        from,
        to,
        start_tangent: tangent,
        end_tangent: tangent,
    })
}

fn marker_segment_quad(
    from: published::SVGPoint,
    ctrl: published::SVGPoint,
    to: published::SVGPoint,
) -> Option<SVGMarkerSegment> {
    let start_tangent = first_non_zero_tangent(&[
        vec2(ctrl.x - from.x, ctrl.y - from.y),
        vec2(to.x - from.x, to.y - from.y),
    ])?;
    let end_tangent = first_non_zero_tangent(&[
        vec2(to.x - ctrl.x, to.y - ctrl.y),
        vec2(to.x - from.x, to.y - from.y),
    ])?;
    Some(SVGMarkerSegment {
        from,
        to,
        start_tangent,
        end_tangent,
    })
}

fn marker_segment_cubic(
    from: published::SVGPoint,
    ctrl1: published::SVGPoint,
    ctrl2: published::SVGPoint,
    to: published::SVGPoint,
) -> Option<SVGMarkerSegment> {
    let start_tangent = first_non_zero_tangent(&[
        vec2(ctrl1.x - from.x, ctrl1.y - from.y),
        vec2(ctrl2.x - from.x, ctrl2.y - from.y),
        vec2(to.x - from.x, to.y - from.y),
    ])?;
    let end_tangent = first_non_zero_tangent(&[
        vec2(to.x - ctrl2.x, to.y - ctrl2.y),
        vec2(to.x - ctrl1.x, to.y - ctrl1.y),
        vec2(to.x - from.x, to.y - from.y),
    ])?;
    Some(SVGMarkerSegment {
        from,
        to,
        start_tangent,
        end_tangent,
    })
}

fn append_subpath_marker_placements(
    placements: &mut SVGMarkerPlacements,
    segments: &[SVGMarkerSegment],
) {
    let Some(first) = segments.first() else {
        return;
    };
    placements.start.push(SVGMarkerPlacement {
        position: first.from,
        angle_radians: tangent_to_angle(first.start_tangent).unwrap_or(0.0),
    });
    for index in 0..segments.len().saturating_sub(1) {
        let incoming = segments[index].end_tangent;
        let outgoing = segments[index + 1].start_tangent;
        placements.mid.push(SVGMarkerPlacement {
            position: segments[index].to,
            angle_radians: tangent_pair_angle(incoming, outgoing)
                .or_else(|| tangent_to_angle(outgoing))
                .or_else(|| tangent_to_angle(incoming))
                .unwrap_or(0.0),
        });
    }
    if let Some(last) = segments.last() {
        placements.end.push(SVGMarkerPlacement {
            position: last.to,
            angle_radians: tangent_to_angle(last.end_tangent).unwrap_or(0.0),
        });
    }
}

fn first_non_zero_tangent(candidates: &[makepad_widgets::Vec2f]) -> Option<makepad_widgets::Vec2f> {
    candidates
        .iter()
        .copied()
        .find(|tangent| tangent_to_angle(*tangent).is_some())
}

fn tangent_pair_angle(
    incoming: makepad_widgets::Vec2f,
    outgoing: makepad_widgets::Vec2f,
) -> Option<f32> {
    let sum = vec2(incoming.x + outgoing.x, incoming.y + outgoing.y);
    tangent_to_angle(sum)
}

fn tangent_to_angle(tangent: makepad_widgets::Vec2f) -> Option<f32> {
    let length_sq = tangent.x * tangent.x + tangent.y * tangent.y;
    (length_sq > f32::EPSILON).then_some(tangent.y.atan2(tangent.x))
}

fn push_svg_marker_reference_frame(
    scene: &mut MpScene,
    parent: makepad_browser_scene::MpSpatialId,
    position: published::SVGPoint,
    angle_radians: f32,
) -> makepad_browser_scene::MpSpatialId {
    let sin = angle_radians.sin();
    let cos = angle_radians.cos();
    scene.push_spatial_node(MpSpatialNode {
        parent: Some(parent),
        kind: MpSpatialKind::ReferenceFrame(MpReferenceFrame {
            viewport_rect: Rect {
                pos: dvec2(0.0, 0.0),
                size: dvec2(0.0, 0.0),
            },
            placement_origin: dvec2(0.0, 0.0),
            transform: Some(affine_to_mat4([
                cos,
                sin,
                -sin,
                cos,
                position.x,
                position.y,
            ])),
            perspective: None,
            transform_style: makepad_browser_scene::MpTransformStyle::Flat,
            backface_visibility: makepad_browser_scene::MpBackfaceVisibility::Visible,
            flattens_descendants: true,
        }),
    })
}

fn build_pattern_tile_source(
    cx: &mut Cx2d,
    generation: &published::FragmentArenaGeneration,
    resource_id: published::SVGResourceId,
    object_bounding_box: &published::SVGRect,
    paint_context: &SVGPaintContext,
    operation_opacity: f32,
    scene: &MpScene,
    registry: &mut ResourceRegistry,
    state: &mut BuildState,
    build_cx: BuildContext,
    fragment_id: published::FragmentId,
    phase_index: u64,
) -> Result<Option<(MpPatternTileId, [f32; 6])>, String> {
    let Some(resource) = generation.svg_resource(resource_id) else {
        return Ok(None);
    };
    let published::SVGResourceKind::PaintServer(published::SVGPaintServerResource::Pattern(pattern)) =
        &resource.kind
    else {
        return Ok(None);
    };
    if pattern.source_fragment_roots.is_empty() {
        return Ok(None);
    }

    let tile_rect = resolve_pattern_tile_rect(
        pattern,
        object_bounding_box,
        current_svg_viewport_rect(
            generation,
            fragment_id,
            build_cx.svg_viewport_rect_override,
        ),
    );
    if tile_rect.size.width <= 0.0 || tile_rect.size.height <= 0.0 {
        return Ok(None);
    }

    let tile_size = dvec2(tile_rect.size.width as f64, tile_rect.size.height as f64);
    let tile_id = pattern_tile_id(fragment_id, resource_id, phase_index);
    let tile_revision = pattern_tile_revision(pattern, object_bounding_box, paint_context, operation_opacity);
    let tile_document = build_pattern_tile_document(
        cx,
        generation,
        pattern,
        object_bounding_box,
        tile_rect,
        paint_context,
        operation_opacity,
        registry,
        tile_size,
        build_cx.pipeline_id,
    )?;
    state.pattern_tiles.insert(
        tile_id,
        MpPatternTileSource {
            document: Box::new(tile_document),
            revision: tile_revision,
            tile_size,
        },
    );

    let uv_from_world = resolve_pattern_uv_from_world(pattern, tile_rect, scene, build_cx.spatial_id);
    Ok(Some((tile_id, uv_from_world)))
}

fn build_pattern_tile_document(
    cx: &mut Cx2d,
    generation: &published::FragmentArenaGeneration,
    pattern: &published::SVGPatternResource,
    object_bounding_box: &published::SVGRect,
    tile_rect: published::SVGRect,
    paint_context: &SVGPaintContext,
    operation_opacity: f32,
    registry: &mut ResourceRegistry,
    tile_size: DVec2,
    pipeline_id: webrender_api::PipelineId,
) -> Result<MpDocument, String> {
    let viewport = Rect {
        pos: dvec2(0.0, 0.0),
        size: tile_size,
    };
    let mut scene = MpScene::new(makepad_browser_scene::MpSceneId(0), viewport);
    let clip_id = scene.push_clip(MpClipNode {
        spatial_id: scene.root_spatial_id,
        kind: MpClipKind::Rect { rect: viewport },
    });
    let clip_chain_id = scene.push_clip_chain(MpClipChain {
        parent: Some(scene.root_clip_chain_id),
        clips: vec![clip_id],
    });
    let content_affine = resolve_pattern_tile_content_transform(pattern, object_bounding_box, tile_rect);
    let spatial_id = if content_affine == identity_affine() {
        scene.root_spatial_id
    } else {
        scene.push_spatial_node(MpSpatialNode {
            parent: Some(scene.root_spatial_id),
            kind: MpSpatialKind::ReferenceFrame(MpReferenceFrame {
                viewport_rect: viewport,
                placement_origin: dvec2(0.0, 0.0),
                transform: Some(affine_to_mat4(content_affine)),
                perspective: None,
                transform_style: makepad_browser_scene::MpTransformStyle::Flat,
                backface_visibility: makepad_browser_scene::MpBackfaceVisibility::Visible,
                flattens_descendants: true,
            }),
        })
    };
    let mut state = BuildState::default();
    let effect_id = if operation_opacity < 0.999 {
        Some(scene.push_effect(MpEffectNode {
            spatial_id,
            clip_chain_id,
            opacity: operation_opacity,
            filters: Vec::new(),
            blend_mode: makepad_browser_scene::MpBlendMode::Normal,
            isolation: MpIsolation::Isolate,
            mask: None,
        }))
    } else {
        None
    };
    let build_cx = BuildContext {
        pipeline_id,
        spatial_id,
        clip_chain_id,
        effect_id,
        containing_block_origin: dvec2(0.0, 0.0),
        svg_paint_context: Some(paint_context.clone()),
        svg_viewport_rect_override: Some(published::SVGRect::new(
            euclid::point2(0.0, 0.0),
            euclid::size2(tile_rect.size.width, tile_rect.size.height),
        )),
    };
    let mut scroll_nodes = BrowserDocumentScrollNodes::default();
    let mut ids = DirectBuilderIds::default();
    for fragment_id in &pattern.source_fragment_roots {
        build_fragment(
            cx,
            generation,
            *fragment_id,
            &crate::ScrollState::default(),
            &mut scene,
            registry,
            &mut state,
            &mut ids,
            build_cx.clone(),
            &mut scroll_nodes,
            None,
        )?;
    }
    Ok(MpDocument {
        id: ids.alloc_document_id(),
        epoch: 0,
        scene,
        glyph_runs: state.glyph_runs,
        child_documents: state.child_documents,
        pattern_tiles: state.pattern_tiles,
    })
}

fn pattern_tile_id(
    fragment_id: published::FragmentId,
    resource_id: published::SVGResourceId,
    phase_index: u64,
) -> MpPatternTileId {
    MpPatternTileId(((fragment_id.0 as u64) << 32) ^ ((resource_id.0 as u64) << 1) ^ phase_index)
}

fn pattern_tile_revision(
    pattern: &published::SVGPatternResource,
    object_bounding_box: &published::SVGRect,
    paint_context: &SVGPaintContext,
    operation_opacity: f32,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    std::mem::discriminant(&pattern.units).hash(&mut hasher);
    std::mem::discriminant(&pattern.content_units).hash(&mut hasher);
    hash_svg_transform(pattern.pattern_transform, &mut hasher);
    hash_svg_pattern_rect(&pattern.rect, &mut hasher);
    if let Some(view_box) = pattern.view_box {
        hash_svg_rect(view_box, &mut hasher);
    }
    std::mem::discriminant(&pattern.preserve_aspect_ratio.align).hash(&mut hasher);
    std::mem::discriminant(&pattern.preserve_aspect_ratio.meet_or_slice).hash(&mut hasher);
    hash_svg_rect(*object_bounding_box, &mut hasher);
    hash_svg_paint_context(paint_context, &mut hasher);
    operation_opacity.to_bits().hash(&mut hasher);
    for root in &pattern.source_fragment_roots {
        root.0.hash(&mut hasher);
    }
    for dependency in &pattern.source_resource_dependencies {
        dependency.0.hash(&mut hasher);
    }
    hasher.finish()
}

fn hash_svg_paint_context(context: &SVGPaintContext, hasher: &mut impl Hasher) {
    hash_svg_color(context.current_color, hasher);
    hash_svg_paint(&context.fill.paint, hasher);
    context.fill.opacity.to_bits().hash(hasher);
    if let Some(stroke) = &context.stroke {
        hash_svg_paint(&stroke.paint, hasher);
        stroke.opacity.to_bits().hash(hasher);
        stroke.width.to_bits().hash(hasher);
    }
}

fn hash_svg_paint(paint: &published::SVGPaint, hasher: &mut impl Hasher) {
    match paint {
        published::SVGPaint::None => 0u8.hash(hasher),
        published::SVGPaint::SolidColor(color) => {
            1u8.hash(hasher);
            hash_svg_color(*color, hasher);
        }
        published::SVGPaint::CurrentColor => 2u8.hash(hasher),
        published::SVGPaint::ContextFill => 3u8.hash(hasher),
        published::SVGPaint::ContextStroke => 4u8.hash(hasher),
        published::SVGPaint::Server(id) => {
            5u8.hash(hasher);
            id.0.hash(hasher);
        }
    }
}

fn hash_svg_color(color: published::SVGColor, hasher: &mut impl Hasher) {
    color.red.to_bits().hash(hasher);
    color.green.to_bits().hash(hasher);
    color.blue.to_bits().hash(hasher);
    color.alpha.to_bits().hash(hasher);
}

fn hash_svg_pattern_rect(rect: &published::SVGPatternRect, hasher: &mut impl Hasher) {
    hash_svg_length(rect.x, hasher);
    hash_svg_length(rect.y, hasher);
    hash_svg_length(rect.width, hasher);
    hash_svg_length(rect.height, hasher);
}

fn hash_svg_length(length: published::SVGLength, hasher: &mut impl Hasher) {
    length.value.to_bits().hash(hasher);
    std::mem::discriminant(&length.unit).hash(hasher);
}

fn hash_svg_rect(rect: published::SVGRect, hasher: &mut impl Hasher) {
    rect.origin.x.to_bits().hash(hasher);
    rect.origin.y.to_bits().hash(hasher);
    rect.size.width.to_bits().hash(hasher);
    rect.size.height.to_bits().hash(hasher);
}

fn hash_svg_transform(transform: published::SVGTransform, hasher: &mut impl Hasher) {
    transform.m11.to_bits().hash(hasher);
    transform.m12.to_bits().hash(hasher);
    transform.m21.to_bits().hash(hasher);
    transform.m22.to_bits().hash(hasher);
    transform.m31.to_bits().hash(hasher);
    transform.m32.to_bits().hash(hasher);
}

fn svg_visual_bounds(svg: &published::SVGLeafFragment) -> Rect {
    Rect {
        pos: dvec2(
            svg.bounds.decorated_bounding_box.origin.x as f64,
            svg.bounds.decorated_bounding_box.origin.y as f64,
        ),
        size: dvec2(
            svg.bounds.decorated_bounding_box.size.width as f64,
            svg.bounds.decorated_bounding_box.size.height as f64,
        ),
    }
}

fn resolve_pattern_tile_rect(
    pattern: &published::SVGPatternResource,
    object_bounding_box: &published::SVGRect,
    viewport_rect: Option<published::SVGRect>,
) -> published::SVGRect {
    let viewport_width = viewport_rect
        .map(|rect| rect.size.width)
        .unwrap_or(object_bounding_box.size.width);
    let viewport_height = viewport_rect
        .map(|rect| rect.size.height)
        .unwrap_or(object_bounding_box.size.height);
    match pattern.units {
        published::SVGCoordinateUnits::UserSpaceOnUse => published::SVGRect::new(
            euclid::point2(
                resolve_pattern_length_user_space(pattern.rect.x, viewport_width),
                resolve_pattern_length_user_space(pattern.rect.y, viewport_height),
            ),
            euclid::size2(
                resolve_pattern_length_user_space(pattern.rect.width, viewport_width),
                resolve_pattern_length_user_space(pattern.rect.height, viewport_height),
            ),
        ),
        published::SVGCoordinateUnits::ObjectBoundingBox => published::SVGRect::new(
            euclid::point2(
                object_bounding_box.origin.x
                    + resolve_pattern_length_object_bbox(pattern.rect.x, object_bounding_box.size.width),
                object_bounding_box.origin.y
                    + resolve_pattern_length_object_bbox(pattern.rect.y, object_bounding_box.size.height),
            ),
            euclid::size2(
                resolve_pattern_length_object_bbox(pattern.rect.width, object_bounding_box.size.width),
                resolve_pattern_length_object_bbox(pattern.rect.height, object_bounding_box.size.height),
            ),
        ),
    }
}

fn current_svg_viewport_rect(
    generation: &published::FragmentArenaGeneration,
    fragment_id: published::FragmentId,
    override_rect: Option<published::SVGRect>,
) -> Option<published::SVGRect> {
    if let Some(override_rect) = override_rect {
        return Some(override_rect);
    }
    let mut current = Some(fragment_id);
    while let Some(id) = current {
        let node = generation.node(id);
        if let published::FragmentKind::SVGViewport(svg) = &node.kind {
            return Some(svg.viewport_rect);
        }
        current = node.parent;
    }
    None
}

fn resolve_pattern_tile_content_transform(
    pattern: &published::SVGPatternResource,
    object_bounding_box: &published::SVGRect,
    tile_rect: published::SVGRect,
) -> [f32; 6] {
    if let Some(view_box) = pattern.view_box {
        let mapper = pattern_view_box_mapper(
            published::SVGRect::new(
                euclid::point2(0.0, 0.0),
                euclid::size2(tile_rect.size.width, tile_rect.size.height),
            ),
            view_box,
            pattern.preserve_aspect_ratio,
        );
        return [
            mapper.m11,
            mapper.m12,
            mapper.m21,
            mapper.m22,
            mapper.m31,
            mapper.m32,
        ];
    }

    match pattern.content_units {
        published::SVGCoordinateUnits::UserSpaceOnUse => {
            [1.0, 0.0, 0.0, 1.0, -tile_rect.origin.x, -tile_rect.origin.y]
        }
        published::SVGCoordinateUnits::ObjectBoundingBox => [
            object_bounding_box.size.width,
            0.0,
            0.0,
            object_bounding_box.size.height,
            object_bounding_box.origin.x - tile_rect.origin.x,
            object_bounding_box.origin.y - tile_rect.origin.y,
        ],
    }
}

fn resolve_pattern_uv_from_world(
    pattern: &published::SVGPatternResource,
    tile_rect: published::SVGRect,
    scene: &MpScene,
    spatial_id: makepad_browser_scene::MpSpatialId,
) -> [f32; 6] {
    let tile_width = tile_rect.size.width.max(f32::EPSILON);
    let tile_height = tile_rect.size.height.max(f32::EPSILON);
    let pattern_transform = [
        pattern.pattern_transform.m11,
        pattern.pattern_transform.m12,
        pattern.pattern_transform.m21,
        pattern.pattern_transform.m22,
        pattern.pattern_transform.m31,
        pattern.pattern_transform.m32,
    ];
    let local_to_scene = spatial_affine(scene, spatial_id);
    let scene_to_local = invert_affine(local_to_scene).unwrap_or(identity_affine());
    let scene_to_pattern = multiply_affine(
        invert_affine(pattern_transform).unwrap_or(identity_affine()),
        scene_to_local,
    );
    let pattern_to_uv = [
        1.0 / tile_width,
        0.0,
        0.0,
        1.0 / tile_height,
        -tile_rect.origin.x / tile_width,
        -tile_rect.origin.y / tile_height,
    ];
    multiply_affine(pattern_to_uv, scene_to_pattern)
}

fn resolve_pattern_length_user_space(length: published::SVGLength, reference: f32) -> f32 {
    match length.unit {
        published::SVGLengthUnit::Percent => length.value * reference / 100.0,
        published::SVGLengthUnit::Px | published::SVGLengthUnit::Number => length.value,
        published::SVGLengthUnit::In => length.value * 96.0,
        published::SVGLengthUnit::Cm => length.value * (96.0 / 2.54),
        published::SVGLengthUnit::Mm => length.value * (96.0 / 25.4),
        published::SVGLengthUnit::Pt => length.value * (96.0 / 72.0),
        published::SVGLengthUnit::Pc => length.value * 16.0,
    }
}

fn resolve_pattern_length_object_bbox(length: published::SVGLength, reference: f32) -> f32 {
    match length.unit {
        published::SVGLengthUnit::Percent => length.value * reference / 100.0,
        published::SVGLengthUnit::Number => length.value * reference,
        _ => resolve_pattern_length_user_space(length, reference),
    }
}

fn pattern_view_box_mapper(
    viewport_rect: published::SVGRect,
    view_box_rect: published::SVGRect,
    preserve_aspect_ratio: published::SVGPreserveAspectRatio,
) -> published::SVGTransform {
    let viewport_width = viewport_rect.size.width.max(0.0);
    let viewport_height = viewport_rect.size.height.max(0.0);
    let view_box_width = view_box_rect.size.width;
    let view_box_height = view_box_rect.size.height;
    if viewport_width <= 0.0 || viewport_height <= 0.0 || view_box_width <= 0.0 || view_box_height <= 0.0 {
        return published::SVGTransform::identity();
    }

    let (scale_x, scale_y, extra_x, extra_y) = if matches!(
        preserve_aspect_ratio.align,
        published::SVGPreserveAspectRatioAlign::None
    ) {
        (
            viewport_width / view_box_width,
            viewport_height / view_box_height,
            0.0,
            0.0,
        )
    } else {
        let uniform_scale = if matches!(preserve_aspect_ratio.meet_or_slice, published::SVGMeetOrSlice::Slice) {
            (viewport_width / view_box_width).max(viewport_height / view_box_height)
        } else {
            (viewport_width / view_box_width).min(viewport_height / view_box_height)
        };
        let fitted_width = view_box_width * uniform_scale;
        let fitted_height = view_box_height * uniform_scale;
        (
            uniform_scale,
            uniform_scale,
            viewport_width - fitted_width,
            viewport_height - fitted_height,
        )
    };
    let (align_x, align_y) = pattern_alignment_factors(preserve_aspect_ratio.align);
    let tx = viewport_rect.origin.x + extra_x * align_x - view_box_rect.origin.x * scale_x;
    let ty = viewport_rect.origin.y + extra_y * align_y - view_box_rect.origin.y * scale_y;
    published::SVGTransform::new(scale_x, 0.0, 0.0, scale_y, tx, ty)
}

fn pattern_alignment_factors(align: published::SVGPreserveAspectRatioAlign) -> (f32, f32) {
    match align {
        published::SVGPreserveAspectRatioAlign::None | published::SVGPreserveAspectRatioAlign::XMinYMin => (0.0, 0.0),
        published::SVGPreserveAspectRatioAlign::XMidYMin => (0.5, 0.0),
        published::SVGPreserveAspectRatioAlign::XMaxYMin => (1.0, 0.0),
        published::SVGPreserveAspectRatioAlign::XMinYMid => (0.0, 0.5),
        published::SVGPreserveAspectRatioAlign::XMidYMid => (0.5, 0.5),
        published::SVGPreserveAspectRatioAlign::XMaxYMid => (1.0, 0.5),
        published::SVGPreserveAspectRatioAlign::XMinYMax => (0.0, 1.0),
        published::SVGPreserveAspectRatioAlign::XMidYMax => (0.5, 1.0),
        published::SVGPreserveAspectRatioAlign::XMaxYMax => (1.0, 1.0),
    }
}

fn spatial_affine(scene: &MpScene, spatial_id: makepad_browser_scene::MpSpatialId) -> [f32; 6] {
    let transform = scene.resolve_spatial_transform(spatial_id);
    [transform.v[0], transform.v[1], transform.v[4], transform.v[5], transform.v[12], transform.v[13]]
}

fn affine_to_mat4(affine: [f32; 6]) -> Mat4f {
    Mat4f {
        v: [
            affine[0], affine[1], 0.0, 0.0,
            affine[2], affine[3], 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            affine[4], affine[5], 0.0, 1.0,
        ],
    }
}

fn identity_affine() -> [f32; 6] {
    [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
}

fn multiply_affine(a: [f32; 6], b: [f32; 6]) -> [f32; 6] {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

fn invert_affine(affine: [f32; 6]) -> Option<[f32; 6]> {
    let det = affine[0] * affine[3] - affine[1] * affine[2];
    if det.abs() <= f32::EPSILON {
        return None;
    }
    let inv_det = 1.0 / det;
    let a = affine[3] * inv_det;
    let b = -affine[1] * inv_det;
    let c = -affine[2] * inv_det;
    let d = affine[0] * inv_det;
    let e = -(a * affine[4] + c * affine[5]);
    let f = -(b * affine[4] + d * affine[5]);
    Some([a, b, c, d, e, f])
}

fn svg_rect_to_physical_rect(rect: published::SVGRect) -> havi_types::PhysicalRect<app_units::Au> {
    havi_types::PhysicalRect::new(
        havi_types::PhysicalPoint::new(
            app_units::Au::from_f32_px(rect.origin.x),
            app_units::Au::from_f32_px(rect.origin.y),
        ),
        havi_types::PhysicalSize::new(
            app_units::Au::from_f32_px(rect.size.width.max(0.0)),
            app_units::Au::from_f32_px(rect.size.height.max(0.0)),
        ),
    )
}

fn physical_rect_to_svg_rect(
    rect: havi_types::PhysicalRect<app_units::Au>,
) -> published::SVGRect {
    published::SVGRect::new(
        euclid::point2(rect.origin.x.to_f32_px(), rect.origin.y.to_f32_px()),
        euclid::size2(rect.size.width.to_f32_px(), rect.size.height.to_f32_px()),
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
