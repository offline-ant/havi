use std::mem;

use havi_types::fragment_tree::{SVGFillRule, SVGPathData, SVGPoint, SVGStrokeStyle};

use super::path::flatten_svg_path;

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

pub(crate) fn point_in_fill(path: &SVGPathData, point: SVGPoint) -> bool {
    let segments = flatten_svg_path(path);
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

pub(crate) fn point_near_stroke(path: &SVGPathData, point: SVGPoint, width: f32) -> bool {
    let tolerance = width * 0.5;
    flatten_svg_path(path)
        .into_iter()
        .any(|(from, to)| distance_to_segment(point, from, to) <= tolerance)
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
    use havi_types::fragment_tree::SVGPathCommand;

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
