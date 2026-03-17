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

    let node_id = bf.base.tag.map(|t| t.node.0).unwrap_or(0);
    eprintln!("[REF_FRAME] node={} border_rect=({},{} {}x{}) current_origin=({},{}) anchor=({},{}) has_transform={} has_perspective={}",
        node_id,
        border_rect.origin.x.to_f32_px(), border_rect.origin.y.to_f32_px(),
        bw, bh,
        current_origin.x, current_origin.y,
        anchor.x, anchor.y,
        presence.has_transform, presence.has_perspective);

    if !presence.has_transform && presence.has_perspective {
        eprintln!("[REF_FRAME] node={} -> PerspectiveOnlyIsolation", node_id);
        return Some(ReferenceFrameSpec {
            mode: ReferenceFrameMode::PerspectiveOnlyIsolation,
            matrix: Mat4f::identity(),
        });
    }

    let css_matrix = compute_css_reference_frame_matrix(style, bw, bh)?;
    let composed = compose_reference_frame_transform(anchor, css_matrix);
    eprintln!("[REF_FRAME] node={} css_matrix=[{:.2},{:.2},{:.2},{:.2} | {:.2},{:.2},{:.2},{:.2} | {:.2},{:.2},{:.2},{:.2} | {:.2},{:.2},{:.2},{:.2}]",
        node_id,
        css_matrix.v[0], css_matrix.v[1], css_matrix.v[2], css_matrix.v[3],
        css_matrix.v[4], css_matrix.v[5], css_matrix.v[6], css_matrix.v[7],
        css_matrix.v[8], css_matrix.v[9], css_matrix.v[10], css_matrix.v[11],
        css_matrix.v[12], css_matrix.v[13], css_matrix.v[14], css_matrix.v[15]);
    eprintln!("[REF_FRAME] node={} composed=[{:.2},{:.2},{:.2},{:.2} | {:.2},{:.2},{:.2},{:.2} | {:.2},{:.2},{:.2},{:.2} | {:.2},{:.2},{:.2},{:.2}]",
        node_id,
        composed.v[0], composed.v[1], composed.v[2], composed.v[3],
        composed.v[4], composed.v[5], composed.v[6], composed.v[7],
        composed.v[8], composed.v[9], composed.v[10], composed.v[11],
        composed.v[12], composed.v[13], composed.v[14], composed.v[15]);
    Some(ReferenceFrameSpec {
        mode: ReferenceFrameMode::AnchoredTransform,
        matrix: composed,
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
    // T(anchor) * transform * T(-anchor)
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
