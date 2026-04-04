/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use html5ever::{LocalName, ns};

pub(crate) use layout_api::{
    SVGLengthValue, SVGPreserveAspectRatioValue, SVGTransformValue,
    SVG_LENGTHTYPE_NUMBER, SVG_LENGTHTYPE_UNKNOWN, SVG_MEETORSLICE_MEET,
    SVG_PRESERVEASPECTRATIO_XMIDYMID, SVG_TRANSFORM_MATRIX, SVG_TRANSFORM_ROTATE,
    SVG_TRANSFORM_SCALE, SVG_TRANSFORM_SKEWX, SVG_TRANSFORM_SKEWY,
    SVG_TRANSFORM_TRANSLATE, compose_svg_transform_list,
    parse_svg_enumeration, parse_svg_length, parse_svg_length_list, parse_svg_number,
    parse_svg_number_list, parse_svg_preserve_aspect_ratio, parse_svg_transform_list,
    parse_svg_view_box, serialize_svg_enumeration, serialize_svg_length,
    serialize_svg_length_list, serialize_svg_number_list,
    serialize_svg_preserve_aspect_ratio, serialize_svg_transform_list,
};

use style::attr::AttrValue;

use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::str::DOMString;
use crate::dom::element::Element;
use crate::dom::svg::svgelement::SVGElement;
use crate::script_runtime::CanGc;

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
        Some(value) => element.set_attribute_exact_name(attribute, AttrValue::String(DOMString::from(value).into()), can_gc),
        None => {
            element.remove_attribute(&ns!(), attribute, can_gc);
        }
    }
}
