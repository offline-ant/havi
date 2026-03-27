use std::str::FromStr;

use style::color::ColorSpace;
use style::computed_values::pointer_events::T as PointerEvents;
use style::computed_values::visibility::T as Visibility;
use style::properties::ComputedValues;

use havi_types::fragment_tree::{SVGColor, SVGFillRule, SVGLineCap, SVGLineJoin};
use layout_api::{SVGElementData, SVGNodeKind, SVGPaintData};

#[derive(Clone, Debug)]
pub struct SVGViewportStyle {
    pub overflow_hidden: bool,
    pub preserve_aspect_ratio: Option<svgtypes::AspectRatio>,
    pub displayed: bool,
    pub visible: bool,
    pub opacity: f32,
}

impl Default for SVGViewportStyle {
    fn default() -> Self {
        Self {
            overflow_hidden: false,
            preserve_aspect_ratio: None,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGPointerEvents {
    Auto,
    None,
    VisiblePainted,
    VisibleFill,
    VisibleStroke,
    Visible,
    Painted,
    Fill,
    Stroke,
    All,
    BoundingBox,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGTextAnchor {
    Start,
    Middle,
    End,
}

#[derive(Clone, Debug)]
pub struct SVGResolvedStroke {
    pub paint: SVGResolvedPaint,
    pub width: f32,
    pub opacity: f32,
    pub line_cap: SVGLineCap,
    pub line_join: SVGLineJoin,
    pub miter_limit: f32,
    pub non_scaling: bool,
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
    pub alignment_baseline: Option<String>,
    pub dominant_baseline: Option<String>,
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
}

impl Default for SVGPaintStyle {
    fn default() -> Self {
        Self {
            current_color: svg_black(),
            fill: SVGResolvedPaint::SolidColor(svg_black()),
            fill_opacity: 1.0,
            stroke: None,
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
    let preserve_aspect_ratio = element
        .viewport()
        .and_then(|viewport| viewport.preserve_aspect_ratio)
        .and_then(|raw| raw.parse::<svgtypes::AspectRatio>().ok());
    let overflow_hidden = element
        .viewport()
        .and_then(|viewport| viewport.overflow)
        .is_some_and(|overflow| matches!(overflow, "hidden" | "scroll" | "auto"));
    SVGViewportStyle {
        overflow_hidden,
        preserve_aspect_ratio,
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
    let fill_rule = element
        .paint
        .fill_rule
        .and_then(parse_fill_rule)
        .unwrap_or(SVGFillRule::NonZero);
    let clip_rule = element
        .paint
        .clip_rule
        .and_then(parse_fill_rule)
        .unwrap_or(SVGFillRule::NonZero);
    let non_scaling_stroke = element
        .paint
        .vector_effect
        .is_some_and(|value| value.split_whitespace().any(|part| part == "non-scaling-stroke"));
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
            .and_then(parse_text_anchor)
            .unwrap_or(SVGTextAnchor::Start),
        alignment_baseline: text_data.and_then(|text| text.alignment_baseline).map(ToOwned::to_owned),
        dominant_baseline: text_data.and_then(|text| text.dominant_baseline).map(ToOwned::to_owned),
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
    let fill_opacity = paint_data
        .fill_opacity
        .and_then(parse_unit_interval)
        .unwrap_or(1.0);
    let stroke_paint = resolve_paint(
        paint_data.stroke,
        current_color,
        inherited.and_then(|style| style.stroke.as_ref()).map(|stroke| &stroke.paint),
        SVGResolvedPaint::None,
    );
    let stroke = match stroke_paint {
        SVGResolvedPaint::None => None,
        paint => {
            let inherited_stroke = inherited.and_then(|style| style.stroke.as_ref());
            Some(SVGResolvedStroke {
                paint,
                width: paint_data
                    .stroke_width
                    .and_then(parse_non_negative_number)
                    .or_else(|| inherited_stroke.map(|stroke| stroke.width))
                    .unwrap_or(1.0),
                opacity: paint_data
                    .stroke_opacity
                    .and_then(parse_unit_interval)
                    .or_else(|| inherited_stroke.map(|stroke| stroke.opacity))
                    .unwrap_or(1.0),
                line_cap: paint_data
                    .stroke_linecap
                    .and_then(parse_line_cap)
                    .or_else(|| inherited_stroke.map(|stroke| stroke.line_cap))
                    .unwrap_or(SVGLineCap::Butt),
                line_join: paint_data
                    .stroke_linejoin
                    .and_then(parse_line_join)
                    .or_else(|| inherited_stroke.map(|stroke| stroke.line_join))
                    .unwrap_or(SVGLineJoin::Miter),
                miter_limit: paint_data
                    .stroke_miterlimit
                    .and_then(parse_non_negative_number)
                    .or_else(|| inherited_stroke.map(|stroke| stroke.miter_limit))
                    .unwrap_or(4.0),
                non_scaling: paint_data
                    .vector_effect
                    .is_some_and(|value| value.split_whitespace().any(|part| part == "non-scaling-stroke")),
            })
        }
    };
    SVGPaintStyle {
        current_color,
        fill,
        fill_opacity,
        stroke,
    }
}

pub fn resolve_resource_reference_style(paint_data: &SVGPaintData<'_>) -> SVGResourceReferenceStyle {
    SVGResourceReferenceStyle {
        clip_path: paint_data.clip_path.and_then(parse_resource_iri),
        mask: paint_data.mask.and_then(parse_resource_iri),
        filter: paint_data.filter.and_then(parse_resource_iri),
        marker_start: paint_data.marker_start.and_then(parse_resource_iri),
        marker_mid: paint_data.marker_mid.and_then(parse_resource_iri),
        marker_end: paint_data.marker_end.and_then(parse_resource_iri),
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
        opacity: paint_data
            .opacity
            .and_then(parse_unit_interval)
            .unwrap_or(computed.get_effects().opacity),
        pointer_events: paint_data
            .pointer_events
            .and_then(parse_pointer_events)
            .unwrap_or_else(|| {
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
    if raw == "inherit" || raw == "context-fill" || raw == "context-stroke" {
        return inherited.cloned().unwrap_or(default);
    }
    parse_paint(raw, current_color).unwrap_or_else(|| inherited.cloned().unwrap_or(default))
}

fn parse_paint(raw: &str, current_color: SVGColor) -> Option<SVGResolvedPaint> {
    match svgtypes::Paint::from_str(raw).ok()? {
        svgtypes::Paint::None => Some(SVGResolvedPaint::None),
        svgtypes::Paint::CurrentColor => Some(SVGResolvedPaint::SolidColor(current_color)),
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
        svgtypes::Paint::Inherit | svgtypes::Paint::ContextFill | svgtypes::Paint::ContextStroke => {
            None
        }
    }
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

fn parse_resource_iri(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let inner = raw.strip_prefix("url(")?.strip_suffix(')')?.trim();
    let inner = inner.strip_prefix('#').or_else(|| inner.strip_prefix("'#")).or_else(|| inner.strip_prefix("\"#"))?;
    Some(
        inner
            .trim_matches(|ch| ch == '\'' || ch == '"' || ch == ' ')
            .to_owned(),
    )
}

fn parse_unit_interval(raw: &str) -> Option<f32> {
    let value = raw.trim().parse::<f32>().ok()?;
    Some(value.clamp(0.0, 1.0))
}

fn parse_non_negative_number(raw: &str) -> Option<f32> {
    let value = raw.trim().parse::<f32>().ok()?;
    if value.is_finite() && value >= 0.0 {
        Some(value)
    } else {
        None
    }
}

fn parse_fill_rule(raw: &str) -> Option<SVGFillRule> {
    match raw.trim() {
        "nonzero" => Some(SVGFillRule::NonZero),
        "evenodd" => Some(SVGFillRule::EvenOdd),
        _ => None,
    }
}

fn parse_line_cap(raw: &str) -> Option<SVGLineCap> {
    match raw.trim() {
        "butt" => Some(SVGLineCap::Butt),
        "round" => Some(SVGLineCap::Round),
        "square" => Some(SVGLineCap::Square),
        _ => None,
    }
}

fn parse_line_join(raw: &str) -> Option<SVGLineJoin> {
    match raw.trim() {
        "miter" => Some(SVGLineJoin::Miter),
        "round" => Some(SVGLineJoin::Round),
        "bevel" => Some(SVGLineJoin::Bevel),
        _ => None,
    }
}

fn parse_pointer_events(raw: &str) -> Option<SVGPointerEvents> {
    match raw.trim() {
        "auto" => Some(SVGPointerEvents::Auto),
        "none" => Some(SVGPointerEvents::None),
        "visiblePainted" => Some(SVGPointerEvents::VisiblePainted),
        "visibleFill" => Some(SVGPointerEvents::VisibleFill),
        "visibleStroke" => Some(SVGPointerEvents::VisibleStroke),
        "visible" => Some(SVGPointerEvents::Visible),
        "painted" => Some(SVGPointerEvents::Painted),
        "fill" => Some(SVGPointerEvents::Fill),
        "stroke" => Some(SVGPointerEvents::Stroke),
        "all" => Some(SVGPointerEvents::All),
        "bounding-box" => Some(SVGPointerEvents::BoundingBox),
        _ => None,
    }
}

fn parse_text_anchor(raw: &str) -> Option<SVGTextAnchor> {
    match raw.trim() {
        "start" => Some(SVGTextAnchor::Start),
        "middle" => Some(SVGTextAnchor::Middle),
        "end" => Some(SVGTextAnchor::End),
        _ => None,
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
