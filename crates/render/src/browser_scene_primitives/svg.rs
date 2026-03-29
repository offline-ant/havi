use std::collections::HashMap;
use std::sync::Arc;

use havi_types::fragment_tree as published;
use makepad_browser_scene::{
    MpGradientStop, MpPrimitive, MpPrimitiveId, MpVectorDraw, MpVectorFillRule,
    MpVectorGradientSpreadMethod, MpVectorLineCap, MpVectorLineJoin, MpVectorPaint,
    MpVectorPathCommand, MpVectorPathPrimitive, ResourceRegistry,
};
use makepad_widgets::{Rect, Vec2f, Vec4f, dvec2, vec2, vec4};

use super::text::lower_svg_text_primitives;
use crate::color::inherited_color;

pub(crate) fn lower_svg_leaf_primitives(
    generation: &published::FragmentArenaGeneration,
    registry: &mut ResourceRegistry,
    glyph_runs: &mut HashMap<makepad_browser_scene::MpGlyphRunKey, makepad_browser_scene::MpGlyphRunResource>,
    owner_node_id: Option<usize>,
    local_origin: makepad_widgets::DVec2,
    _bounds: Rect,
    svg: &published::SVGLeafFragment,
    spatial_id: makepad_browser_scene::MpSpatialId,
    clip_chain_id: makepad_browser_scene::MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
) -> Result<Vec<MpPrimitive>, String> {
    match &svg.kind {
        published::SVGLeafKind::Path(path) => Ok(lower_svg_path_leaf_primitives(
            generation,
            svg,
            path,
            spatial_id,
            clip_chain_id,
            effect_id,
            owner_node_id,
        )),
        published::SVGLeafKind::Text(text) => {
            if let Some(color) = svg_text_fast_path_color(svg, text) {
                return lower_svg_text_primitives(
                    registry,
                    glyph_runs,
                    owner_node_id,
                    local_origin,
                    &text.runs,
                    color,
                    spatial_id,
                    clip_chain_id,
                    effect_id,
                );
            }
            Ok(lower_svg_text_outline_primitives(
                generation,
                svg,
                text,
                spatial_id,
                clip_chain_id,
                effect_id,
                owner_node_id,
            ))
        }
        published::SVGLeafKind::Image(_) => Ok(Vec::new()),
    }
}

fn lower_svg_path_leaf_primitives(
    generation: &published::FragmentArenaGeneration,
    svg: &published::SVGLeafFragment,
    path: &published::SVGPathPayload,
    spatial_id: makepad_browser_scene::MpSpatialId,
    clip_chain_id: makepad_browser_scene::MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
    owner_node_id: Option<usize>,
) -> Vec<MpPrimitive> {
    lower_svg_vector_primitives(
        generation,
        svg,
        Arc::<[MpVectorPathCommand]>::from(
            path.path
                .commands
                .iter()
                .map(convert_path_command)
                .collect::<Vec<_>>(),
        ),
        match path.path.fill_rule {
            published::SVGFillRule::NonZero => MpVectorFillRule::NonZero,
            published::SVGFillRule::EvenOdd => MpVectorFillRule::EvenOdd,
        },
        spatial_id,
        clip_chain_id,
        effect_id,
        owner_node_id,
    )
}

fn lower_svg_text_outline_primitives(
    generation: &published::FragmentArenaGeneration,
    svg: &published::SVGLeafFragment,
    text: &published::SVGTextPayload,
    spatial_id: makepad_browser_scene::MpSpatialId,
    clip_chain_id: makepad_browser_scene::MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
    owner_node_id: Option<usize>,
) -> Vec<MpPrimitive> {
    let mut primitives = Vec::new();
    for (run_index, run) in text.runs.iter().enumerate() {
        let Some(commands) = svg_text_run_outline_commands(text, run_index as u32, run) else {
            continue;
        };
        primitives.extend(lower_svg_vector_primitives(
            generation,
            svg,
            Arc::<[MpVectorPathCommand]>::from(commands),
            MpVectorFillRule::NonZero,
            spatial_id,
            clip_chain_id,
            effect_id,
            owner_node_id,
        ));
    }
    primitives
}

fn lower_svg_vector_primitives(
    generation: &published::FragmentArenaGeneration,
    svg: &published::SVGLeafFragment,
    commands: Arc<[MpVectorPathCommand]>,
    fill_rule: MpVectorFillRule,
    spatial_id: makepad_browser_scene::MpSpatialId,
    clip_chain_id: makepad_browser_scene::MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
    owner_node_id: Option<usize>,
) -> Vec<MpPrimitive> {
    let bounds = Rect {
        pos: dvec2(
            svg.bounds.decorated_bounding_box.origin.x as f64,
            svg.bounds.decorated_bounding_box.origin.y as f64,
        ),
        size: dvec2(
            svg.bounds.decorated_bounding_box.size.width as f64,
            svg.bounds.decorated_bounding_box.size.height as f64,
        ),
    };
    let current_color = svg_current_color(svg);
    let mut primitives = Vec::new();
    for op in svg_paint_ops(svg.paint.paint_order) {
        match op {
            SVGPaintOp::Fill => {
                if let Some(fill) = lower_svg_paint(
                    generation,
                    &svg.bounds.object_bounding_box,
                    current_color,
                    &svg.paint.fill,
                    svg.paint.fill_opacity * svg.paint.opacity,
                ) {
                    primitives.push(MpPrimitive {
                        id: MpPrimitiveId(0),
                        spatial_id,
                        clip_chain_id,
                        effect_id,
                        bounds,
                        kind: makepad_browser_scene::MpPrimitiveKind::VectorPath(MpVectorPathPrimitive {
                            commands: commands.clone(),
                            draw: MpVectorDraw::Fill { fill_rule },
                            paint: fill,
                        }),
                        hit_test_tag: owner_node_id
                            .map(|id| makepad_browser_scene::MpHitTestTag(id as u64)),
                    });
                }
            }
            SVGPaintOp::Stroke => {
                let Some(stroke) = &svg.paint.stroke else {
                    continue;
                };
                if let Some(paint) = lower_svg_paint(
                    generation,
                    &svg.bounds.object_bounding_box,
                    current_color,
                    &stroke.paint,
                    stroke.opacity * svg.paint.opacity,
                ) {
                    primitives.push(MpPrimitive {
                        id: MpPrimitiveId(0),
                        spatial_id,
                        clip_chain_id,
                        effect_id,
                        bounds,
                        kind: makepad_browser_scene::MpPrimitiveKind::VectorPath(MpVectorPathPrimitive {
                            commands: commands.clone(),
                            draw: MpVectorDraw::Stroke {
                                width: stroke.width.max(0.0),
                                line_cap: match stroke.line_cap {
                                    published::SVGLineCap::Butt => MpVectorLineCap::Butt,
                                    published::SVGLineCap::Round => MpVectorLineCap::Round,
                                    published::SVGLineCap::Square => MpVectorLineCap::Square,
                                },
                                line_join: match stroke.line_join {
                                    published::SVGLineJoin::Miter => MpVectorLineJoin::Miter,
                                    published::SVGLineJoin::Round => MpVectorLineJoin::Round,
                                    published::SVGLineJoin::Bevel => MpVectorLineJoin::Bevel,
                                },
                                miter_limit: stroke.miter_limit,
                                non_scaling: matches!(
                                    stroke.vector_effect,
                                    published::SVGVectorEffect::NonScalingStroke
                                ),
                            },
                            paint,
                        }),
                        hit_test_tag: owner_node_id
                            .map(|id| makepad_browser_scene::MpHitTestTag(id as u64)),
                    });
                }
            }
        }
    }
    primitives
}

fn svg_text_payload_is_linear(text: &published::SVGTextPayload) -> bool {
    text.runs
        .iter()
        .enumerate()
        .all(|(run_index, run)| svg_text_run_is_linear(text, run_index as u32, run))
}

fn svg_text_run_is_linear(
    text: &published::SVGTextPayload,
    run_index: u32,
    run: &published::SVGGlyphRun,
) -> bool {
    let mut run_addressings = text.addressing.iter().filter(|char| char.run_index == run_index);
    let mut pen_x = run.rect.origin.x.to_f32_px();
    let baseline_y = run.rect.origin.y.to_f32_px() + run.baseline_ascent.to_f32_px();
    let epsilon = 0.01_f32;

    for glyph in &run.glyphs {
        let char_count = glyph.char_count.max(1) as usize;
        let Some(first_char) = run_addressings.next() else {
            return false;
        };
        if first_char.rotation != 0.0 {
            return false;
        }
        let expected_x = pen_x + glyph.x_offset.to_f32_px();
        let expected_y = baseline_y + glyph.y_offset.to_f32_px();
        if (first_char.position.x - expected_x).abs() > epsilon ||
            (first_char.position.y - expected_y).abs() > epsilon
        {
            return false;
        }
        for _ in 1..char_count {
            let Some(char) = run_addressings.next() else {
                return false;
            };
            if char.rotation != 0.0 ||
                (char.position.x - expected_x).abs() > epsilon ||
                (char.position.y - expected_y).abs() > epsilon
            {
                return false;
            }
        }
        pen_x += glyph.advance.to_f32_px();
    }

    let expected_x = pen_x;
    run_addressings.all(|char| {
        char.rotation == 0.0 &&
            (char.position.x - expected_x).abs() <= epsilon &&
            (char.position.y - baseline_y).abs() <= epsilon
    })
}

fn svg_text_run_outline_commands(
    text: &published::SVGTextPayload,
    run_index: u32,
    run: &published::SVGGlyphRun,
) -> Option<Vec<MpVectorPathCommand>> {
    let font_data = run.font_data.as_ref()?;
    let face = ttf_parser::Face::parse(font_data.as_slice(), run.font_index).ok()?;
    let units_per_em = face.units_per_em() as f32;
    if units_per_em <= 0.0 {
        return None;
    }
    let scale = run.font_size_px / units_per_em;
    let glyph_origins = svg_text_run_glyph_origins(text, run_index, run);
    let mut commands = Vec::new();
    for (glyph, (origin_x, origin_y, rotation_degrees)) in run.glyphs.iter().zip(glyph_origins) {
        let glyph_id = match u16::try_from(glyph.glyph_id) {
            Ok(glyph_id) => ttf_parser::GlyphId(glyph_id),
            Err(_) => continue,
        };
        let mut builder = SVGTextOutlineBuilder {
            commands: &mut commands,
            origin_x,
            origin_y,
            rotation_degrees,
            scale,
        };
        let _ = face.outline_glyph(glyph_id, &mut builder);
    }
    (!commands.is_empty()).then_some(commands)
}

fn svg_text_run_glyph_origins(
    text: &published::SVGTextPayload,
    run_index: u32,
    run: &published::SVGGlyphRun,
) -> Vec<(f32, f32, f32)> {
    let mut origins = Vec::with_capacity(run.glyphs.len());
    let mut run_addressings = text.addressing.iter().filter(|char| char.run_index == run_index);
    let mut pen_x = run.rect.origin.x.to_f32_px();
    let baseline_y = run.rect.origin.y.to_f32_px() + run.baseline_ascent.to_f32_px();

    for glyph in &run.glyphs {
        let char_count = glyph.char_count.max(1) as usize;
        let fallback = (
            pen_x + glyph.x_offset.to_f32_px(),
            baseline_y + glyph.y_offset.to_f32_px(),
            0.0,
        );
        let first = run_addressings.next().map(|char| {
            (
                char.position.x,
                char.position.y,
                char.rotation,
            )
        }).unwrap_or(fallback);
        for _ in 1..char_count {
            let _ = run_addressings.next();
        }
        origins.push(first);
        pen_x += glyph.advance.to_f32_px();
    }

    origins
}

struct SVGTextOutlineBuilder<'a> {
    commands: &'a mut Vec<MpVectorPathCommand>,
    origin_x: f32,
    origin_y: f32,
    rotation_degrees: f32,
    scale: f32,
}

impl SVGTextOutlineBuilder<'_> {
    fn point(&self, x: f32, y: f32) -> makepad_widgets::Vec2f {
        let local_x = x * self.scale;
        let local_y = -y * self.scale;
        let angle = self.rotation_degrees.to_radians();
        let sin = angle.sin();
        let cos = angle.cos();
        vec2(
            self.origin_x + local_x * cos - local_y * sin,
            self.origin_y + local_x * sin + local_y * cos,
        )
    }
}

impl ttf_parser::OutlineBuilder for SVGTextOutlineBuilder<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.commands.push(MpVectorPathCommand::MoveTo(self.point(x, y)));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.commands.push(MpVectorPathCommand::LineTo(self.point(x, y)));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.commands.push(MpVectorPathCommand::QuadTo {
            ctrl: self.point(x1, y1),
            to: self.point(x, y),
        });
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.commands.push(MpVectorPathCommand::CubicTo {
            ctrl1: self.point(x1, y1),
            ctrl2: self.point(x2, y2),
            to: self.point(x, y),
        });
    }

    fn close(&mut self) {
        self.commands.push(MpVectorPathCommand::Close);
    }
}

#[derive(Clone, Copy)]
enum SVGPaintOp {
    Fill,
    Stroke,
}

fn svg_paint_ops(order: published::SVGPaintOrder) -> &'static [SVGPaintOp] {
    use SVGPaintOp::{Fill, Stroke};
    match order {
        published::SVGPaintOrder::Normal |
        published::SVGPaintOrder::FillStrokeMarkers |
        published::SVGPaintOrder::FillMarkersStroke |
        published::SVGPaintOrder::MarkersFillStroke => &[Fill, Stroke],
        published::SVGPaintOrder::StrokeFillMarkers |
        published::SVGPaintOrder::StrokeMarkersFill |
        published::SVGPaintOrder::MarkersStrokeFill => &[Stroke, Fill],
    }
}

fn svg_current_color(svg: &published::SVGLeafFragment) -> published::SVGColor {
    let color = inherited_color(&svg.base.style);
    published::SVGColor {
        red: color.x,
        green: color.y,
        blue: color.z,
        alpha: color.w,
    }
}

fn svg_text_fast_path_color(
    svg: &published::SVGLeafFragment,
    text: &published::SVGTextPayload,
) -> Option<Vec4f> {
    if svg.paint.stroke.is_some() || !svg_text_payload_is_linear(text) {
        return None;
    }
    let alpha = svg.paint.fill_opacity * svg.paint.opacity;
    if !alpha.is_finite() || alpha <= 0.0 {
        return None;
    }
    match &svg.paint.fill {
        published::SVGPaint::SolidColor(color) => Some(svg_color(*color, alpha)),
        published::SVGPaint::CurrentColor => Some(svg_color(svg_current_color(svg), alpha)),
        published::SVGPaint::None |
        published::SVGPaint::ContextFill |
        published::SVGPaint::ContextStroke |
        published::SVGPaint::Server(_) => None,
    }
}

fn svg_color(color: published::SVGColor, alpha_scale: f32) -> Vec4f {
    vec4(
        color.red,
        color.green,
        color.blue,
        (color.alpha * alpha_scale).clamp(0.0, 1.0),
    )
}

fn convert_path_command(command: &published::SVGPathCommand) -> MpVectorPathCommand {
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

fn lower_svg_paint(
    generation: &published::FragmentArenaGeneration,
    object_bounding_box: &published::SVGRect,
    current_color: published::SVGColor,
    paint: &published::SVGPaint,
    opacity: f32,
) -> Option<MpVectorPaint> {
    match paint {
        published::SVGPaint::None | published::SVGPaint::ContextFill | published::SVGPaint::ContextStroke => None,
        published::SVGPaint::SolidColor(color) => Some(MpVectorPaint::Solid {
            color: svg_color(*color, opacity),
        }),
        published::SVGPaint::CurrentColor => Some(MpVectorPaint::Solid {
            color: svg_color(current_color, opacity),
        }),
        published::SVGPaint::Server(resource_id) => {
            lower_svg_paint_server(generation, object_bounding_box, *resource_id, opacity)
        }
    }
}

fn lower_svg_paint_server(
    generation: &published::FragmentArenaGeneration,
    object_bounding_box: &published::SVGRect,
    resource_id: published::SVGResourceId,
    opacity: f32,
) -> Option<MpVectorPaint> {
    let resource = generation.svg_resource(resource_id)?;
    let published::SVGResourceKind::PaintServer(server) = &resource.kind else {
        return None;
    };
    match server {
        published::SVGPaintServerResource::Gradient(gradient) => {
            lower_svg_gradient_paint(object_bounding_box, gradient, opacity)
        }
        published::SVGPaintServerResource::Pattern(_) => None,
    }
}

fn lower_svg_gradient_paint(
    object_bounding_box: &published::SVGRect,
    gradient: &published::SVGGradientResource,
    opacity: f32,
) -> Option<MpVectorPaint> {
    let stops = gradient
        .stops
        .iter()
        .map(|stop| MpGradientStop {
            offset: stop.offset,
            color: svg_color(stop.color, stop.opacity * opacity),
        })
        .collect::<Vec<_>>();
    match &gradient.kind {
        published::SVGGradientKind::Linear(linear) => {
            let start = transform_gradient_point(
                gradient.gradient_transform,
                gradient_point(gradient.units, object_bounding_box, linear.start),
            );
            let end = transform_gradient_point(
                gradient.gradient_transform,
                gradient_point(gradient.units, object_bounding_box, linear.end),
            );
            Some(MpVectorPaint::LinearGradient {
                start,
                end,
                spread_method: gradient_spread_method(gradient.spread_method),
                stops,
            })
        }
        published::SVGGradientKind::Radial(radial) => {
            let center = transform_gradient_point(
                gradient.gradient_transform,
                gradient_point(gradient.units, object_bounding_box, radial.center),
            );
            let focal = transform_gradient_point(
                gradient.gradient_transform,
                gradient_point(gradient.units, object_bounding_box, radial.focal),
            );
            let radius = transform_gradient_radius(
                gradient.gradient_transform,
                gradient_radius(gradient.units, object_bounding_box, radial.radius),
            );
            Some(MpVectorPaint::RadialGradient {
                center,
                focal,
                radius,
                focal_radius_ratio: radial_focal_radius_ratio(radial),
                spread_method: gradient_spread_method(gradient.spread_method),
                stops,
            })
        }
    }
}

fn gradient_spread_method(
    spread_method: published::SVGGradientSpreadMethod,
) -> MpVectorGradientSpreadMethod {
    match spread_method {
        published::SVGGradientSpreadMethod::Pad => MpVectorGradientSpreadMethod::Pad,
        published::SVGGradientSpreadMethod::Reflect => MpVectorGradientSpreadMethod::Reflect,
        published::SVGGradientSpreadMethod::Repeat => MpVectorGradientSpreadMethod::Repeat,
    }
}

fn gradient_point(
    units: published::SVGCoordinateUnits,
    object_bounding_box: &published::SVGRect,
    point: published::SVGPoint,
) -> Vec2f {
    match units {
        published::SVGCoordinateUnits::UserSpaceOnUse => vec2(point.x, point.y),
        published::SVGCoordinateUnits::ObjectBoundingBox => vec2(
            object_bounding_box.origin.x + point.x * object_bounding_box.size.width,
            object_bounding_box.origin.y + point.y * object_bounding_box.size.height,
        ),
    }
}

fn gradient_radius(
    units: published::SVGCoordinateUnits,
    object_bounding_box: &published::SVGRect,
    radius: f32,
) -> Vec2f {
    match units {
        published::SVGCoordinateUnits::UserSpaceOnUse => vec2(radius, radius),
        published::SVGCoordinateUnits::ObjectBoundingBox => vec2(
            radius * object_bounding_box.size.width,
            radius * object_bounding_box.size.height,
        ),
    }
}

fn transform_gradient_point(transform: published::SVGTransform, point: Vec2f) -> Vec2f {
    vec2(
        transform.m11 * point.x + transform.m21 * point.y + transform.m31,
        transform.m12 * point.x + transform.m22 * point.y + transform.m32,
    )
}

fn transform_gradient_radius(transform: published::SVGTransform, radius: Vec2f) -> Vec2f {
    vec2(
        radius.x * transform.m11.hypot(transform.m12),
        radius.y * transform.m21.hypot(transform.m22),
    )
}

fn radial_focal_radius_ratio(radial: &published::SVGRadialGradient) -> f32 {
    if radial.radius.abs() <= f32::EPSILON {
        return if radial.focal_radius.abs() <= f32::EPSILON {
            0.0
        } else {
            1.0
        };
    }
    (radial.focal_radius / radial.radius).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use app_units::Au;
    use havi_types::fragment_tree::{
        FragmentArenaGeneration, FragmentDerivedData, PaintChild, SVGColor,
        SVGCoordinateUnits, SVGGradientKind, SVGGradientResource, SVGGradientSpreadMethod,
        SVGGradientStop, SVGLinearGradient, SVGPaint, SVGPaintServerResource, SVGRadialGradient,
        SVGResourceKind, SVGResourceNode, SVGTransform,
    };
    use havi_types::{PhysicalPoint, PhysicalRect, PhysicalSize};

    use super::*;

    fn au_rect(x: i32, y: i32, width: i32, height: i32) -> PhysicalRect<Au> {
        PhysicalRect::new(
            PhysicalPoint::new(Au::from_px(x), Au::from_px(y)),
            PhysicalSize::new(Au::from_px(width), Au::from_px(height)),
        )
    }

    fn generation_with_gradient(gradient: SVGGradientResource) -> FragmentArenaGeneration {
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
                kind: SVGResourceKind::PaintServer(SVGPaintServerResource::Gradient(gradient)),
            }]),
            initial_containing_block: au_rect(0, 0, 0, 0),
            scrollable_overflow: au_rect(0, 0, 0, 0),
        }
    }

    fn object_bounding_box() -> published::SVGRect {
        PhysicalRect::new(PhysicalPoint::new(10.0, 20.0), PhysicalSize::new(200.0, 100.0))
    }

    fn stop(offset: f32) -> SVGGradientStop {
        SVGGradientStop {
            offset,
            color: SVGColor {
                red: offset,
                green: 0.25,
                blue: 0.5,
                alpha: 1.0,
            },
            opacity: 1.0,
        }
    }

    #[test]
    fn lower_svg_gradient_paint_preserves_linear_spread_method() {
        let generation = generation_with_gradient(SVGGradientResource {
            units: SVGCoordinateUnits::UserSpaceOnUse,
            gradient_transform: SVGTransform::identity(),
            spread_method: SVGGradientSpreadMethod::Reflect,
            kind: SVGGradientKind::Linear(SVGLinearGradient {
                start: published::SVGPoint::new(2.0, 3.0),
                end: published::SVGPoint::new(11.0, 13.0),
            }),
            stops: vec![stop(0.0), stop(1.0)],
        });

        let paint = lower_svg_paint(
            &generation,
            &object_bounding_box(),
            SVGColor {
                red: 0.0,
                green: 0.0,
                blue: 0.0,
                alpha: 1.0,
            },
            &SVGPaint::Server(published::SVGResourceId(0)),
            1.0,
        )
        .expect("gradient paint");

        match paint {
            MpVectorPaint::LinearGradient {
                start,
                end,
                spread_method,
                stops,
            } => {
                assert_eq!(start, vec2(2.0, 3.0));
                assert_eq!(end, vec2(11.0, 13.0));
                assert_eq!(spread_method, MpVectorGradientSpreadMethod::Reflect);
                assert_eq!(stops.len(), 2);
            }
            other => panic!("expected linear gradient, got {other:?}"),
        }
    }

    #[test]
    fn lower_svg_gradient_paint_preserves_radial_focal_data() {
        let generation = generation_with_gradient(SVGGradientResource {
            units: SVGCoordinateUnits::ObjectBoundingBox,
            gradient_transform: SVGTransform::new(2.0, 0.0, 0.0, 3.0, 7.0, 11.0),
            spread_method: SVGGradientSpreadMethod::Repeat,
            kind: SVGGradientKind::Radial(SVGRadialGradient {
                center: published::SVGPoint::new(0.5, 0.25),
                focal: published::SVGPoint::new(0.75, 0.5),
                radius: 0.5,
                focal_radius: 0.125,
            }),
            stops: vec![stop(0.25), stop(1.0)],
        });

        let paint = lower_svg_paint(
            &generation,
            &object_bounding_box(),
            SVGColor {
                red: 0.0,
                green: 0.0,
                blue: 0.0,
                alpha: 1.0,
            },
            &SVGPaint::Server(published::SVGResourceId(0)),
            0.5,
        )
        .expect("gradient paint");

        match paint {
            MpVectorPaint::RadialGradient {
                center,
                focal,
                radius,
                focal_radius_ratio,
                spread_method,
                stops,
            } => {
                assert_eq!(center, vec2(227.0, 146.0));
                assert_eq!(focal, vec2(327.0, 221.0));
                assert_eq!(radius, vec2(200.0, 150.0));
                assert_eq!(focal_radius_ratio, 0.25);
                assert_eq!(spread_method, MpVectorGradientSpreadMethod::Repeat);
                assert_eq!(stops.len(), 2);
                assert_eq!(stops[0].color.w, 0.5);
            }
            other => panic!("expected radial gradient, got {other:?}"),
        }
    }
}
