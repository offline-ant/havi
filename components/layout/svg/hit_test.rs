use std::mem;

use havi_types::fragment_tree::{SVGFillRule, SVGPathCommand, SVGPathData, SVGPoint, SVGStrokeStyle};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGHitTargetKind {
    Fill,
    Stroke,
    BoundingBox,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SVGHitTestResult {
    pub hit: bool,
    pub target: SVGHitTargetKind,
}

pub fn hit_test_svg_path(
    path: &SVGPathData,
    stroke: Option<&SVGStrokeStyle>,
    point: SVGPoint,
) -> SVGHitTestResult {
    if let Some(stroke) = stroke {
        if stroke.width > 0.0 && point_near_stroke(path, point, stroke.width) {
            return SVGHitTestResult {
                hit: true,
                target: SVGHitTargetKind::Stroke,
            };
        }
    }

    if point_in_fill(path, point) {
        return SVGHitTestResult {
            hit: true,
            target: SVGHitTargetKind::Fill,
        };
    }

    SVGHitTestResult {
        hit: false,
        target: SVGHitTargetKind::BoundingBox,
    }
}

fn point_in_fill(path: &SVGPathData, point: SVGPoint) -> bool {
    let segments = flatten_path(path);
    match path.fill_rule {
        SVGFillRule::EvenOdd => {
            let mut crossings = 0;
            for (from, to) in &segments {
                if ray_crosses_segment(point, *from, *to) {
                    crossings += 1;
                }
            }
            crossings % 2 == 1
        }
        SVGFillRule::NonZero => {
            let mut winding = 0;
            for (from, to) in &segments {
                if from.y <= point.y {
                    if to.y > point.y && cross(*from, *to, point) > 0.0 {
                        winding += 1;
                    }
                } else if to.y <= point.y && cross(*from, *to, point) < 0.0 {
                    winding -= 1;
                }
            }
            winding != 0
        }
    }
}

fn point_near_stroke(path: &SVGPathData, point: SVGPoint, width: f32) -> bool {
    let tolerance = width * 0.5;
    flatten_path(path)
        .into_iter()
        .any(|(from, to)| distance_to_segment(point, from, to) <= tolerance)
}

fn flatten_path(path: &SVGPathData) -> Vec<(SVGPoint, SVGPoint)> {
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

fn ray_crosses_segment(point: SVGPoint, mut from: SVGPoint, mut to: SVGPoint) -> bool {
    if from.y > to.y {
        mem::swap(&mut from, &mut to);
    }
    if point.y < from.y || point.y >= to.y || point.x >= from.x.max(to.x) {
        return false;
    }
    if point.x < from.x.min(to.x) {
        return true;
    }
    cross(from, to, point) > 0.0
}

fn cross(from: SVGPoint, to: SVGPoint, point: SVGPoint) -> f32 {
    (to.x - from.x) * (point.y - from.y) - (to.y - from.y) * (point.x - from.x)
}

fn distance_to_segment(point: SVGPoint, from: SVGPoint, to: SVGPoint) -> f32 {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq <= f32::EPSILON {
        return ((point.x - from.x).powi(2) + (point.y - from.y).powi(2)).sqrt();
    }
    let t = (((point.x - from.x) * dx + (point.y - from.y) * dy) / len_sq).clamp(0.0, 1.0);
    let proj_x = from.x + t * dx;
    let proj_y = from.y + t * dy;
    ((point.x - proj_x).powi(2) + (point.y - proj_y).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_hit_test_uses_geometry() {
        let path = SVGPathData {
            fill_rule: SVGFillRule::NonZero,
            commands: vec![
                SVGPathCommand::MoveTo(SVGPoint::new(0.0, 0.0)),
                SVGPathCommand::LineTo(SVGPoint::new(10.0, 0.0)),
                SVGPathCommand::LineTo(SVGPoint::new(10.0, 10.0)),
                SVGPathCommand::Close,
            ],
        };
        assert!(hit_test_svg_path(&path, None, SVGPoint::new(5.0, 5.0)).hit);
        assert!(!hit_test_svg_path(&path, None, SVGPoint::new(12.0, 5.0)).hit);
    }
}
