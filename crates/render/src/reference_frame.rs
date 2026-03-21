use havi_fragment_semantics::fragment_tree::BoxFragment;
use makepad_widgets::*;
use style::properties::ComputedValues;
use style::values::generics::box_::Perspective;
use style::values::generics::transform::{GenericRotate, GenericScale, GenericTranslate};

use crate::transform::{
    compute_css_descendant_perspective_matrix, compute_css_reference_frame_matrix,
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct ReferenceFrameSemantics {
    pub has_perspective: bool,
    pub placement_origin: DVec2,
    pub transform_matrix: Option<Mat4f>,
    pub perspective_matrix: Option<Mat4f>,
}

/// Compute the scene-owned reference-frame semantics for a box fragment, if any.
///
/// The scene stores transform and descendant-perspective inputs separately.
/// Execution matrices are derived later from these semantic inputs.
pub(crate) fn reference_frame_semantics(
    bf: &BoxFragment,
    current_origin: DVec2,
    flatten_3d: bool,
) -> Option<ReferenceFrameSemantics> {
    let style = &bf.base.style;
    let presence = transform_presence(style);

    if !presence.has_any_reference_frame_effect() {
        return None;
    }

    let border_rect = bf.border_rect();
    let bw = border_rect.size.width.to_f32_px();
    let bh = border_rect.size.height.to_f32_px();
    let transform_matrix = compute_css_reference_frame_matrix(style, bw, bh, flatten_3d);
    let perspective_matrix = compute_css_descendant_perspective_matrix(style, bw, bh)
        .map(|matrix| Mat4f { v: matrix });

    let combined = match (perspective_matrix, transform_matrix) {
        (Some(perspective), Some(transform)) => Some(Mat4f::mul(&perspective, &transform)),
        (Some(perspective), None) => Some(perspective),
        (None, Some(transform)) => Some(transform),
        (None, None) => None,
    };
    if combined.map(|matrix| matrix.invert()).is_none() {
        return None;
    }

    Some(ReferenceFrameSemantics {
        has_perspective: presence.has_perspective,
        placement_origin: current_origin,
        transform_matrix,
        perspective_matrix,
    })
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


