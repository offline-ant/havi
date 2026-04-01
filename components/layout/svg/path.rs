use havi_types::fragment_tree::{
    SVGFillRule, SVGLineCap, SVGLineJoin, SVGPaint, SVGPathCommand, SVGPathData, SVGPoint,
    SVGRect,
};

use layout_api::{SVGGeometryData, SVGLengthValue, resolve_svg_length_to_user_units};
use super::style::SVGResolvedStroke;
use super::transform::transform_svg_point;

#[derive(Clone, Debug)]
pub struct SVGStrokeStyleSpec {
    pub paint: SVGPaint,
    pub width: f32,
    pub line_cap: SVGLineCap,
    pub line_join: SVGLineJoin,
    pub miter_limit: f32,
    pub non_scaling: bool,
}

impl Default for SVGStrokeStyleSpec {
    fn default() -> Self {
        Self {
            paint: SVGPaint::None,
            width: 1.0,
            line_cap: SVGLineCap::Butt,
            line_join: SVGLineJoin::Miter,
            miter_limit: 4.0,
            non_scaling: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SVGNormalizedPath {
    pub fill_rule: SVGFillRule,
    pub commands: Vec<SVGPathCommand>,
}

impl Default for SVGNormalizedPath {
    fn default() -> Self {
        Self {
            fill_rule: SVGFillRule::NonZero,
            commands: Vec::new(),
        }
    }
}

impl From<SVGNormalizedPath> for SVGPathData {
    fn from(path: SVGNormalizedPath) -> Self {
        Self {
            fill_rule: path.fill_rule,
            commands: path.commands,
        }
    }
}

pub fn normalize_svg_geometry(
    geometry: &SVGGeometryData<'_>,
    fill_rule: SVGFillRule,
) -> SVGNormalizedPath {
    match geometry {
        SVGGeometryData::Path { d } => normalize_svg_path_data(*d, fill_rule),
        SVGGeometryData::Rect {
            x,
            y,
            width,
            height,
            ..
        } => normalize_rect(
            resolve_length(*x).unwrap_or(0.0),
            resolve_length(*y).unwrap_or(0.0),
            resolve_length(*width).unwrap_or(0.0),
            resolve_length(*height).unwrap_or(0.0),
            fill_rule,
        ),
        SVGGeometryData::Circle { cx, cy, r } => normalize_ellipse(
            resolve_length(*cx).unwrap_or(0.0),
            resolve_length(*cy).unwrap_or(0.0),
            resolve_length(*r).unwrap_or(0.0),
            resolve_length(*r).unwrap_or(0.0),
            fill_rule,
        ),
        SVGGeometryData::Ellipse { cx, cy, rx, ry } => normalize_ellipse(
            resolve_length(*cx).unwrap_or(0.0),
            resolve_length(*cy).unwrap_or(0.0),
            resolve_length(*rx).unwrap_or(0.0),
            resolve_length(*ry).unwrap_or(0.0),
            fill_rule,
        ),
        SVGGeometryData::Line { x1, y1, x2, y2 } => SVGNormalizedPath {
            fill_rule,
            commands: vec![
                SVGPathCommand::MoveTo(point(
                    resolve_length(*x1).unwrap_or(0.0),
                    resolve_length(*y1).unwrap_or(0.0),
                )),
                SVGPathCommand::LineTo(point(
                    resolve_length(*x2).unwrap_or(0.0),
                    resolve_length(*y2).unwrap_or(0.0),
                )),
            ],
        },
        SVGGeometryData::Polyline { points } => normalize_points(*points, fill_rule, false),
        SVGGeometryData::Polygon { points } => normalize_points(*points, fill_rule, true),
    }
}

pub fn path_bounds(path: &SVGPathData) -> Option<SVGRect> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut saw_point = false;

    let mut update = |point: SVGPoint| {
        saw_point = true;
        min_x = min_x.min(point.x);
        min_y = min_y.min(point.y);
        max_x = max_x.max(point.x);
        max_y = max_y.max(point.y);
    };

    for command in &path.commands {
        match command {
            SVGPathCommand::MoveTo(point) | SVGPathCommand::LineTo(point) => update(*point),
            SVGPathCommand::QuadTo { ctrl, to } => {
                update(*ctrl);
                update(*to);
            }
            SVGPathCommand::CubicTo { ctrl1, ctrl2, to } => {
                update(*ctrl1);
                update(*ctrl2);
                update(*to);
            }
            SVGPathCommand::Close => {}
        }
    }

    saw_point.then(|| SVGRect::new(euclid::point2(min_x, min_y), euclid::size2(max_x - min_x, max_y - min_y)))
}

pub fn decorated_bounds(
    path: &SVGPathData,
    stroke: Option<&SVGResolvedStroke>,
) -> Option<SVGRect> {
    let mut bounds = path_bounds(path)?;
    if let Some(stroke) = stroke {
        let inflate = (stroke.width.max(0.0) * 0.5).max(0.0);
        bounds.origin.x -= inflate;
        bounds.origin.y -= inflate;
        bounds.size.width += inflate * 2.0;
        bounds.size.height += inflate * 2.0;
    }
    Some(bounds)
}

fn normalize_svg_path_data(raw: Option<&str>, fill_rule: SVGFillRule) -> SVGNormalizedPath {
    let mut commands = Vec::new();
    let Some(raw) = raw else {
        return SVGNormalizedPath { fill_rule, commands };
    };
    for segment in svgtypes::SimplifyingPathParser::from(raw) {
        let Ok(segment) = segment else {
            return SVGNormalizedPath { fill_rule, commands: Vec::new() };
        };
        match segment {
            svgtypes::SimplePathSegment::MoveTo { x, y } => commands.push(SVGPathCommand::MoveTo(point(x as f32, y as f32))),
            svgtypes::SimplePathSegment::LineTo { x, y } => commands.push(SVGPathCommand::LineTo(point(x as f32, y as f32))),
            svgtypes::SimplePathSegment::CurveTo { x1, y1, x2, y2, x, y } => commands.push(
                SVGPathCommand::CubicTo {
                    ctrl1: point(x1 as f32, y1 as f32),
                    ctrl2: point(x2 as f32, y2 as f32),
                    to: point(x as f32, y as f32),
                },
            ),
            svgtypes::SimplePathSegment::Quadratic { x1, y1, x, y } => commands.push(
                SVGPathCommand::QuadTo {
                    ctrl: point(x1 as f32, y1 as f32),
                    to: point(x as f32, y as f32),
                },
            ),
            svgtypes::SimplePathSegment::ClosePath => commands.push(SVGPathCommand::Close),
        }
    }
    SVGNormalizedPath { fill_rule, commands }
}

fn normalize_rect(x: f32, y: f32, width: f32, height: f32, fill_rule: SVGFillRule) -> SVGNormalizedPath {
    if width <= 0.0 || height <= 0.0 {
        return SVGNormalizedPath::default();
    }
    SVGNormalizedPath {
        fill_rule,
        commands: vec![
            SVGPathCommand::MoveTo(point(x, y)),
            SVGPathCommand::LineTo(point(x + width, y)),
            SVGPathCommand::LineTo(point(x + width, y + height)),
            SVGPathCommand::LineTo(point(x, y + height)),
            SVGPathCommand::Close,
        ],
    }
}

fn normalize_ellipse(cx: f32, cy: f32, rx: f32, ry: f32, fill_rule: SVGFillRule) -> SVGNormalizedPath {
    if rx <= 0.0 || ry <= 0.0 {
        return SVGNormalizedPath::default();
    }
    let kappa = 0.552_284_8_f32;
    let ox = rx * kappa;
    let oy = ry * kappa;
    SVGNormalizedPath {
        fill_rule,
        commands: vec![
            SVGPathCommand::MoveTo(point(cx + rx, cy)),
            SVGPathCommand::CubicTo {
                ctrl1: point(cx + rx, cy + oy),
                ctrl2: point(cx + ox, cy + ry),
                to: point(cx, cy + ry),
            },
            SVGPathCommand::CubicTo {
                ctrl1: point(cx - ox, cy + ry),
                ctrl2: point(cx - rx, cy + oy),
                to: point(cx - rx, cy),
            },
            SVGPathCommand::CubicTo {
                ctrl1: point(cx - rx, cy - oy),
                ctrl2: point(cx - ox, cy - ry),
                to: point(cx, cy - ry),
            },
            SVGPathCommand::CubicTo {
                ctrl1: point(cx + ox, cy - ry),
                ctrl2: point(cx + rx, cy - oy),
                to: point(cx + rx, cy),
            },
            SVGPathCommand::Close,
        ],
    }
}

fn normalize_points(raw: Option<&str>, fill_rule: SVGFillRule, closed: bool) -> SVGNormalizedPath {
    let Some(raw) = raw else {
        return SVGNormalizedPath::default();
    };
    let mut coords = raw
        .split(|ch: char| ch == ',' || ch.is_ascii_whitespace())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<f32>().ok());
    let Some(first_x) = coords.next() else {
        return SVGNormalizedPath::default();
    };
    let Some(first_y) = coords.next() else {
        return SVGNormalizedPath::default();
    };
    let mut commands = vec![SVGPathCommand::MoveTo(point(first_x, first_y))];
    loop {
        let Some(x) = coords.next() else {
            break;
        };
        let Some(y) = coords.next() else {
            break;
        };
        commands.push(SVGPathCommand::LineTo(point(x, y)));
    }
    if closed {
        commands.push(SVGPathCommand::Close);
    }
    SVGNormalizedPath { fill_rule, commands }
}

pub fn resolve_length(length: Option<SVGLengthValue>) -> Option<f32> {
    length.and_then(resolve_svg_length_to_user_units)
}

pub fn transform_svg_path_data(path: &SVGPathData, transform: havi_types::fragment_tree::SVGTransform) -> SVGPathData {
    let mut transformed = path.clone();
    for command in &mut transformed.commands {
        match command {
            SVGPathCommand::MoveTo(point) | SVGPathCommand::LineTo(point) => {
                *point = transform_svg_point(transform, *point);
            }
            SVGPathCommand::QuadTo { ctrl, to } => {
                *ctrl = transform_svg_point(transform, *ctrl);
                *to = transform_svg_point(transform, *to);
            }
            SVGPathCommand::CubicTo { ctrl1, ctrl2, to } => {
                *ctrl1 = transform_svg_point(transform, *ctrl1);
                *ctrl2 = transform_svg_point(transform, *ctrl2);
                *to = transform_svg_point(transform, *to);
            }
            SVGPathCommand::Close => {}
        }
    }
    transformed
}

pub fn flatten_svg_path(path: &SVGPathData) -> Vec<(SVGPoint, SVGPoint)> {
    let mut segments = Vec::new();
    let mut current = SVGPoint::new(0.0, 0.0);
    let mut subpath_start = SVGPoint::new(0.0, 0.0);
    let mut has_current = false;

    for command in &path.commands {
        match command {
            SVGPathCommand::MoveTo(point) => {
                current = *point;
                subpath_start = *point;
                has_current = true;
            }
            SVGPathCommand::LineTo(point) if has_current => {
                segments.push((current, *point));
                current = *point;
            }
            SVGPathCommand::QuadTo { ctrl, to } if has_current => {
                let mut from = current;
                for step in 1..=12 {
                    let t = step as f32 / 12.0;
                    let point = quadratic_bezier(current, *ctrl, *to, t);
                    segments.push((from, point));
                    from = point;
                }
                current = *to;
            }
            SVGPathCommand::CubicTo { ctrl1, ctrl2, to } if has_current => {
                let mut from = current;
                for step in 1..=16 {
                    let t = step as f32 / 16.0;
                    let point = cubic_bezier(current, *ctrl1, *ctrl2, *to, t);
                    segments.push((from, point));
                    from = point;
                }
                current = *to;
            }
            SVGPathCommand::Close if has_current => {
                segments.push((current, subpath_start));
                current = subpath_start;
            }
            _ => {}
        }
    }

    segments
}

pub fn svg_path_total_length(path: &SVGPathData) -> f32 {
    flatten_svg_path(path)
        .into_iter()
        .map(|(from, to)| segment_length(from, to))
        .sum()
}

pub fn svg_path_point_and_tangent_at_length(
    path: &SVGPathData,
    length: f32,
) -> Option<(SVGPoint, SVGPoint)> {
    let segments = flatten_svg_path(path);
    let total_length: f32 = segments.iter().map(|(from, to)| segment_length(*from, *to)).sum();
    if segments.is_empty() || !total_length.is_finite() || total_length <= 0.0 {
        return None;
    }

    let clamped = length.clamp(0.0, total_length);
    let mut remaining = clamped;
    for (from, to) in &segments {
        let segment_length = segment_length(*from, *to);
        if segment_length <= f32::EPSILON {
            continue;
        }
        if remaining <= segment_length {
            let t = remaining / segment_length;
            let point = SVGPoint::new(
                from.x + (to.x - from.x) * t,
                from.y + (to.y - from.y) * t,
            );
            let tangent = normalize_vector(SVGPoint::new(to.x - from.x, to.y - from.y));
            return Some((point, tangent));
        }
        remaining -= segment_length;
    }

    let (from, to) = *segments.last()?;
    Some((to, normalize_vector(SVGPoint::new(to.x - from.x, to.y - from.y))))
}

fn quadratic_bezier(from: SVGPoint, ctrl: SVGPoint, to: SVGPoint, t: f32) -> SVGPoint {
    let inv = 1.0 - t;
    SVGPoint::new(
        inv * inv * from.x + 2.0 * inv * t * ctrl.x + t * t * to.x,
        inv * inv * from.y + 2.0 * inv * t * ctrl.y + t * t * to.y,
    )
}

fn cubic_bezier(from: SVGPoint, ctrl1: SVGPoint, ctrl2: SVGPoint, to: SVGPoint, t: f32) -> SVGPoint {
    let inv = 1.0 - t;
    SVGPoint::new(
        inv.powi(3) * from.x +
            3.0 * inv.powi(2) * t * ctrl1.x +
            3.0 * inv * t.powi(2) * ctrl2.x +
            t.powi(3) * to.x,
        inv.powi(3) * from.y +
            3.0 * inv.powi(2) * t * ctrl1.y +
            3.0 * inv * t.powi(2) * ctrl2.y +
            t.powi(3) * to.y,
    )
}

fn segment_length(from: SVGPoint, to: SVGPoint) -> f32 {
    ((to.x - from.x).powi(2) + (to.y - from.y).powi(2)).sqrt()
}

fn normalize_vector(vector: SVGPoint) -> SVGPoint {
    let length = (vector.x * vector.x + vector.y * vector.y).sqrt();
    if length <= f32::EPSILON {
        SVGPoint::new(1.0, 0.0)
    } else {
        SVGPoint::new(vector.x / length, vector.y / length)
    }
}

fn point(x: f32, y: f32) -> SVGPoint {
    SVGPoint::new(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_length_and_sampling_follow_flattened_segments() {
        let path = SVGPathData {
            fill_rule: SVGFillRule::NonZero,
            commands: vec![
                SVGPathCommand::MoveTo(SVGPoint::new(0.0, 0.0)),
                SVGPathCommand::LineTo(SVGPoint::new(0.0, 10.0)),
                SVGPathCommand::LineTo(SVGPoint::new(10.0, 10.0)),
            ],
        };

        assert_eq!(svg_path_total_length(&path), 20.0);

        let (point, tangent) = svg_path_point_and_tangent_at_length(&path, 5.0).unwrap();
        assert_eq!(point, SVGPoint::new(0.0, 5.0));
        assert_eq!(tangent, SVGPoint::new(0.0, 1.0));

        let (point, tangent) = svg_path_point_and_tangent_at_length(&path, 15.0).unwrap();
        assert_eq!(point, SVGPoint::new(5.0, 10.0));
        assert_eq!(tangent, SVGPoint::new(1.0, 0.0));
    }
}
