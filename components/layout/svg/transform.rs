use havi_types::fragment_tree::{SVGRect, SVGTransform};

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
