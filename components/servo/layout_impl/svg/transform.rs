use havi_types::fragment_tree::{SVGPoint, SVGRect, SVGTransform};
use crate::layout::{
    SVGPreserveAspectRatioValue, SVGTransformValue, compose_svg_transform_list,
    SVG_MEETORSLICE_SLICE, SVG_PRESERVEASPECTRATIO_NONE, SVG_PRESERVEASPECTRATIO_XMAXYMAX,
    SVG_PRESERVEASPECTRATIO_XMAXYMID, SVG_PRESERVEASPECTRATIO_XMAXYMIN,
    SVG_PRESERVEASPECTRATIO_XMIDYMAX, SVG_PRESERVEASPECTRATIO_XMIDYMID,
    SVG_PRESERVEASPECTRATIO_XMIDYMIN, SVG_PRESERVEASPECTRATIO_XMINYMAX,
    SVG_PRESERVEASPECTRATIO_XMINYMID, SVG_PRESERVEASPECTRATIO_XMINYMIN,
};

#[derive(Clone, Debug)]
pub struct SVGCoordinateMapper {
    pub local_to_parent: SVGTransform,
}

impl SVGCoordinateMapper {
    pub fn identity() -> Self {
        Self {
            local_to_parent: SVGTransform::identity(),
        }
    }
}

pub fn compute_view_box_mapper(
    viewport_rect: SVGRect,
    view_box_rect: Option<SVGRect>,
    preserve_aspect_ratio: SVGPreserveAspectRatioValue,
) -> SVGCoordinateMapper {
    let Some(view_box_rect) = view_box_rect else {
        return SVGCoordinateMapper::identity();
    };

    let viewport_width = viewport_rect.size.width.max(0.0);
    let viewport_height = viewport_rect.size.height.max(0.0);
    let view_box_width = view_box_rect.size.width;
    let view_box_height = view_box_rect.size.height;
    if viewport_width <= 0.0 || viewport_height <= 0.0 || view_box_width <= 0.0 || view_box_height <= 0.0 {
        return SVGCoordinateMapper::identity();
    }

    let preserve = preserve_aspect_ratio;
    let (scale_x, scale_y, extra_x, extra_y) = if preserve.align == SVG_PRESERVEASPECTRATIO_NONE {
        (
            viewport_width / view_box_width,
            viewport_height / view_box_height,
            0.0,
            0.0,
        )
    } else {
        let uniform_scale = if preserve.meet_or_slice == SVG_MEETORSLICE_SLICE {
            (viewport_width / view_box_width).max(viewport_height / view_box_height)
        } else {
            (viewport_width / view_box_width).min(viewport_height / view_box_height)
        };
        let fitted_width = view_box_width * uniform_scale;
        let fitted_height = view_box_height * uniform_scale;
        let extra_x = viewport_width - fitted_width;
        let extra_y = viewport_height - fitted_height;
        (uniform_scale, uniform_scale, extra_x, extra_y)
    };

    let (align_x, align_y) = alignment_factors(preserve.align);
    let tx = viewport_rect.origin.x + extra_x * align_x - view_box_rect.origin.x * scale_x;
    let ty = viewport_rect.origin.y + extra_y * align_y - view_box_rect.origin.y * scale_y;

    SVGCoordinateMapper {
        local_to_parent: SVGTransform::new(scale_x, 0.0, 0.0, scale_y, tx, ty),
    }
}

pub fn parse_svg_transform(raw: &[SVGTransformValue]) -> SVGTransform {
    let matrix = compose_svg_transform_list(raw);
    SVGTransform::new(
        matrix.m11,
        matrix.m12,
        matrix.m21,
        matrix.m22,
        matrix.m31,
        matrix.m32,
    )
}

pub fn translate_svg_transform(tx: f32, ty: f32) -> SVGTransform {
    SVGTransform::new(1.0, 0.0, 0.0, 1.0, tx, ty)
}

pub fn then_svg_transform(current: SVGTransform, next: SVGTransform) -> SVGTransform {
    SVGTransform::new(
        next.m11 * current.m11 + next.m21 * current.m12,
        next.m12 * current.m11 + next.m22 * current.m12,
        next.m11 * current.m21 + next.m21 * current.m22,
        next.m12 * current.m21 + next.m22 * current.m22,
        next.m11 * current.m31 + next.m21 * current.m32 + next.m31,
        next.m12 * current.m31 + next.m22 * current.m32 + next.m32,
    )
}

pub fn transform_svg_point(transform: SVGTransform, point: SVGPoint) -> SVGPoint {
    SVGPoint::new(
        transform.m11 * point.x + transform.m21 * point.y + transform.m31,
        transform.m12 * point.x + transform.m22 * point.y + transform.m32,
    )
}

fn alignment_factors(align: u16) -> (f32, f32) {
    match align {
        SVG_PRESERVEASPECTRATIO_NONE | SVG_PRESERVEASPECTRATIO_XMINYMIN => (0.0, 0.0),
        SVG_PRESERVEASPECTRATIO_XMIDYMIN => (0.5, 0.0),
        SVG_PRESERVEASPECTRATIO_XMAXYMIN => (1.0, 0.0),
        SVG_PRESERVEASPECTRATIO_XMINYMID => (0.0, 0.5),
        SVG_PRESERVEASPECTRATIO_XMIDYMID => (0.5, 0.5),
        SVG_PRESERVEASPECTRATIO_XMAXYMID => (1.0, 0.5),
        SVG_PRESERVEASPECTRATIO_XMINYMAX => (0.0, 1.0),
        SVG_PRESERVEASPECTRATIO_XMIDYMAX => (0.5, 1.0),
        SVG_PRESERVEASPECTRATIO_XMAXYMAX => (1.0, 1.0),
        _ => (0.5, 0.5),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_list_applies_left_to_right() {
        let transform = parse_svg_transform(&crate::layout::parse_svg_transform_list(Some("translate(10 0) scale(2)")));
        let point = transform_svg_point(transform, SVGPoint::new(1.0, 1.0));
        assert_eq!(point, SVGPoint::new(22.0, 2.0));
    }
}
