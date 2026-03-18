use havi_types::fragment_tree::BoxFragment;
use makepad_widgets::*;
use style::properties::ComputedValues;
use style::values::generics::box_::Perspective;
use style::values::generics::transform::{GenericRotate, GenericScale, GenericTranslate};

use crate::transform::compute_css_reference_frame_matrix;

/// Compute the reference-frame world matrix for a box fragment, if any.
///
/// A reference frame is created when a box has a CSS transform or perspective.
/// The returned matrix is `T(anchor) * css_transform * T(-anchor)`, where
/// `anchor` is the box's border-box origin in absolute page coordinates and
/// `css_transform` already has `transform-origin` baked in via `change_basis`.
///
/// This matches the WebRender model: items inside the reference frame keep
/// their absolute page-space coordinates, and the `T(-anchor)` converts them
/// to frame-local coordinates before the CSS transform is applied.
///
/// Pure `perspective` owners (no transform) get an identity matrix — they
/// establish structural isolation only.
pub(crate) fn reference_frame_matrix(
    bf: &BoxFragment,
    current_origin: DVec2,
    flatten_3d: bool,
) -> Option<Mat4f> {
    let style = &bf.base.style;
    let presence = transform_presence(style);

    if !presence.has_any_reference_frame_effect() {
        return None;
    }

    // Pure perspective (no transform): structural isolation with identity matrix.
    if !presence.has_transform && presence.has_perspective {
        return Some(Mat4f::identity());
    }

    let border_rect = bf.border_rect();
    let bw = border_rect.size.width.to_f32_px();
    let bh = border_rect.size.height.to_f32_px();
    let anchor = border_origin_absolute(bf, current_origin);
    let css_matrix = compute_css_reference_frame_matrix(style, bw, bh, flatten_3d)?;
    Some(compose_reference_frame_transform(anchor, css_matrix))
}

pub(crate) fn border_origin_absolute(bf: &BoxFragment, current_origin: DVec2) -> DVec2 {
    let border_rect = bf.border_rect();
    dvec2(
        current_origin.x + border_rect.origin.x.to_f32_px() as f64,
        current_origin.y + border_rect.origin.y.to_f32_px() as f64,
    )
}

/// Compose a CSS transform (with transform-origin baked in) into a world-space
/// matrix anchored at the element's border-box origin.
///
/// Result: `T(anchor) * transform * T(-anchor)`
fn compose_reference_frame_transform(anchor: DVec2, transform: Mat4f) -> Mat4f {
    let t_pos = translation_matrix(anchor.x as f32, anchor.y as f32);
    let t_neg = translation_matrix(-(anchor.x as f32), -(anchor.y as f32));
    Mat4f::mul(&t_pos, &Mat4f::mul(&transform, &t_neg))
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TransformPresence {
    has_transform: bool,
    has_perspective: bool,
}

impl TransformPresence {
    fn has_any_reference_frame_effect(self) -> bool {
        self.has_transform || self.has_perspective
    }
}

fn transform_presence(style: &ComputedValues) -> TransformPresence {
    let box_style = style.get_box();
    TransformPresence {
        has_transform: !box_style.transform.0.is_empty()
            || box_style.scale != GenericScale::None
            || box_style.rotate != GenericRotate::None
            || box_style.translate != GenericTranslate::None,
        has_perspective: !matches!(box_style.perspective, Perspective::None),
    }
}

fn translation_matrix(tx: f32, ty: f32) -> Mat4f {
    Mat4f {
        v: [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            tx, ty, 0.0, 1.0,
        ],
    }
}
