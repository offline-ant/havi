use std::sync::Arc;

use havi_types::fragment_tree as published;
use makepad_browser_scene::{
    MpGradientStop, MpPrimitive, MpPrimitiveId, MpVectorDraw, MpVectorFillRule,
    MpVectorGradientSpreadMethod, MpVectorLineCap, MpVectorLineJoin, MpVectorPaint,
    MpVectorPathCommand, MpVectorPathPrimitive,
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
    use std::collections::HashMap;
    use std::sync::Arc;

    use app_units::Au;
    use havi_types::fragment_tree::{
        FragmentArenaGeneration, FragmentDerivedData, PaintChild, SVGColor,
        SVGCoordinateUnits, SVGGradientKind, SVGGradientResource,
        SVGGradientSpreadMethod, SVGGradientStop, SVGLinearGradient, SVGPaint,
        SVGRadialGradient, SVGResourceKind, SVGResourceNode, SVGTransform,
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
                kind: SVGResourceKind::Gradient(gradient),
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
            &SVGPaint::Resource(published::SVGResourceId(0)),
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
            &SVGPaint::Resource(published::SVGResourceId(0)),
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
