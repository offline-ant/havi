use havi_types::fragment_tree as published;
use makepad_browser_scene::{
    MpGradientStop, MpLinearGradient, MpPrimitive, MpPrimitiveId, MpPrimitiveKind,
    MpRadialGradient,
};
use makepad_widgets::{vec2, vec4, Rect, Vec4f};

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
    bounds: Rect,
    svg: &published::SVGPathFragment,
    spatial_id: makepad_browser_scene::MpSpatialId,
    clip_chain_id: makepad_browser_scene::MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
    owner_node_id: Option<usize>,
) -> Vec<MpPrimitive> {
    let mut primitives = Vec::new();

    if let Some(fill_primitive) = lower_svg_fill(
        generation,
        bounds,
        &svg.fill,
        1.0,
        spatial_id,
        clip_chain_id,
        effect_id,
        owner_node_id,
    ) {
        primitives.push(fill_primitive);
    }

    if let Some(stroke) = &svg.stroke {
        if let Some(stroke_primitive) = lower_svg_stroke(
            generation,
            bounds,
            stroke,
            spatial_id,
            clip_chain_id,
            effect_id,
            owner_node_id,
        ) {
            primitives.push(stroke_primitive);
        }
    }

    primitives
}

fn lower_svg_fill(
    generation: &published::FragmentArenaGeneration,
    bounds: Rect,
    paint: &published::SVGPaint,
    opacity: f32,
    spatial_id: makepad_browser_scene::MpSpatialId,
    clip_chain_id: makepad_browser_scene::MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
    owner_node_id: Option<usize>,
) -> Option<MpPrimitive> {
    let kind = match paint {
        published::SVGPaint::None | published::SVGPaint::CurrentColor => return None,
        published::SVGPaint::SolidColor(color) => {
            makepad_browser_scene::MpPrimitiveKind::SolidRect(makepad_browser_scene::MpSolidRect {
                color: svg_color(*color, opacity),
            })
        }
        published::SVGPaint::Resource(resource_id) => {
            lower_svg_gradient_primitive_kind(generation, bounds, *resource_id)?
        }
    };

    Some(MpPrimitive {
        id: MpPrimitiveId(0),
        spatial_id,
        clip_chain_id,
        effect_id,
        bounds,
        kind,
        hit_test_tag: owner_node_id.map(|id| makepad_browser_scene::MpHitTestTag(id as u64)),
    })
}

fn lower_svg_stroke(
    generation: &published::FragmentArenaGeneration,
    bounds: Rect,
    stroke: &published::SVGStrokeStyle,
    spatial_id: makepad_browser_scene::MpSpatialId,
    clip_chain_id: makepad_browser_scene::MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
    owner_node_id: Option<usize>,
) -> Option<MpPrimitive> {
    match &stroke.paint {
        published::SVGPaint::SolidColor(color) => Some(MpPrimitive {
            id: MpPrimitiveId(0),
            spatial_id,
            clip_chain_id,
            effect_id,
            bounds,
            kind: makepad_browser_scene::MpPrimitiveKind::Border(makepad_browser_scene::MpBorder {
                color: svg_color(*color, stroke.opacity),
                width: stroke.width.max(0.0),
                radius: makepad_browser_scene::MpPerCornerRadius::uniform(0.0),
            }),
            hit_test_tag: owner_node_id.map(|id| makepad_browser_scene::MpHitTestTag(id as u64)),
        }),
        published::SVGPaint::Resource(resource_id) => lower_svg_fill(
            generation,
            bounds,
            &published::SVGPaint::Resource(*resource_id),
            stroke.opacity,
            spatial_id,
            clip_chain_id,
            effect_id,
            owner_node_id,
        ),
        published::SVGPaint::None | published::SVGPaint::CurrentColor => None,
    }
}

fn lower_svg_gradient_primitive_kind(
    generation: &published::FragmentArenaGeneration,
    bounds: Rect,
    resource_id: published::SVGResourceId,
) -> Option<MpPrimitiveKind> {
    let resource = generation.svg_resource(resource_id)?;
    match &resource.kind {
        published::SVGResourceKind::Gradient(gradient) => match &gradient.kind {
            published::SVGGradientKind::Linear(linear) => Some(MpPrimitiveKind::LinearGradient(
                MpLinearGradient {
                    start: vec2(linear.start.x, linear.start.y),
                    end: vec2(linear.end.x, linear.end.y),
                    repeating: matches!(
                        gradient.spread_method,
                        published::SVGGradientSpreadMethod::Reflect |
                            published::SVGGradientSpreadMethod::Repeat
                    ),
                    stops: gradient
                        .stops
                        .iter()
                        .map(|stop| MpGradientStop {
                            offset: stop.offset,
                            color: svg_color(stop.color, stop.opacity),
                        })
                        .collect(),
                },
            )),
            published::SVGGradientKind::Radial(radial) => {
                let size = bounds.size;
                Some(MpPrimitiveKind::RadialGradient(MpRadialGradient {
                    center: vec2(radial.center.x, radial.center.y),
                    radius: vec2(
                        (radial.radius / size.x.max(1.0) as f32).max(0.0),
                        (radial.radius / size.y.max(1.0) as f32).max(0.0),
                    ),
                    repeating: matches!(
                        gradient.spread_method,
                        published::SVGGradientSpreadMethod::Reflect |
                            published::SVGGradientSpreadMethod::Repeat
                    ),
                    stops: gradient
                        .stops
                        .iter()
                        .map(|stop| MpGradientStop {
                            offset: stop.offset,
                            color: svg_color(stop.color, stop.opacity),
                        })
                        .collect(),
                }))
            }
        },
        _ => None,
    }
}
