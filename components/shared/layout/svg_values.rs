use std::fmt::Write;

use euclid::default::Transform2D;
use havi_types::fragment_tree::{
    SVGCoordinateUnits, SVGFillRule, SVGGradientSpreadMethod, SVGLineCap, SVGLineJoin,
    SVGPaintOrder, SVGTextAnchor, SVGVectorEffect,
};

pub const SVG_LENGTHTYPE_UNKNOWN: u16 = 0;
pub const SVG_LENGTHTYPE_NUMBER: u16 = 1;
pub const SVG_LENGTHTYPE_PERCENTAGE: u16 = 2;
pub const SVG_LENGTHTYPE_EMS: u16 = 3;
pub const SVG_LENGTHTYPE_EXS: u16 = 4;
pub const SVG_LENGTHTYPE_PX: u16 = 5;
pub const SVG_LENGTHTYPE_CM: u16 = 6;
pub const SVG_LENGTHTYPE_MM: u16 = 7;
pub const SVG_LENGTHTYPE_IN: u16 = 8;
pub const SVG_LENGTHTYPE_PT: u16 = 9;
pub const SVG_LENGTHTYPE_PC: u16 = 10;

pub const SVG_TRANSFORM_UNKNOWN: u16 = 0;
pub const SVG_TRANSFORM_MATRIX: u16 = 1;
pub const SVG_TRANSFORM_TRANSLATE: u16 = 2;
pub const SVG_TRANSFORM_SCALE: u16 = 3;
pub const SVG_TRANSFORM_ROTATE: u16 = 4;
pub const SVG_TRANSFORM_SKEWX: u16 = 5;
pub const SVG_TRANSFORM_SKEWY: u16 = 6;

pub const SVG_PRESERVEASPECTRATIO_UNKNOWN: u16 = 0;
pub const SVG_PRESERVEASPECTRATIO_NONE: u16 = 1;
pub const SVG_PRESERVEASPECTRATIO_XMINYMIN: u16 = 2;
pub const SVG_PRESERVEASPECTRATIO_XMIDYMIN: u16 = 3;
pub const SVG_PRESERVEASPECTRATIO_XMAXYMIN: u16 = 4;
pub const SVG_PRESERVEASPECTRATIO_XMINYMID: u16 = 5;
pub const SVG_PRESERVEASPECTRATIO_XMIDYMID: u16 = 6;
pub const SVG_PRESERVEASPECTRATIO_XMAXYMID: u16 = 7;
pub const SVG_PRESERVEASPECTRATIO_XMINYMAX: u16 = 8;
pub const SVG_PRESERVEASPECTRATIO_XMIDYMAX: u16 = 9;
pub const SVG_PRESERVEASPECTRATIO_XMAXYMAX: u16 = 10;

pub const SVG_MEETORSLICE_UNKNOWN: u16 = 0;
pub const SVG_MEETORSLICE_MEET: u16 = 1;
pub const SVG_MEETORSLICE_SLICE: u16 = 2;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SVGLengthValue {
    pub unit_type: u16,
    pub value: f32,
}

pub type SVGLengthListValue = Vec<SVGLengthValue>;
pub type SVGNumberListValue = Vec<f32>;
pub type SVGTransformListValue = Vec<SVGTransformValue>;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SVGRectValue {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SVGPreserveAspectRatioValue {
    pub align: u16,
    pub meet_or_slice: u16,
}

impl Default for SVGPreserveAspectRatioValue {
    fn default() -> Self {
        Self {
            align: SVG_PRESERVEASPECTRATIO_XMIDYMID,
            meet_or_slice: SVG_MEETORSLICE_MEET,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SVGTransformValue {
    pub transform_type: u16,
    pub matrix: Transform2D<f32>,
    pub angle: f32,
}

impl Default for SVGTransformValue {
    fn default() -> Self {
        Self {
            transform_type: SVG_TRANSFORM_MATRIX,
            matrix: Transform2D::identity(),
            angle: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGLengthAdjustValue {
    Spacing,
    SpacingAndGlyphs,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGTextBaselineValue {
    Middle,
    Central,
    Hanging,
    TextBeforeEdge,
    TextAfterEdge,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGPointerEventsValue {
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
pub struct SVGReferenceValue<'a> {
    pub raw: &'a str,
    pub local_reference: Option<&'a str>,
}

pub fn parse_svg_enumeration(raw: Option<&str>, mapping: &[(&str, u16)]) -> u16 {
    let Some(raw) = raw.map(str::trim).filter(|raw| !raw.is_empty()) else {
        return 0;
    };
    mapping
        .iter()
        .find_map(|(name, value)| raw.eq(*name).then_some(*value))
        .unwrap_or(0)
}

pub fn serialize_svg_enumeration<'a>(value: u16, mapping: &'a [(&'a str, u16)]) -> Option<&'a str> {
    mapping
        .iter()
        .find_map(|(name, mapped)| (*mapped == value).then_some(*name))
}

pub fn parse_svg_length(raw: Option<&str>) -> SVGLengthValue {
    let Some(raw) = raw.map(str::trim).filter(|raw| !raw.is_empty()) else {
        return SVGLengthValue {
            unit_type: SVG_LENGTHTYPE_NUMBER,
            value: 0.0,
        };
    };
    let (number_text, unit_type) = parse_svg_length_components(raw)
        .unwrap_or(("", SVG_LENGTHTYPE_UNKNOWN));
    let Some(value) = parse_finite_f32(number_text) else {
        return SVGLengthValue {
            unit_type: SVG_LENGTHTYPE_UNKNOWN,
            value: 0.0,
        };
    };
    SVGLengthValue { unit_type, value }
}

pub fn serialize_svg_length(value: SVGLengthValue) -> String {
    let suffix = match value.unit_type {
        SVG_LENGTHTYPE_PERCENTAGE => "%",
        SVG_LENGTHTYPE_EMS => "em",
        SVG_LENGTHTYPE_EXS => "ex",
        SVG_LENGTHTYPE_PX => "px",
        SVG_LENGTHTYPE_CM => "cm",
        SVG_LENGTHTYPE_MM => "mm",
        SVG_LENGTHTYPE_IN => "in",
        SVG_LENGTHTYPE_PT => "pt",
        SVG_LENGTHTYPE_PC => "pc",
        _ => "",
    };
    format!("{}{}", trim_float(value.value), suffix)
}

pub fn parse_svg_length_list(raw: Option<&str>) -> SVGLengthListValue {
    let Some(raw) = raw.map(str::trim).filter(|raw| !raw.is_empty()) else {
        return Vec::new();
    };
    split_svg_arguments(raw)
        .into_iter()
        .map(|token| parse_svg_length(Some(token)))
        .collect()
}

pub fn serialize_svg_length_list(values: &[SVGLengthValue]) -> Option<String> {
    (!values.is_empty()).then(|| {
        values
            .iter()
            .map(|value| serialize_svg_length(*value))
            .collect::<Vec<_>>()
            .join(" ")
    })
}

pub fn resolve_svg_length_to_user_units(value: SVGLengthValue) -> Option<f32> {
    let resolved = match value.unit_type {
        SVG_LENGTHTYPE_UNKNOWN => return None,
        SVG_LENGTHTYPE_NUMBER | SVG_LENGTHTYPE_PX => value.value,
        SVG_LENGTHTYPE_IN => value.value * 96.0,
        SVG_LENGTHTYPE_CM => value.value * (96.0 / 2.54),
        SVG_LENGTHTYPE_MM => value.value * (96.0 / 25.4),
        SVG_LENGTHTYPE_PT => value.value * (96.0 / 72.0),
        SVG_LENGTHTYPE_PC => value.value * 16.0,
        SVG_LENGTHTYPE_PERCENTAGE | SVG_LENGTHTYPE_EMS | SVG_LENGTHTYPE_EXS => return None,
        _ => return None,
    };
    resolved.is_finite().then_some(resolved)
}

pub fn parse_svg_number(raw: Option<&str>) -> f32 {
    parse_svg_optional_number(raw).unwrap_or(0.0)
}

pub fn parse_svg_optional_number(raw: Option<&str>) -> Option<f32> {
    raw.and_then(|raw| parse_finite_f32(raw.trim()))
}

pub fn serialize_svg_number(value: f32) -> String {
    trim_float(value)
}

pub fn parse_svg_number_list(raw: Option<&str>) -> SVGNumberListValue {
    let Some(raw) = raw.map(str::trim).filter(|raw| !raw.is_empty()) else {
        return Vec::new();
    };
    split_svg_arguments(raw)
        .into_iter()
        .filter_map(parse_finite_f32)
        .collect()
}

pub fn serialize_svg_number_list(values: &[f32]) -> Option<String> {
    (!values.is_empty()).then(|| {
        values
            .iter()
            .map(|value| trim_float(*value))
            .collect::<Vec<_>>()
            .join(" ")
    })
}

pub fn parse_svg_view_box(raw: Option<&str>) -> SVGRectValue {
    parse_svg_optional_view_box(raw).unwrap_or_default()
}

pub fn parse_svg_optional_view_box(raw: Option<&str>) -> Option<SVGRectValue> {
    let values = parse_svg_number_list(raw);
    if values.len() != 4 {
        return None;
    }
    Some(SVGRectValue {
        x: values[0],
        y: values[1],
        width: values[2],
        height: values[3],
    })
}

pub fn serialize_svg_view_box(value: SVGRectValue) -> String {
    format!(
        "{} {} {} {}",
        trim_float(value.x),
        trim_float(value.y),
        trim_float(value.width),
        trim_float(value.height)
    )
}

pub fn svg_view_box_ratio(value: SVGRectValue) -> Option<f32> {
    (value.width > 0.0 && value.height > 0.0).then_some(value.width / value.height)
}

pub fn parse_svg_preserve_aspect_ratio(raw: Option<&str>) -> SVGPreserveAspectRatioValue {
    let Some(raw) = raw.map(str::trim).filter(|raw| !raw.is_empty()) else {
        return SVGPreserveAspectRatioValue::default();
    };
    let mut result = SVGPreserveAspectRatioValue::default();
    for token in raw.split_ascii_whitespace() {
        result.align = match token {
            "none" => SVG_PRESERVEASPECTRATIO_NONE,
            "xMinYMin" => SVG_PRESERVEASPECTRATIO_XMINYMIN,
            "xMidYMin" => SVG_PRESERVEASPECTRATIO_XMIDYMIN,
            "xMaxYMin" => SVG_PRESERVEASPECTRATIO_XMAXYMIN,
            "xMinYMid" => SVG_PRESERVEASPECTRATIO_XMINYMID,
            "xMidYMid" => SVG_PRESERVEASPECTRATIO_XMIDYMID,
            "xMaxYMid" => SVG_PRESERVEASPECTRATIO_XMAXYMID,
            "xMinYMax" => SVG_PRESERVEASPECTRATIO_XMINYMAX,
            "xMidYMax" => SVG_PRESERVEASPECTRATIO_XMIDYMAX,
            "xMaxYMax" => SVG_PRESERVEASPECTRATIO_XMAXYMAX,
            "meet" => {
                result.meet_or_slice = SVG_MEETORSLICE_MEET;
                continue;
            }
            "slice" => {
                result.meet_or_slice = SVG_MEETORSLICE_SLICE;
                continue;
            }
            _ => result.align,
        };
    }
    result
}

pub fn serialize_svg_preserve_aspect_ratio(value: SVGPreserveAspectRatioValue) -> String {
    let align = match value.align {
        SVG_PRESERVEASPECTRATIO_NONE => "none",
        SVG_PRESERVEASPECTRATIO_XMINYMIN => "xMinYMin",
        SVG_PRESERVEASPECTRATIO_XMIDYMIN => "xMidYMin",
        SVG_PRESERVEASPECTRATIO_XMAXYMIN => "xMaxYMin",
        SVG_PRESERVEASPECTRATIO_XMINYMID => "xMinYMid",
        SVG_PRESERVEASPECTRATIO_XMIDYMID => "xMidYMid",
        SVG_PRESERVEASPECTRATIO_XMAXYMID => "xMaxYMid",
        SVG_PRESERVEASPECTRATIO_XMINYMAX => "xMinYMax",
        SVG_PRESERVEASPECTRATIO_XMIDYMAX => "xMidYMax",
        SVG_PRESERVEASPECTRATIO_XMAXYMAX => "xMaxYMax",
        _ => "xMidYMid",
    };
    let meet_or_slice = match value.meet_or_slice {
        SVG_MEETORSLICE_SLICE => " slice",
        _ => " meet",
    };
    format!("{}{}", align, meet_or_slice)
}

pub fn parse_svg_transform_list(raw: Option<&str>) -> SVGTransformListValue {
    let Some(mut raw) = raw.map(str::trim).filter(|raw| !raw.is_empty()) else {
        return Vec::new();
    };
    let mut values = Vec::new();
    while !raw.is_empty() {
        raw = raw.trim_start_matches(|ch: char| ch.is_ascii_whitespace() || ch == ',');
        if raw.is_empty() {
            break;
        }
        let Some(open) = raw.find('(') else {
            break;
        };
        let Some(close) = raw[open + 1..].find(')') else {
            break;
        };
        let name = raw[..open].trim();
        let args = &raw[open + 1..open + 1 + close];
        if let Some(value) = parse_svg_transform(name, args) {
            values.push(value);
        }
        raw = &raw[open + 1 + close + 1..];
    }
    values
}

pub fn serialize_svg_transform_list(values: &[SVGTransformValue]) -> Option<String> {
    (!values.is_empty()).then(|| {
        values
            .iter()
            .map(|value| {
                format!(
                    "matrix({} {} {} {} {} {})",
                    trim_float(value.matrix.m11),
                    trim_float(value.matrix.m12),
                    trim_float(value.matrix.m21),
                    trim_float(value.matrix.m22),
                    trim_float(value.matrix.m31),
                    trim_float(value.matrix.m32),
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    })
}

pub fn compose_svg_transform_list(values: &[SVGTransformValue]) -> Transform2D<f32> {
    values.iter().fold(Transform2D::identity(), |current, value| {
        current.then(&value.matrix)
    })
}

pub fn parse_svg_reference(raw: Option<&str>) -> Option<SVGReferenceValue<'_>> {
    let raw = raw.map(str::trim).filter(|raw| !raw.is_empty())?;
    Some(SVGReferenceValue {
        raw,
        local_reference: normalize_local_reference(raw),
    })
}

pub fn parse_svg_text_anchor(raw: Option<&str>) -> Option<SVGTextAnchor> {
    match raw?.trim() {
        "start" => Some(SVGTextAnchor::Start),
        "middle" => Some(SVGTextAnchor::Middle),
        "end" => Some(SVGTextAnchor::End),
        _ => None,
    }
}

pub fn parse_svg_length_adjust(raw: Option<&str>) -> Option<SVGLengthAdjustValue> {
    match raw?.trim() {
        "spacing" => Some(SVGLengthAdjustValue::Spacing),
        "spacingAndGlyphs" => Some(SVGLengthAdjustValue::SpacingAndGlyphs),
        _ => None,
    }
}

pub fn parse_svg_baseline(raw: Option<&str>) -> Option<SVGTextBaselineValue> {
    match raw?.trim() {
        "middle" => Some(SVGTextBaselineValue::Middle),
        "central" => Some(SVGTextBaselineValue::Central),
        "hanging" => Some(SVGTextBaselineValue::Hanging),
        "text-before-edge" => Some(SVGTextBaselineValue::TextBeforeEdge),
        "text-after-edge" => Some(SVGTextBaselineValue::TextAfterEdge),
        _ => None,
    }
}

pub fn parse_svg_coordinate_units(raw: Option<&str>) -> Option<SVGCoordinateUnits> {
    match raw?.trim() {
        "userSpaceOnUse" => Some(SVGCoordinateUnits::UserSpaceOnUse),
        "objectBoundingBox" => Some(SVGCoordinateUnits::ObjectBoundingBox),
        _ => None,
    }
}

pub fn parse_svg_spread_method(raw: Option<&str>) -> Option<SVGGradientSpreadMethod> {
    match raw?.trim() {
        "pad" => Some(SVGGradientSpreadMethod::Pad),
        "reflect" => Some(SVGGradientSpreadMethod::Reflect),
        "repeat" => Some(SVGGradientSpreadMethod::Repeat),
        _ => None,
    }
}

pub fn parse_svg_fill_rule(raw: Option<&str>) -> Option<SVGFillRule> {
    match raw?.trim() {
        "nonzero" => Some(SVGFillRule::NonZero),
        "evenodd" => Some(SVGFillRule::EvenOdd),
        _ => None,
    }
}

pub fn parse_svg_line_cap(raw: Option<&str>) -> Option<SVGLineCap> {
    match raw?.trim() {
        "butt" => Some(SVGLineCap::Butt),
        "round" => Some(SVGLineCap::Round),
        "square" => Some(SVGLineCap::Square),
        _ => None,
    }
}

pub fn parse_svg_line_join(raw: Option<&str>) -> Option<SVGLineJoin> {
    match raw?.trim() {
        "miter" => Some(SVGLineJoin::Miter),
        "round" => Some(SVGLineJoin::Round),
        "bevel" => Some(SVGLineJoin::Bevel),
        _ => None,
    }
}

pub fn parse_svg_paint_order(raw: Option<&str>) -> Option<SVGPaintOrder> {
    let raw = raw?.trim();
    let mut parts = raw.split_whitespace();
    match (parts.next()?, parts.next(), parts.next()) {
        ("normal", None, None) => Some(SVGPaintOrder::Normal),
        ("fill", Some("stroke"), Some("markers")) => Some(SVGPaintOrder::FillStrokeMarkers),
        ("fill", Some("markers"), Some("stroke")) => Some(SVGPaintOrder::FillMarkersStroke),
        ("stroke", Some("fill"), Some("markers")) => Some(SVGPaintOrder::StrokeFillMarkers),
        ("stroke", Some("markers"), Some("fill")) => Some(SVGPaintOrder::StrokeMarkersFill),
        ("markers", Some("fill"), Some("stroke")) => Some(SVGPaintOrder::MarkersFillStroke),
        ("markers", Some("stroke"), Some("fill")) => Some(SVGPaintOrder::MarkersStrokeFill),
        _ => None,
    }
}

pub fn parse_svg_pointer_events(raw: Option<&str>) -> Option<SVGPointerEventsValue> {
    match raw?.trim() {
        "auto" => Some(SVGPointerEventsValue::Auto),
        "none" => Some(SVGPointerEventsValue::None),
        "visiblePainted" => Some(SVGPointerEventsValue::VisiblePainted),
        "visibleFill" => Some(SVGPointerEventsValue::VisibleFill),
        "visibleStroke" => Some(SVGPointerEventsValue::VisibleStroke),
        "visible" => Some(SVGPointerEventsValue::Visible),
        "painted" => Some(SVGPointerEventsValue::Painted),
        "fill" => Some(SVGPointerEventsValue::Fill),
        "stroke" => Some(SVGPointerEventsValue::Stroke),
        "all" => Some(SVGPointerEventsValue::All),
        "bounding-box" => Some(SVGPointerEventsValue::BoundingBox),
        _ => None,
    }
}

pub fn parse_svg_vector_effect(raw: Option<&str>) -> Option<SVGVectorEffect> {
    raw?
        .split_whitespace()
        .find_map(|part| match part {
            "none" => Some(SVGVectorEffect::None),
            "non-scaling-stroke" => Some(SVGVectorEffect::NonScalingStroke),
            _ => None,
        })
}

pub fn parse_svg_unit_interval(raw: Option<&str>) -> Option<f32> {
    let value = raw?.trim().parse::<f32>().ok()?;
    Some(value.clamp(0.0, 1.0))
}

pub fn parse_svg_non_negative_number(raw: Option<&str>) -> Option<f32> {
    let value = raw?.trim().parse::<f32>().ok()?;
    if value.is_finite() && value >= 0.0 {
        Some(value)
    } else {
        None
    }
}

pub fn parse_svg_dash_array(raw: Option<&str>) -> Option<Vec<f32>> {
    let raw = raw?.trim();
    if raw.eq_ignore_ascii_case("none") {
        return Some(Vec::new());
    }
    raw.split(|ch: char| ch == ',' || ch.is_ascii_whitespace())
        .filter(|part| !part.is_empty())
        .map(|part| parse_svg_non_negative_number(Some(part)))
        .collect::<Option<Vec<_>>>()
}

pub fn parse_svg_stop_offset(raw: Option<&str>) -> Option<f32> {
    let raw = raw?.trim();
    if let Some(percent) = raw.strip_suffix('%') {
        return percent
            .parse::<f32>()
            .ok()
            .map(|value| (value / 100.0).clamp(0.0, 1.0));
    }
    raw.parse::<f32>().ok().map(|value| value.clamp(0.0, 1.0))
}

pub fn parse_svg_overflow_hidden(raw: Option<&str>) -> bool {
    raw.is_some_and(|overflow| matches!(overflow.trim(), "hidden" | "scroll" | "auto"))
}

fn parse_svg_length_components(raw: &str) -> Option<(&str, u16)> {
    if let Some(number) = raw.strip_suffix('%') {
        return Some((number, SVG_LENGTHTYPE_PERCENTAGE));
    }
    for (suffix, unit_type) in [
        ("em", SVG_LENGTHTYPE_EMS),
        ("ex", SVG_LENGTHTYPE_EXS),
        ("px", SVG_LENGTHTYPE_PX),
        ("cm", SVG_LENGTHTYPE_CM),
        ("mm", SVG_LENGTHTYPE_MM),
        ("in", SVG_LENGTHTYPE_IN),
        ("pt", SVG_LENGTHTYPE_PT),
        ("pc", SVG_LENGTHTYPE_PC),
    ] {
        if let Some(number) = raw.strip_suffix(suffix) {
            return Some((number, unit_type));
        }
    }
    Some((raw, SVG_LENGTHTYPE_NUMBER))
}

fn parse_svg_transform(name: &str, args: &str) -> Option<SVGTransformValue> {
    let args = split_svg_arguments(args)
        .into_iter()
        .map(parse_finite_f32)
        .collect::<Option<Vec<_>>>()?;
    match name {
        "matrix" if args.len() == 6 => Some(SVGTransformValue {
            transform_type: SVG_TRANSFORM_MATRIX,
            matrix: Transform2D::new(args[0], args[1], args[2], args[3], args[4], args[5]),
            angle: 0.0,
        }),
        "translate" if args.len() == 1 || args.len() == 2 => Some(SVGTransformValue {
            transform_type: SVG_TRANSFORM_TRANSLATE,
            matrix: Transform2D::translation(args[0], args.get(1).copied().unwrap_or(0.0)),
            angle: 0.0,
        }),
        "scale" if args.len() == 1 || args.len() == 2 => Some(SVGTransformValue {
            transform_type: SVG_TRANSFORM_SCALE,
            matrix: Transform2D::scale(args[0], args.get(1).copied().unwrap_or(args[0])),
            angle: 0.0,
        }),
        "rotate" if args.len() == 1 || args.len() == 3 => {
            let angle = args[0];
            let matrix = if args.len() == 3 {
                Transform2D::translation(args[1], args[2])
                    .then_rotate(euclid::Angle::degrees(angle))
                    .then_translate(euclid::vec2(-args[1], -args[2]))
            } else {
                Transform2D::rotation(euclid::Angle::degrees(angle))
            };
            Some(SVGTransformValue {
                transform_type: SVG_TRANSFORM_ROTATE,
                matrix,
                angle,
            })
        }
        "skewX" if args.len() == 1 => Some(SVGTransformValue {
            transform_type: SVG_TRANSFORM_SKEWX,
            matrix: Transform2D::new(1.0, 0.0, args[0].to_radians().tan(), 1.0, 0.0, 0.0),
            angle: args[0],
        }),
        "skewY" if args.len() == 1 => Some(SVGTransformValue {
            transform_type: SVG_TRANSFORM_SKEWY,
            matrix: Transform2D::new(1.0, args[0].to_radians().tan(), 0.0, 1.0, 0.0, 0.0),
            angle: args[0],
        }),
        _ => None,
    }
}

fn split_svg_arguments(raw: &str) -> Vec<&str> {
    raw.split(|ch: char| ch == ',' || ch.is_ascii_whitespace())
        .filter(|part| !part.is_empty())
        .collect()
}

fn normalize_local_reference(raw: &str) -> Option<&str> {
    let raw = raw.trim();
    if let Some(stripped) = raw.strip_prefix('#') {
        return Some(stripped.trim());
    }
    let inner = raw.strip_prefix("url(")?.strip_suffix(')')?.trim();
    if let Some(stripped) = inner.strip_prefix('#') {
        return Some(stripped.trim_matches(|ch| ch == '\'' || ch == '"' || ch == ' '));
    }
    let inner = inner.trim_matches(|ch| ch == '\'' || ch == '"' || ch == ' ');
    inner.strip_prefix('#')
}

fn parse_finite_f32(raw: &str) -> Option<f32> {
    raw.parse::<f32>().ok().filter(|value| value.is_finite())
}

pub fn trim_float(value: f32) -> String {
    let mut text = format!("{value}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text.is_empty() {
        let _ = write!(&mut text, "0");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_svg_references_with_and_without_url_wrapper() {
        let plain = parse_svg_reference(Some("#shape")).expect("plain reference");
        assert_eq!(plain.local_reference, Some("shape"));

        let wrapped = parse_svg_reference(Some("url('#clip')")).expect("wrapped reference");
        assert_eq!(wrapped.local_reference, Some("clip"));

        let bare = parse_svg_reference(Some("grad")).expect("bare reference");
        assert_eq!(bare.local_reference, None);
    }

    #[test]
    fn composes_transform_lists_left_to_right() {
        let list = parse_svg_transform_list(Some("translate(10 0) scale(2)"));
        let matrix = compose_svg_transform_list(&list);
        let point = matrix.transform_point(euclid::point2(1.0, 1.0));
        assert_eq!(point, euclid::point2(22.0, 2.0));
    }

    #[test]
    fn parses_preserve_aspect_ratio_defaults() {
        let default_value = parse_svg_preserve_aspect_ratio(None);
        assert_eq!(default_value.align, SVG_PRESERVEASPECTRATIO_XMIDYMID);
        assert_eq!(default_value.meet_or_slice, SVG_MEETORSLICE_MEET);

        let value = parse_svg_preserve_aspect_ratio(Some("xMinYMin slice"));
        assert_eq!(value.align, SVG_PRESERVEASPECTRATIO_XMINYMIN);
        assert_eq!(value.meet_or_slice, SVG_MEETORSLICE_SLICE);
    }
}
