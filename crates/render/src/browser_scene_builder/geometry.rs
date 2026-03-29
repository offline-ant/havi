use havi_types::fragment_tree as published;
use makepad_widgets::{dvec2, DVec2, Rect};

pub(super) fn fragment_local_bounds(
    generation: &published::FragmentArenaGeneration,
    fragment_id: published::FragmentId,
    containing_block_origin: DVec2,
) -> Rect {
    let rect = match generation.kind(fragment_id) {
        published::FragmentKind::Box(bf) | published::FragmentKind::Float(bf) => {
            physical_rect_to_rect(bf.border_rect())
        }
        published::FragmentKind::Text(text) => physical_rect_to_rect(text.base.rect),
        published::FragmentKind::Image(image) => physical_rect_to_rect(image.base.rect),
        published::FragmentKind::IFrame(iframe) => physical_rect_to_rect(iframe.base.rect),
        published::FragmentKind::SVGViewport(svg) => physical_rect_to_rect(svg.base.rect),
        published::FragmentKind::SVGContainer(svg) => physical_rect_to_rect(svg.base.rect),
        published::FragmentKind::SVGLeaf(svg) => physical_rect_to_rect(svg.base.rect),
        published::FragmentKind::Positioning(_) => Rect {
            pos: dvec2(0.0, 0.0),
            size: dvec2(0.0, 0.0),
        },
    };
    Rect {
        pos: containing_block_origin + rect.pos,
        size: rect.size,
    }
}

pub(super) fn box_content_insets(style: &style::properties::ComputedValues) -> (f64, f64, f64, f64) {
    use style::values::specified::border::BorderStyle;

    let border = style.get_border();
    let border_width = |style: BorderStyle, width: style::values::computed::BorderSideWidth| -> f64 {
        if matches!(style, BorderStyle::None | BorderStyle::Hidden) {
            0.0
        } else {
            width.0.to_f32_px().max(0.0) as f64
        }
    };
    let padding = style.get_padding();
    (
        border_width(border.clone_border_left_style(), border.clone_border_left_width())
            + padding.padding_left.0.to_length().map_or(0.0, |l| l.px()) as f64,
        border_width(border.clone_border_top_style(), border.clone_border_top_width())
            + padding.padding_top.0.to_length().map_or(0.0, |l| l.px()) as f64,
        border_width(border.clone_border_right_style(), border.clone_border_right_width())
            + padding.padding_right.0.to_length().map_or(0.0, |l| l.px()) as f64,
        border_width(border.clone_border_bottom_style(), border.clone_border_bottom_width())
            + padding.padding_bottom.0.to_length().map_or(0.0, |l| l.px()) as f64,
    )
}

pub(super) fn outset_rect(rect: Rect, insets: (f64, f64, f64, f64)) -> Rect {
    let (left, top, right, bottom) = insets;
    Rect {
        pos: rect.pos - dvec2(left, top),
        size: dvec2(rect.size.x + left + right, rect.size.y + top + bottom),
    }
}

pub(super) fn physical_rect_to_rect(rect: havi_types::PhysicalRect<app_units::Au>) -> Rect {
    Rect {
        pos: dvec2(rect.origin.x.to_f32_px() as f64, rect.origin.y.to_f32_px() as f64),
        size: dvec2(rect.size.width.to_f32_px() as f64, rect.size.height.to_f32_px() as f64),
    }
}

pub(super) fn map_box_rect_to_spatial_space(
    rect: Rect,
    containing_block_origin: DVec2,
    border_box_origin: DVec2,
    uses_box_local_basis: bool,
) -> Rect {
    Rect {
        pos: if uses_box_local_basis {
            rect.pos - border_box_origin
        } else {
            containing_block_origin + rect.pos
        },
        size: rect.size,
    }
}
