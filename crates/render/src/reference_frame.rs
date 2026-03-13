use havi_types::fragment_tree::BoxFragment;
use makepad_widgets::*;
use style::properties::ComputedValues;
use style::values::generics::box_::Perspective;
use style::values::generics::transform::{GenericRotate, GenericScale, GenericTranslate};

use crate::transform::compute_css_reference_frame_matrix;

/// Structural reference-frame mode used by the renderer.
///
/// HAVI supports ordinary 2D reference frames directly. When CSS produces a
/// true 3D or perspective matrix, the renderer falls back to a flattened 2D
/// approximation from `transform.rs`. Pure `perspective` owners without a
/// transform still establish a structural isolation boundary, but do not rebase
/// descendant local coordinates to the border-box anchor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReferenceFrameMode {
    /// The subtree uses an anchored transform, including flattened 3D fallback.
    AnchoredTransform,
    /// The subtree needs structural isolation only.
    PerspectiveOnlyIsolation,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ReferenceFrameSpec {
    pub mode: ReferenceFrameMode,
    pub matrix: Mat4f,
    pub origin_basis: Option<DVec2>,
}

pub(crate) fn reference_frame_spec(
    bf: &BoxFragment,
    current_origin: DVec2,
) -> Option<ReferenceFrameSpec> {
    let border_rect = bf.border_rect();
    let bw = border_rect.size.width.to_f32_px();
    let bh = border_rect.size.height.to_f32_px();
    let anchor = border_origin_absolute(bf, current_origin);
    let style = &bf.base.style;
    let presence = transform_presence(style);

    if !presence.has_any_reference_frame_effect() {
        return None;
    }

    if !presence.has_transform && presence.has_perspective {
        return Some(ReferenceFrameSpec {
            mode: ReferenceFrameMode::PerspectiveOnlyIsolation,
            matrix: Mat4f::identity(),
            origin_basis: None,
        });
    }

    let css_matrix = compute_css_reference_frame_matrix(style, bw, bh)?;
    Some(ReferenceFrameSpec {
        mode: ReferenceFrameMode::AnchoredTransform,
        matrix: compose_reference_frame_transform(anchor, css_matrix),
        origin_basis: Some(anchor),
    })
}

pub(crate) fn border_origin_absolute(bf: &BoxFragment, current_origin: DVec2) -> DVec2 {
    let border_rect = bf.border_rect();
    dvec2(
        current_origin.x + border_rect.origin.x.to_f32_px() as f64,
        current_origin.y + border_rect.origin.y.to_f32_px() as f64,
    )
}

fn compose_reference_frame_transform(anchor: DVec2, transform: Mat4f) -> Mat4f {
    Mat4f::mul(&translation_matrix(anchor.x as f32, anchor.y as f32), &transform)
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
