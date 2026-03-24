use layout::fragment_tree::BoxFragment;
use makepad_compositor::{MpBackfaceVisibility, MpTransformStyle};
use makepad_widgets::*;
use style::properties::ComputedValues;
use style::values::generics::box_::Perspective;
use style::values::generics::transform::{GenericRotate, GenericScale, GenericTranslate};

use crate::render_plan::compute_used_transform_style;
use crate::transform::{
    compute_css_descendant_perspective_matrix, compute_css_reference_frame_matrix,
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct ReferenceFrameSemantics {
    pub placement_origin: DVec2,
    pub transform_matrix: Option<Mat4f>,
    pub perspective_matrix: Option<Mat4f>,
    pub transform_style: MpTransformStyle,
    pub flattens_descendants: bool,
    pub backface_visibility: MpBackfaceVisibility,
}

// HAVI resolves semantic reference-frame inputs here. The compositor owns the
// later flat-vs-3D execution decision.
pub(crate) fn reference_frame_semantics(
    bf: &BoxFragment,
    current_origin: DVec2,
) -> Option<ReferenceFrameSemantics> {
    let style = bf.style();
    let presence = transform_presence(&style);

    if !presence.has_any_reference_frame_effect() {
        return None;
    }

    let border_rect = bf.border_rect();
    let bw = border_rect.size.width.to_f32_px();
    let bh = border_rect.size.height.to_f32_px();
    let transform_matrix = compute_css_reference_frame_matrix(&style, bw, bh);
    let perspective_matrix = compute_css_descendant_perspective_matrix(&style, bw, bh)
        .map(|matrix| Mat4f { v: matrix });

    let combined = match (perspective_matrix, transform_matrix) {
        (Some(perspective), Some(transform)) => Some(Mat4f::mul(&perspective, &transform)),
        (Some(perspective), None) => Some(perspective),
        (None, Some(transform)) => Some(transform),
        (None, None) => None,
    };
    combined.map(|matrix| matrix.invert())?;

    let transform_style = compute_used_transform_style(&style);
    Some(ReferenceFrameSemantics {
        placement_origin: current_origin,
        transform_matrix,
        perspective_matrix,
        transform_style,
        flattens_descendants: !matches!(transform_style, MpTransformStyle::Preserve3D),
        backface_visibility: match style.get_box().backface_visibility {
            style::computed_values::backface_visibility::T::Hidden => MpBackfaceVisibility::Hidden,
            style::computed_values::backface_visibility::T::Visible => MpBackfaceVisibility::Visible,
        },
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
