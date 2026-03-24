use makepad_browser_scene::MpGradientStop;
use makepad_widgets::{vec2, Vec2f};
use style::color::AbsoluteColor;

use crate::color::resolve_color;

pub(super) fn length_percentage_stops(
    items: &[style::values::generics::image::GradientItem<
        style::values::computed::Color,
        style::values::computed::LengthPercentage,
    >],
    gradient_length: f32,
    current_abs: &AbsoluteColor,
) -> Vec<MpGradientStop> {
    let mut stops = Vec::new();
    for item in items {
        match item {
            style::values::generics::image::GradientItem::SimpleColorStop(color) => {
                stops.push(MpGradientStop {
                    color: resolve_color(color, current_abs),
                    offset: -1.0,
                });
            }
            style::values::generics::image::GradientItem::ComplexColorStop { color, position } => {
                stops.push(MpGradientStop {
                    color: resolve_color(color, current_abs),
                    offset: position
                        .to_used_value(app_units::Au::from_f32_px(gradient_length.max(0.001)))
                        .to_f32_px()
                        / gradient_length.max(0.001),
                });
            }
            style::values::generics::image::GradientItem::InterpolationHint(_) => {}
        }
    }
    normalize_gradient_stops(stops)
}

pub(super) fn angle_percentage_stops(
    items: &[style::values::generics::image::GradientItem<
        style::values::computed::Color,
        style::values::computed::AngleOrPercentage,
    >],
    current_abs: &AbsoluteColor,
) -> Vec<MpGradientStop> {
    let mut stops = Vec::new();
    for item in items {
        match item {
            style::values::generics::image::GradientItem::SimpleColorStop(color) => {
                stops.push(MpGradientStop {
                    color: resolve_color(color, current_abs),
                    offset: -1.0,
                });
            }
            style::values::generics::image::GradientItem::ComplexColorStop { color, position } => {
                let offset = match position {
                    style::values::computed::AngleOrPercentage::Percentage(p) => p.0,
                    style::values::computed::AngleOrPercentage::Angle(angle) => angle.degrees() / 360.0,
                };
                stops.push(MpGradientStop {
                    color: resolve_color(color, current_abs),
                    offset,
                });
            }
            style::values::generics::image::GradientItem::InterpolationHint(_) => {}
        }
    }
    normalize_gradient_stops(stops)
}

fn normalize_gradient_stops(mut stops: Vec<MpGradientStop>) -> Vec<MpGradientStop> {
    if stops.is_empty() {
        return stops;
    }
    if stops[0].offset < 0.0 {
        stops[0].offset = 0.0;
    }
    let last = stops.len() - 1;
    if stops[last].offset < 0.0 {
        stops[last].offset = 1.0;
    }
    let mut index = 0;
    while index < stops.len() {
        if stops[index].offset < 0.0 {
            let start = index - 1;
            let mut end = index + 1;
            while end < stops.len() && stops[end].offset < 0.0 {
                end += 1;
            }
            let range_start = stops[start].offset;
            let range_end = stops[end].offset;
            let count = end - start;
            for (current, stop) in stops
                .iter_mut()
                .enumerate()
                .take(end)
                .skip(start + 1)
            {
                stop.offset = range_start
                    + (range_end - range_start) * ((current - start) as f32) / (count as f32);
            }
            index = end + 1;
        } else {
            index += 1;
        }
    }
    stops
}

pub(super) fn radial_shape(
    shape: &style::values::computed::image::EndingShape,
    width: f32,
    height: f32,
    center_x: f32,
    center_y: f32,
) -> Vec2f {
    use style::values::generics::image::{Circle, Ellipse};

    match shape {
        style::values::computed::image::EndingShape::Circle(circle) => match circle {
            Circle::Radius(radius) => vec2(radius.px(), radius.px()),
            Circle::Extent(extent) => {
                let radius = match extent {
                    style::values::generics::image::ShapeExtent::ClosestSide => center_x.min(center_y).min(width - center_x).min(height - center_y),
                    style::values::generics::image::ShapeExtent::FarthestSide => center_x.max(center_y).max(width - center_x).max(height - center_y),
                    style::values::generics::image::ShapeExtent::ClosestCorner => {
                        let dx = center_x.min(width - center_x);
                        let dy = center_y.min(height - center_y);
                        (dx * dx + dy * dy).sqrt()
                    }
                    style::values::generics::image::ShapeExtent::FarthestCorner
                    | style::values::generics::image::ShapeExtent::Contain
                    | style::values::generics::image::ShapeExtent::Cover => {
                        let dx = center_x.max(width - center_x);
                        let dy = center_y.max(height - center_y);
                        (dx * dx + dy * dy).sqrt()
                    }
                };
                vec2(radius, radius)
            }
        },
        style::values::computed::image::EndingShape::Ellipse(ellipse) => match ellipse {
            Ellipse::Radii(rx, ry) => vec2(
                rx.to_used_value(app_units::Au::from_f32_px(width)).to_f32_px(),
                ry.to_used_value(app_units::Au::from_f32_px(height)).to_f32_px(),
            ),
            Ellipse::Extent(extent) => {
                let dxc = center_x.min(width - center_x);
                let dyc = center_y.min(height - center_y);
                let dxf = center_x.max(width - center_x);
                let dyf = center_y.max(height - center_y);
                match extent {
                    style::values::generics::image::ShapeExtent::ClosestSide => vec2(dxc, dyc),
                    style::values::generics::image::ShapeExtent::FarthestSide => vec2(dxf, dyf),
                    style::values::generics::image::ShapeExtent::ClosestCorner
                    | style::values::generics::image::ShapeExtent::Contain => {
                        let diagonal = (dxc * dxc + dyc * dyc).sqrt();
                        vec2(
                            if dxc < 0.001 { 0.0 } else { dxc * diagonal / dxc.max(0.001) },
                            if dyc < 0.001 { 0.0 } else { dyc * diagonal / dyc.max(0.001) },
                        )
                    }
                    style::values::generics::image::ShapeExtent::FarthestCorner
                    | style::values::generics::image::ShapeExtent::Cover => {
                        let diagonal = (dxf * dxf + dyf * dyf).sqrt();
                        vec2(
                            if dxf < 0.001 { 0.0 } else { dxf * diagonal / dxf.max(0.001) },
                            if dyf < 0.001 { 0.0 } else { dyf * diagonal / dyf.max(0.001) },
                        )
                    }
                }
            }
        },
    }
}
