/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::fmt::Write;

use euclid::default::Transform2D;
use html5ever::{LocalName, ns};

use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::str::DOMString;
use crate::dom::element::Element;
use crate::dom::svg::svgelement::SVGElement;
use crate::script_runtime::CanGc;

pub(crate) const SVG_LENGTHTYPE_UNKNOWN: u16 = 0;
pub(crate) const SVG_LENGTHTYPE_NUMBER: u16 = 1;
pub(crate) const SVG_LENGTHTYPE_PERCENTAGE: u16 = 2;
pub(crate) const SVG_LENGTHTYPE_EMS: u16 = 3;
pub(crate) const SVG_LENGTHTYPE_EXS: u16 = 4;
pub(crate) const SVG_LENGTHTYPE_PX: u16 = 5;
pub(crate) const SVG_LENGTHTYPE_CM: u16 = 6;
pub(crate) const SVG_LENGTHTYPE_MM: u16 = 7;
pub(crate) const SVG_LENGTHTYPE_IN: u16 = 8;
pub(crate) const SVG_LENGTHTYPE_PT: u16 = 9;
pub(crate) const SVG_LENGTHTYPE_PC: u16 = 10;

pub(crate) const SVG_TRANSFORM_UNKNOWN: u16 = 0;
pub(crate) const SVG_TRANSFORM_MATRIX: u16 = 1;
pub(crate) const SVG_TRANSFORM_TRANSLATE: u16 = 2;
pub(crate) const SVG_TRANSFORM_SCALE: u16 = 3;
pub(crate) const SVG_TRANSFORM_ROTATE: u16 = 4;
pub(crate) const SVG_TRANSFORM_SKEWX: u16 = 5;
pub(crate) const SVG_TRANSFORM_SKEWY: u16 = 6;

pub(crate) const SVG_PRESERVEASPECTRATIO_UNKNOWN: u16 = 0;
pub(crate) const SVG_PRESERVEASPECTRATIO_NONE: u16 = 1;
pub(crate) const SVG_PRESERVEASPECTRATIO_XMINYMIN: u16 = 2;
pub(crate) const SVG_PRESERVEASPECTRATIO_XMIDYMIN: u16 = 3;
pub(crate) const SVG_PRESERVEASPECTRATIO_XMAXYMIN: u16 = 4;
pub(crate) const SVG_PRESERVEASPECTRATIO_XMINYMID: u16 = 5;
pub(crate) const SVG_PRESERVEASPECTRATIO_XMIDYMID: u16 = 6;
pub(crate) const SVG_PRESERVEASPECTRATIO_XMAXYMID: u16 = 7;
pub(crate) const SVG_PRESERVEASPECTRATIO_XMINYMAX: u16 = 8;
pub(crate) const SVG_PRESERVEASPECTRATIO_XMIDYMAX: u16 = 9;
pub(crate) const SVG_PRESERVEASPECTRATIO_XMAXYMAX: u16 = 10;

pub(crate) const SVG_MEETORSLICE_UNKNOWN: u16 = 0;
pub(crate) const SVG_MEETORSLICE_MEET: u16 = 1;
pub(crate) const SVG_MEETORSLICE_SLICE: u16 = 2;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SVGLengthValue {
    pub unit_type: u16,
    pub value: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SVGRectValue {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SVGPreserveAspectRatioValue {
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

#[derive(Clone, Copy, Debug)]
pub(crate) struct SVGTransformValue {
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

pub(crate) fn svg_attribute_value(owner: &SVGElement, attribute: &LocalName) -> Option<String> {
    owner
        .upcast::<Element>()
        .get_attribute(&ns!(), attribute)
        .map(|attr| String::from(&**attr.value()))
}

pub(crate) fn set_svg_attribute_value(
    owner: &SVGElement,
    attribute: &LocalName,
    value: Option<String>,
    can_gc: CanGc,
) {
    let element = owner.upcast::<Element>();
    match value {
        Some(value) => element.set_string_attribute(attribute, DOMString::from(value), can_gc),
        None => {
            element.remove_attribute(&ns!(), attribute, can_gc);
        },
    }
}

pub(crate) fn parse_svg_length(raw: Option<&str>) -> SVGLengthValue {
    let Some(raw) = raw.map(str::trim).filter(|raw| !raw.is_empty()) else {
        return SVGLengthValue {
            unit_type: SVG_LENGTHTYPE_NUMBER,
            value: 0.0,
        };
    };
    let (number_text, unit_type) = parse_svg_length_components(raw).unwrap_or(("", SVG_LENGTHTYPE_UNKNOWN));
    let Some(value) = parse_finite_f32(number_text) else {
        return SVGLengthValue {
            unit_type: SVG_LENGTHTYPE_UNKNOWN,
            value: 0.0,
        };
    };
    SVGLengthValue { unit_type, value }
}

pub(crate) fn serialize_svg_length(value: SVGLengthValue) -> String {
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

pub(crate) fn parse_svg_length_list(raw: Option<&str>) -> Vec<SVGLengthValue> {
    let Some(raw) = raw.map(str::trim).filter(|raw| !raw.is_empty()) else {
        return Vec::new();
    };
    split_svg_arguments(raw)
        .into_iter()
        .map(|token| parse_svg_length(Some(token)))
        .collect()
}

pub(crate) fn serialize_svg_length_list(values: &[SVGLengthValue]) -> Option<String> {
    (!values.is_empty()).then(|| {
        values
            .iter()
            .map(|value| serialize_svg_length(*value))
            .collect::<Vec<_>>()
            .join(" ")
    })
}

pub(crate) fn parse_svg_number(raw: Option<&str>) -> f32 {
    raw.and_then(|raw| parse_finite_f32(raw.trim()))
        .unwrap_or(0.0)
}

pub(crate) fn serialize_svg_number(value: f32) -> String {
    trim_float(value)
}

pub(crate) fn parse_svg_number_list(raw: Option<&str>) -> Vec<f32> {
    let Some(raw) = raw.map(str::trim).filter(|raw| !raw.is_empty()) else {
        return Vec::new();
    };
    split_svg_arguments(raw)
        .into_iter()
        .filter_map(parse_finite_f32)
        .collect()
}

pub(crate) fn serialize_svg_number_list(values: &[f32]) -> Option<String> {
    (!values.is_empty()).then(|| {
        values
            .iter()
            .map(|value| trim_float(*value))
            .collect::<Vec<_>>()
            .join(" ")
    })
}

pub(crate) fn parse_svg_view_box(raw: Option<&str>) -> SVGRectValue {
    let values = parse_svg_number_list(raw);
    if values.len() != 4 {
        return SVGRectValue::default();
    }
    SVGRectValue {
        x: values[0],
        y: values[1],
        width: values[2],
        height: values[3],
    }
}

pub(crate) fn serialize_svg_view_box(value: SVGRectValue) -> String {
    format!(
        "{} {} {} {}",
        trim_float(value.x),
        trim_float(value.y),
        trim_float(value.width),
        trim_float(value.height)
    )
}

pub(crate) fn parse_svg_preserve_aspect_ratio(raw: Option<&str>) -> SVGPreserveAspectRatioValue {
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
            },
            "slice" => {
                result.meet_or_slice = SVG_MEETORSLICE_SLICE;
                continue;
            },
            _ => result.align,
        };
    }
    result
}

pub(crate) fn serialize_svg_preserve_aspect_ratio(value: SVGPreserveAspectRatioValue) -> String {
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

pub(crate) fn parse_svg_transform_list(raw: Option<&str>) -> Vec<SVGTransformValue> {
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

pub(crate) fn serialize_svg_transform_list(values: &[SVGTransformValue]) -> Option<String> {
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

pub(crate) fn compose_svg_transform_list(values: &[SVGTransformValue]) -> Transform2D<f32> {
    values
        .iter()
        .fold(Transform2D::identity(), |current, value| current.then(&value.matrix))
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
        },
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

fn parse_finite_f32(raw: &str) -> Option<f32> {
    raw.parse::<f32>().ok().filter(|value| value.is_finite())
}

pub(crate) fn trim_float(value: f32) -> String {
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
