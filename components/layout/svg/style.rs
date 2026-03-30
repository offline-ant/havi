use std::str::FromStr;

use style::color::ColorSpace;
use style::computed_values::pointer_events::T as PointerEvents;
use style::computed_values::visibility::T as Visibility;
use style::properties::ComputedValues;

use havi_types::fragment_tree::{
    SVGColor, SVGFillRule, SVGLineCap, SVGLineJoin, SVGPaintOrder, SVGTextAnchor,
    SVGVectorEffect,
};
use layout_api::{
    SVGElementData, SVGNodeKind, SVGPaintData, SVGPointerEventsValue as SVGPointerEvents,
    SVGPreserveAspectRatioValue, SVGTextBaselineValue, resolve_svg_length_to_user_units,
};

#[derive(Clone, Debug)]
pub struct SVGViewportStyle {
    pub overflow_hidden: bool,
    pub preserve_aspect_ratio: SVGPreserveAspectRatioValue,
    pub displayed: bool,
    pub visible: bool,
    pub opacity: f32,
}

impl Default for SVGViewportStyle {
    fn default() -> Self {
        Self {
            overflow_hidden: false,
            preserve_aspect_ratio: Default::default(),
            displayed: true,
            visible: true,
            opacity: 1.0,
        }
    }
}

#[derive(Clone, Debug)]
pub enum SVGResolvedPaint {
    None,
    SolidColor(SVGColor),
    CurrentColor,
    ContextFill,
    ContextStroke,
    ResourceReference(SVGPaintServerReference),
}

#[derive(Clone, Debug)]
pub struct SVGPaintServerReference {
    pub iri: String,
    pub fallback: Option<SVGPaintFallback>,
}

#[derive(Clone, Debug)]
pub enum SVGPaintFallback {
    None,
    SolidColor(SVGColor),
}

#[derive(Clone, Debug)]
pub struct SVGResolvedStroke {
    pub paint: SVGResolvedPaint,
    pub width: f32,
    pub opacity: f32,
    pub line_cap: SVGLineCap,
    pub line_join: SVGLineJoin,
    pub miter_limit: f32,
    pub dash_array: Vec<f32>,
    pub dash_offset: f32,
    pub vector_effect: SVGVectorEffect,
}

#[derive(Clone, Debug)]
pub struct SVGGeometryStyle {
    pub paint: SVGPaintStyle,
    pub resources: SVGResourceReferenceStyle,
    pub opacity: f32,
    pub displayed: bool,
    pub visible: bool,
    pub pointer_events: SVGPointerEvents,
    pub fill_rule: SVGFillRule,
    pub clip_rule: SVGFillRule,
    pub non_scaling_stroke: bool,
}

impl Default for SVGGeometryStyle {
    fn default() -> Self {
        Self {
            paint: SVGPaintStyle::default(),
            resources: SVGResourceReferenceStyle::default(),
            opacity: 1.0,
            displayed: true,
            visible: true,
            pointer_events: SVGPointerEvents::Auto,
            fill_rule: SVGFillRule::NonZero,
            clip_rule: SVGFillRule::NonZero,
            non_scaling_stroke: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SVGTextStyle {
    pub paint: SVGPaintStyle,
    pub resources: SVGResourceReferenceStyle,
    pub opacity: f32,
    pub displayed: bool,
    pub visible: bool,
    pub pointer_events: SVGPointerEvents,
    pub text_anchor: SVGTextAnchor,
    pub alignment_baseline: Option<SVGTextBaselineValue>,
    pub dominant_baseline: Option<SVGTextBaselineValue>,
}

impl Default for SVGTextStyle {
    fn default() -> Self {
        Self {
            paint: SVGPaintStyle::default(),
            resources: SVGResourceReferenceStyle::default(),
            opacity: 1.0,
            displayed: true,
            visible: true,
            pointer_events: SVGPointerEvents::Auto,
            text_anchor: SVGTextAnchor::Start,
            alignment_baseline: None,
            dominant_baseline: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SVGPaintStyle {
    pub current_color: SVGColor,
    pub fill: SVGResolvedPaint,
    pub fill_opacity: f32,
    pub stroke: Option<SVGResolvedStroke>,
    pub paint_order: SVGPaintOrder,
}

impl Default for SVGPaintStyle {
    fn default() -> Self {
        Self {
            current_color: svg_black(),
            fill: SVGResolvedPaint::SolidColor(svg_black()),
            fill_opacity: 1.0,
            stroke: None,
            paint_order: SVGPaintOrder::Normal,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SVGResourceReferenceStyle {
    pub clip_path: Option<String>,
    pub mask: Option<String>,
    pub filter: Option<String>,
    pub marker_start: Option<String>,
    pub marker_mid: Option<String>,
    pub marker_end: Option<String>,
}

pub fn resolve_viewport_style(element: &SVGElementData<'_>, computed: &ComputedValues) -> SVGViewportStyle {
    let defaults = resolve_common_state(&element.paint, computed);
    let viewport = element.viewport();
    SVGViewportStyle {
        overflow_hidden: viewport.is_some_and(|viewport| viewport.overflow_hidden),
        preserve_aspect_ratio: viewport.map(|viewport| viewport.preserve_aspect_ratio).unwrap_or_default(),
        displayed: defaults.displayed,
        visible: defaults.visible,
        opacity: defaults.opacity,
    }
}

pub fn resolve_geometry_style(
    element: &SVGElementData<'_>,
    computed: &ComputedValues,
    inherited: Option<&SVGGeometryStyle>,
) -> SVGGeometryStyle {
    let inherited_paint = inherited.map(|style| &style.paint);
    let paint = resolve_paint_style(&element.paint, computed, inherited_paint);
    let common = resolve_common_state(&element.paint, computed);
    let resources = resolve_resource_reference_style(&element.paint);
    let fill_rule = element.paint.fill_rule.unwrap_or(SVGFillRule::NonZero);
    let clip_rule = element.paint.clip_rule.unwrap_or(SVGFillRule::NonZero);
    let non_scaling_stroke = element.paint.vector_effect == Some(SVGVectorEffect::NonScalingStroke);
    SVGGeometryStyle {
        paint,
        resources,
        opacity: common.opacity,
        displayed: common.displayed,
        visible: common.visible,
        pointer_events: common.pointer_events,
        fill_rule,
        clip_rule,
        non_scaling_stroke,
    }
}

pub fn resolve_text_style(
    element: &SVGElementData<'_>,
    computed: &ComputedValues,
    inherited: Option<&SVGTextStyle>,
) -> SVGTextStyle {
    let inherited_paint = inherited.map(|style| &style.paint);
    let paint = resolve_paint_style(&element.paint, computed, inherited_paint);
    let common = resolve_common_state(&element.paint, computed);
    let resources = resolve_resource_reference_style(&element.paint);
    let text_data = match &element.node_kind {
        SVGNodeKind::Text(text) | SVGNodeKind::TSpan(text) => Some(text),
        SVGNodeKind::TextPath(text_path) => Some(&text_path.text),
        _ => None,
    };
    SVGTextStyle {
        paint,
        resources,
        opacity: common.opacity,
        displayed: common.displayed,
        visible: common.visible,
        pointer_events: common.pointer_events,
        text_anchor: text_data
            .and_then(|text| text.text_anchor)
            .unwrap_or(SVGTextAnchor::Start),
        alignment_baseline: text_data.and_then(|text| text.alignment_baseline),
        dominant_baseline: text_data.and_then(|text| text.dominant_baseline),
    }
}

pub fn resolve_paint_style(
    paint_data: &SVGPaintData<'_>,
    computed: &ComputedValues,
    inherited: Option<&SVGPaintStyle>,
) -> SVGPaintStyle {
    let current_color = resolve_current_color(paint_data.color, computed);
    let inherited_fill = inherited.map(|style| &style.fill);
    let fill = resolve_paint(
        paint_data.fill,
        current_color,
        inherited_fill,
        SVGResolvedPaint::SolidColor(svg_black()),
    );
    let fill_opacity = paint_data.fill_opacity.unwrap_or(1.0);
    let stroke_paint = resolve_paint(
        paint_data.stroke,
        current_color,
        inherited.and_then(|style| style.stroke.as_ref()).map(|stroke| &stroke.paint),
        SVGResolvedPaint::None,
    );
    let inherited_stroke = inherited.and_then(|style| style.stroke.as_ref());
    let vector_effect = paint_data
        .vector_effect
        .or_else(|| inherited_stroke.map(|stroke| stroke.vector_effect))
        .unwrap_or(SVGVectorEffect::None);
    let stroke = match stroke_paint {
        SVGResolvedPaint::None => None,
        paint => Some(SVGResolvedStroke {
            paint,
            width: paint_data
                .stroke_width
                .and_then(resolve_svg_length_to_user_units)
                .or_else(|| inherited_stroke.map(|stroke| stroke.width))
                .unwrap_or(1.0),
            opacity: paint_data
                .stroke_opacity
                .or_else(|| inherited_stroke.map(|stroke| stroke.opacity))
                .unwrap_or(1.0),
            line_cap: paint_data
                .stroke_linecap
                .or_else(|| inherited_stroke.map(|stroke| stroke.line_cap))
                .unwrap_or(SVGLineCap::Butt),
            line_join: paint_data
                .stroke_linejoin
                .or_else(|| inherited_stroke.map(|stroke| stroke.line_join))
                .unwrap_or(SVGLineJoin::Miter),
            miter_limit: paint_data
                .stroke_miterlimit
                .or_else(|| inherited_stroke.map(|stroke| stroke.miter_limit))
                .unwrap_or(4.0),
            dash_array: paint_data
                .stroke_dasharray
                .clone()
                .or_else(|| inherited_stroke.map(|stroke| stroke.dash_array.clone()))
                .unwrap_or_default(),
            dash_offset: paint_data
                .stroke_dashoffset
                .or_else(|| inherited_stroke.map(|stroke| stroke.dash_offset))
                .unwrap_or(0.0),
            vector_effect,
        }),
    };
    SVGPaintStyle {
        current_color,
        fill,
        fill_opacity,
        stroke,
        paint_order: paint_data
            .paint_order
            .or_else(|| inherited.map(|style| style.paint_order))
            .unwrap_or(SVGPaintOrder::Normal),
    }
}

pub fn resolve_resource_reference_style(paint_data: &SVGPaintData<'_>) -> SVGResourceReferenceStyle {
    SVGResourceReferenceStyle {
        clip_path: paint_data.clip_path.and_then(|reference| reference.local_reference).map(str::to_owned),
        mask: paint_data.mask.and_then(|reference| reference.local_reference).map(str::to_owned),
        filter: paint_data.filter.and_then(|reference| reference.local_reference).map(str::to_owned),
        marker_start: paint_data.marker_start.and_then(|reference| reference.local_reference).map(str::to_owned),
        marker_mid: paint_data.marker_mid.and_then(|reference| reference.local_reference).map(str::to_owned),
        marker_end: paint_data.marker_end.and_then(|reference| reference.local_reference).map(str::to_owned),
    }
}

#[derive(Clone, Copy, Debug)]
struct SVGCommonResolvedState {
    displayed: bool,
    visible: bool,
    opacity: f32,
    pointer_events: SVGPointerEvents,
}

fn resolve_common_state(paint_data: &SVGPaintData<'_>, computed: &ComputedValues) -> SVGCommonResolvedState {
    SVGCommonResolvedState {
        displayed: !computed.clone_display().is_none(),
        visible: computed.get_inherited_box().visibility == Visibility::Visible,
        opacity: paint_data.opacity.unwrap_or(computed.get_effects().opacity),
        pointer_events: paint_data.pointer_events.unwrap_or_else(|| {
                if computed.get_inherited_ui().pointer_events == PointerEvents::None {
                    SVGPointerEvents::None
                } else {
                    SVGPointerEvents::Auto
                }
            }),
    }
}

fn resolve_current_color(raw_color: Option<&str>, computed: &ComputedValues) -> SVGColor {
    raw_color
        .and_then(parse_color)
        .unwrap_or_else(|| {
            let srgb = computed
                .get_inherited_text()
                .color
                .to_color_space(ColorSpace::Srgb);
            SVGColor {
                red: srgb.components.0,
                green: srgb.components.1,
                blue: srgb.components.2,
                alpha: srgb.alpha,
            }
        })
}

fn resolve_paint(
    raw: Option<&str>,
    current_color: SVGColor,
    inherited: Option<&SVGResolvedPaint>,
    default: SVGResolvedPaint,
) -> SVGResolvedPaint {
    let Some(raw) = raw else {
        return inherited.cloned().unwrap_or(default);
    };
    if raw.trim() == "inherit" {
        return inherited.cloned().unwrap_or(default);
    }
    parse_paint(raw, current_color).unwrap_or_else(|| inherited.cloned().unwrap_or(default))
}

fn parse_paint(raw: &str, current_color: SVGColor) -> Option<SVGResolvedPaint> {
    parse_svgtypes_paint(raw, current_color)
        .or_else(|| parse_func_iri_paint(raw, current_color))
}

fn parse_svgtypes_paint(raw: &str, current_color: SVGColor) -> Option<SVGResolvedPaint> {
    match svgtypes::Paint::from_str(raw).ok()? {
        svgtypes::Paint::None => Some(SVGResolvedPaint::None),
        svgtypes::Paint::CurrentColor => Some(SVGResolvedPaint::CurrentColor),
        svgtypes::Paint::Color(color) => Some(SVGResolvedPaint::SolidColor(convert_svgtypes_color(color))),
        svgtypes::Paint::FuncIRI(iri, fallback) => Some(SVGResolvedPaint::ResourceReference(
            SVGPaintServerReference {
                iri: iri.to_owned(),
                fallback: fallback.map(|fallback| match fallback {
                    svgtypes::PaintFallback::None => SVGPaintFallback::None,
                    svgtypes::PaintFallback::CurrentColor => {
                        SVGPaintFallback::SolidColor(current_color)
                    }
                    svgtypes::PaintFallback::Color(color) => {
                        SVGPaintFallback::SolidColor(convert_svgtypes_color(color))
                    }
                }),
            },
        )),
        svgtypes::Paint::Inherit => None,
        svgtypes::Paint::ContextFill => Some(SVGResolvedPaint::ContextFill),
        svgtypes::Paint::ContextStroke => Some(SVGResolvedPaint::ContextStroke),
    }
}

fn parse_func_iri_paint(raw: &str, current_color: SVGColor) -> Option<SVGResolvedPaint> {
    let raw = raw.trim();
    let tail = raw.strip_prefix("url(")?;
    let close = tail.find(')')?;
    let iri = tail[..close].trim();
    if iri.is_empty() {
        return None;
    }
    let fallback = match tail[close + 1..].trim() {
        "" => None,
        "none" => Some(SVGPaintFallback::None),
        "currentColor" => Some(SVGPaintFallback::SolidColor(current_color)),
        color => Some(SVGPaintFallback::SolidColor(parse_color(color)?)),
    };
    Some(SVGResolvedPaint::ResourceReference(SVGPaintServerReference {
        iri: iri.to_owned(),
        fallback,
    }))
}

fn parse_color(raw: &str) -> Option<SVGColor> {
    svgtypes::Color::from_str(raw).ok().map(convert_svgtypes_color)
}

fn convert_svgtypes_color(color: svgtypes::Color) -> SVGColor {
    SVGColor {
        red: color.red as f32 / 255.0,
        green: color.green as f32 / 255.0,
        blue: color.blue as f32 / 255.0,
        alpha: color.alpha as f32 / 255.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_paint_keeps_func_iri_fallback_color() {
        let current = SVGColor {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            alpha: 1.0,
        };
        let paint = parse_paint("url(#grad) green", current).expect("paint");
        match paint {
            SVGResolvedPaint::ResourceReference(reference) => {
                assert_eq!(reference.iri, "grad");
                match reference.fallback {
                    Some(SVGPaintFallback::SolidColor(color)) => {
                        assert_eq!(color.green, 128.0 / 255.0);
                    }
                    other => panic!("unexpected fallback: {other:?}"),
                }
            }
            other => panic!("unexpected paint: {other:?}"),
        }
    }

    #[test]
    fn parse_paint_preserves_context_and_currentcolor_variants() {
        let current = SVGColor {
            red: 1.0,
            green: 0.0,
            blue: 0.0,
            alpha: 1.0,
        };
        assert!(matches!(
            parse_paint("currentColor", current),
            Some(SVGResolvedPaint::CurrentColor)
        ));
        assert!(matches!(
            parse_paint("context-fill", current),
            Some(SVGResolvedPaint::ContextFill)
        ));
        assert!(matches!(
            parse_paint("context-stroke", current),
            Some(SVGResolvedPaint::ContextStroke)
        ));
    }

    #[test]
    fn phase2_uses_shared_dash_and_paint_order_parsers() {
        assert_eq!(
            layout_api::parse_svg_paint_order(Some("stroke markers fill")),
            Some(SVGPaintOrder::StrokeMarkersFill)
        );
        assert_eq!(
            layout_api::parse_svg_paint_order(Some("stroke fill")),
            Some(SVGPaintOrder::StrokeFillMarkers)
        );
        assert_eq!(
            layout_api::parse_svg_dash_array(Some("1, 2 3")),
            Some(vec![1.0, 2.0, 3.0])
        );
    }
}

fn svg_black() -> SVGColor {
    SVGColor {
        red: 0.0,
        green: 0.0,
        blue: 0.0,
        alpha: 1.0,
    }
}
