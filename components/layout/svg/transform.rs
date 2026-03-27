use havi_types::fragment_tree::{SVGPoint, SVGRect, SVGTransform};

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
    preserve_aspect_ratio: Option<svgtypes::AspectRatio>,
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

    let preserve = preserve_aspect_ratio.unwrap_or_default();
    let (scale_x, scale_y, extra_x, extra_y) = if preserve.align == svgtypes::Align::None {
        (
            viewport_width / view_box_width,
            viewport_height / view_box_height,
            0.0,
            0.0,
        )
    } else {
        let uniform_scale = if preserve.slice {
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

pub fn parse_svg_transform(raw: Option<&str>) -> SVGTransform {
    let Some(raw) = raw.map(str::trim).filter(|raw| !raw.is_empty()) else {
        return SVGTransform::identity();
    };

    let mut transform = SVGTransform::identity();
    for token in svgtypes::TransformListParser::from(raw) {
        let Ok(token) = token else {
            return SVGTransform::identity();
        };
        transform = then_svg_transform(transform, transform_token_to_matrix(token));
    }
    transform
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

fn transform_token_to_matrix(token: svgtypes::TransformListToken) -> SVGTransform {
    match token {
        svgtypes::TransformListToken::Matrix { a, b, c, d, e, f } => {
            SVGTransform::new(a as f32, b as f32, c as f32, d as f32, e as f32, f as f32)
        }
        svgtypes::TransformListToken::Translate { tx, ty } => {
            translate_svg_transform(tx as f32, ty as f32)
        }
        svgtypes::TransformListToken::Scale { sx, sy } => {
            SVGTransform::new(sx as f32, 0.0, 0.0, sy as f32, 0.0, 0.0)
        }
        svgtypes::TransformListToken::Rotate { angle } => {
            let radians = (angle as f32).to_radians();
            let sin = radians.sin();
            let cos = radians.cos();
            SVGTransform::new(cos, sin, -sin, cos, 0.0, 0.0)
        }
        svgtypes::TransformListToken::SkewX { angle } => {
            SVGTransform::new(1.0, 0.0, (angle as f32).to_radians().tan(), 1.0, 0.0, 0.0)
        }
        svgtypes::TransformListToken::SkewY { angle } => {
            SVGTransform::new(1.0, (angle as f32).to_radians().tan(), 0.0, 1.0, 0.0, 0.0)
        }
    }
}

fn alignment_factors(align: svgtypes::Align) -> (f32, f32) {
    use svgtypes::Align;

    match align {
        Align::None | Align::XMinYMin => (0.0, 0.0),
        Align::XMidYMin => (0.5, 0.0),
        Align::XMaxYMin => (1.0, 0.0),
        Align::XMinYMid => (0.0, 0.5),
        Align::XMidYMid => (0.5, 0.5),
        Align::XMaxYMid => (1.0, 0.5),
        Align::XMinYMax => (0.0, 1.0),
        Align::XMidYMax => (0.5, 1.0),
        Align::XMaxYMax => (1.0, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_list_applies_left_to_right() {
        let transform = parse_svg_transform(Some("translate(10 0) scale(2)"));
        let point = transform_svg_point(transform, SVGPoint::new(1.0, 1.0));
        assert_eq!(point, SVGPoint::new(22.0, 2.0));
    }
}
