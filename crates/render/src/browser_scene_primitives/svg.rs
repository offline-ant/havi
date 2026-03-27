use std::sync::Arc;

use havi_types::fragment_tree as published;
use makepad_browser_scene::{
    MpGradientStop, MpPrimitive, MpPrimitiveId, MpVectorDraw, MpVectorFillRule,
    MpVectorLineCap, MpVectorLineJoin, MpVectorPaint, MpVectorPathCommand,
    MpVectorPathPrimitive,
};
use makepad_widgets::{dvec2, vec2, vec4, Rect, Vec2f, Vec4f};

fn svg_color(color: published::SVGColor, alpha_scale: f32) -> Vec4f {
    vec4(
        color.red,
        color.green,
        color.blue,
        (color.alpha * alpha_scale).clamp(0.0, 1.0),
    )
}

pub(crate) fn lower_svg_path_primitives(
    generation: &published::FragmentArenaGeneration,
    _bounds: Rect,
    svg: &published::SVGPathFragment,
    spatial_id: makepad_browser_scene::MpSpatialId,
    clip_chain_id: makepad_browser_scene::MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
    owner_node_id: Option<usize>,
) -> Vec<MpPrimitive> {
    let bounds = Rect {
        pos: dvec2(
            svg.decorated_bounding_box.origin.x as f64,
            svg.decorated_bounding_box.origin.y as f64,
        ),
        size: dvec2(
            svg.decorated_bounding_box.size.width as f64,
            svg.decorated_bounding_box.size.height as f64,
        ),
    };
    let commands = Arc::<[MpVectorPathCommand]>::from(
        svg.path
            .commands
            .iter()
            .map(convert_path_command)
            .collect::<Vec<_>>(),
    );
    let mut primitives = Vec::new();

    if let Some(fill) = lower_svg_paint(generation, &svg.object_bounding_box, &svg.fill, 1.0) {
        primitives.push(MpPrimitive {
            id: MpPrimitiveId(0),
            spatial_id,
            clip_chain_id,
            effect_id,
            bounds,
            kind: makepad_browser_scene::MpPrimitiveKind::VectorPath(MpVectorPathPrimitive {
                commands: commands.clone(),
                draw: MpVectorDraw::Fill {
                    fill_rule: match svg.path.fill_rule {
                        published::SVGFillRule::NonZero => MpVectorFillRule::NonZero,
                        published::SVGFillRule::EvenOdd => MpVectorFillRule::EvenOdd,
                    },
                },
                paint: fill,
            }),
            hit_test_tag: owner_node_id.map(|id| makepad_browser_scene::MpHitTestTag(id as u64)),
        });
    }

    if let Some(stroke) = &svg.stroke {
        if let Some(paint) = lower_svg_paint(
            generation,
            &svg.object_bounding_box,
            &stroke.paint,
            stroke.opacity,
        ) {
            primitives.push(MpPrimitive {
                id: MpPrimitiveId(0),
                spatial_id,
                clip_chain_id,
                effect_id,
                bounds,
                kind: makepad_browser_scene::MpPrimitiveKind::VectorPath(MpVectorPathPrimitive {
                    commands,
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
                        non_scaling: stroke.non_scaling,
                    },
                    paint,
                }),
                hit_test_tag: owner_node_id.map(|id| makepad_browser_scene::MpHitTestTag(id as u64)),
            });
        }
    }

    primitives
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
    paint: &published::SVGPaint,
    opacity: f32,
) -> Option<MpVectorPaint> {
    match paint {
        published::SVGPaint::None | published::SVGPaint::CurrentColor => None,
        published::SVGPaint::SolidColor(color) => Some(MpVectorPaint::Solid {
            color: svg_color(*color, opacity),
        }),
        published::SVGPaint::Resource(resource_id) => {
            lower_svg_gradient_paint(generation, object_bounding_box, *resource_id, opacity)
        }
    }
}

fn lower_svg_gradient_paint(
    generation: &published::FragmentArenaGeneration,
    object_bounding_box: &published::SVGRect,
    resource_id: published::SVGResourceId,
    opacity: f32,
) -> Option<MpVectorPaint> {
    let resource = generation.svg_resource(resource_id)?;
    let published::SVGResourceKind::Gradient(gradient) = &resource.kind else {
        return None;
    };
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
            let start = gradient_point(gradient.units, object_bounding_box, linear.start);
            let end = gradient_point(gradient.units, object_bounding_box, linear.end);
            Some(MpVectorPaint::LinearGradient {
                start,
                end,
                repeating: matches!(
                    gradient.spread_method,
                    published::SVGGradientSpreadMethod::Reflect |
                        published::SVGGradientSpreadMethod::Repeat
                ),
                stops,
            })
        }
        published::SVGGradientKind::Radial(radial) => {
            let center = gradient_point(gradient.units, object_bounding_box, radial.center);
            let radius = gradient_radius(gradient.units, object_bounding_box, radial.radius);
            Some(MpVectorPaint::RadialGradient {
                center,
                radius,
                repeating: matches!(
                    gradient.spread_method,
                    published::SVGGradientSpreadMethod::Reflect |
                        published::SVGGradientSpreadMethod::Repeat
                ),
                stops,
            })
        }
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
