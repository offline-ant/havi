use std::str::FromStr;

use havi_types::fragment_tree::{
    SVGFillRule, SVGLineCap, SVGLineJoin, SVGPaint, SVGPathCommand, SVGPathData, SVGPoint,
    SVGRect,
};

use super::dom::SVGGeometryDataOwned;
use super::style::SVGResolvedStroke;

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
    geometry: &SVGGeometryDataOwned,
    fill_rule: SVGFillRule,
) -> SVGNormalizedPath {
    match geometry {
        SVGGeometryDataOwned::Path { d } => normalize_svg_path_data(d.as_deref(), fill_rule),
        SVGGeometryDataOwned::Rect {
            x,
            y,
            width,
            height,
            ..
        } => normalize_rect(
            parse_svg_length(x.as_deref()).unwrap_or(0.0),
            parse_svg_length(y.as_deref()).unwrap_or(0.0),
            parse_svg_length(width.as_deref()).unwrap_or(0.0),
            parse_svg_length(height.as_deref()).unwrap_or(0.0),
            fill_rule,
        ),
        SVGGeometryDataOwned::Circle { cx, cy, r } => normalize_ellipse(
            parse_svg_length(cx.as_deref()).unwrap_or(0.0),
            parse_svg_length(cy.as_deref()).unwrap_or(0.0),
            parse_svg_length(r.as_deref()).unwrap_or(0.0),
            parse_svg_length(r.as_deref()).unwrap_or(0.0),
            fill_rule,
        ),
        SVGGeometryDataOwned::Ellipse { cx, cy, rx, ry } => normalize_ellipse(
            parse_svg_length(cx.as_deref()).unwrap_or(0.0),
            parse_svg_length(cy.as_deref()).unwrap_or(0.0),
            parse_svg_length(rx.as_deref()).unwrap_or(0.0),
            parse_svg_length(ry.as_deref()).unwrap_or(0.0),
            fill_rule,
        ),
        SVGGeometryDataOwned::Line { x1, y1, x2, y2 } => SVGNormalizedPath {
            fill_rule,
            commands: vec![
                SVGPathCommand::MoveTo(point(
                    parse_svg_length(x1.as_deref()).unwrap_or(0.0),
                    parse_svg_length(y1.as_deref()).unwrap_or(0.0),
                )),
                SVGPathCommand::LineTo(point(
                    parse_svg_length(x2.as_deref()).unwrap_or(0.0),
                    parse_svg_length(y2.as_deref()).unwrap_or(0.0),
                )),
            ],
        },
        SVGGeometryDataOwned::Polyline { points } => normalize_points(points.as_deref(), fill_rule, false),
        SVGGeometryDataOwned::Polygon { points } => normalize_points(points.as_deref(), fill_rule, true),
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

fn parse_svg_length(raw: Option<&str>) -> Option<f32> {
    let raw = raw?;
    let length = svgtypes::Length::from_str(raw).ok()?;
    let px = match length.unit {
        svgtypes::LengthUnit::None | svgtypes::LengthUnit::Px => length.number,
        svgtypes::LengthUnit::In => length.number * 96.0,
        svgtypes::LengthUnit::Cm => length.number * (96.0 / 2.54),
        svgtypes::LengthUnit::Mm => length.number * (96.0 / 25.4),
        svgtypes::LengthUnit::Pt => length.number * (96.0 / 72.0),
        svgtypes::LengthUnit::Pc => length.number * 16.0,
        svgtypes::LengthUnit::Percent | svgtypes::LengthUnit::Em | svgtypes::LengthUnit::Ex => {
            return None;
        }
    };
    px.is_finite().then_some(px as f32)
}

fn point(x: f32, y: f32) -> SVGPoint {
    SVGPoint::new(x, y)
}
