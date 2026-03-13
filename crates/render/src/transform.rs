//! CSS transform extraction.

use makepad_widgets::Mat4f;
use style::properties::ComputedValues;
use style::values::generics::box_::Perspective;
use style::values::generics::transform::{GenericRotate, GenericScale, GenericTranslate};

#[cfg(test)]
mod tests {

    #[test]
    fn is_3d_identity_is_not_3d() {
        // Pure identity: not 3D.
        let m = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        assert!(!super::is_3d_matrix(&m));
    }

    #[test]
    fn is_3d_rotation_2d_is_not_3d() {
        // 2D rotation: m13=m14=m23=m24=0, m33=m44=1.
        let c = std::f32::consts::FRAC_1_SQRT_2;
        let m = [
            c,  c,  0.0, 0.0,
            -c, c,  0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        assert!(!super::is_3d_matrix(&m));
    }

    #[test]
    fn is_3d_perspective_detected() {
        // Perspective: m34 = -1/d != 0.
        let m = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, -0.002,
            0.0, 0.0, 0.0, 1.0,
        ];
        assert!(super::is_3d_matrix(&m));
    }

    #[test]
    fn is_3d_rotatey_detected() {
        // rotateY(45deg): m31 and m13 are non-zero.
        let c = std::f32::consts::FRAC_1_SQRT_2;
        let m = [
            c,   0.0, -c,  0.0,
            0.0, 1.0, 0.0, 0.0,
            c,   0.0, c,   0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        assert!(super::is_3d_matrix(&m));
    }

    #[test]
    fn flatten_3d_preserves_2d_translation() {
        let m = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            -1.6, -0.52, 1.0, -0.0016,
            640.0, 555.0, 0.0, 1.0,
        ];
        let flattened = super::flatten_3d_reference_frame_to_2d(&m, 1280.0, 720.0).unwrap();
        assert!(!super::is_3d_matrix(&flattened));
        assert!((flattened[12] - 640.0).abs() < 0.01);
        assert!((flattened[13] - 555.0).abs() < 0.01);
    }

    #[test]
    fn flatten_3d_rotatey_keeps_origin_finite() {
        let c = std::f32::consts::FRAC_1_SQRT_2;
        let m = [
            c, 0.0, -c, 0.0,
            0.0, 1.0, 0.0, 0.0,
            c, 0.0, c, 0.0,
            100.0, 50.0, 0.0, 1.0,
        ];
        let flattened = super::flatten_3d_reference_frame_to_2d(&m, 400.0, 300.0).unwrap();
        assert!(!super::is_3d_matrix(&flattened));
        assert!(flattened[12].is_finite());
        assert!(flattened[13].is_finite());
    }
}

/// Compute a full 3D transform matrix (4x4, column-major) from CSS `transform`
/// and `perspective` properties, with transform-origin baked in.
/// Returns None if there is no effective transform or perspective.
pub(crate) fn has_effective_transform_or_perspective(
    computed: &ComputedValues,
) -> bool {
    let box_style = computed.get_box();
    !box_style.transform.0.is_empty()
        || box_style.scale != GenericScale::None
        || box_style.rotate != GenericRotate::None
        || box_style.translate != GenericTranslate::None
        || box_style.perspective != Perspective::None
}

/// Compute the renderer's 2D reference-frame matrix.
///
/// HAVI does not implement full 3D scene composition. When CSS produces a
/// genuine 3D or perspective matrix, the renderer falls back to a 2D affine
/// approximation of the transformed z=0 plane. This preserves structural frame
/// isolation and slide ownership while keeping the Makepad backend strictly 2D.
pub(crate) fn compute_css_reference_frame_matrix(
    computed: &ComputedValues,
    bw: f32,
    bh: f32,
) -> Option<Mat4f> {
    if !has_effective_transform_or_perspective(computed) {
        return None;
    }

    let matrix = compute_css_transform_3d(computed, bw, bh)?;
    let matrix = if is_3d_matrix(&matrix) {
        flatten_3d_reference_frame_to_2d(&matrix, bw, bh)?
    } else {
        matrix
    };
    Some(Mat4f { v: matrix })
}

pub(crate) fn compute_css_transform_3d(
    computed: &ComputedValues,
    bw: f32,
    bh: f32,
) -> Option<[f32; 16]> {
    use euclid::{Point2D, Rect, Size2D, Transform3D, UnknownUnit};
    use style::values::computed::length::CSSPixelLength;
    use style::values::generics::box_::Perspective;

    let box_style = computed.get_box();
    let transform_list = &box_style.transform;
    let has_transform = !transform_list.0.is_empty();
    let has_perspective = !matches!(box_style.perspective, Perspective::None);
    let has_individual_transform = box_style.scale != GenericScale::None
        || box_style.rotate != GenericRotate::None
        || box_style.translate != GenericTranslate::None;

    if !has_transform && !has_perspective && !has_individual_transform {
        return None;
    }

    let reference_box: Rect<CSSPixelLength, UnknownUnit> = Rect::new(
        Point2D::new(CSSPixelLength::new(0.0), CSSPixelLength::new(0.0)),
        Size2D::new(CSSPixelLength::new(bw), CSSPixelLength::new(bh)),
    );

    let list_transform: Transform3D<f32, UnknownUnit, UnknownUnit> = if has_transform {
        let (matrix, _is_3d) = transform_list.to_transform_3d_matrix(Some(&reference_box)).ok()?;
        if !matrix.is_invertible() {
            return None;
        }
        matrix
    } else {
        Transform3D::identity()
    };

    let rotate = match box_style.rotate {
        GenericRotate::Rotate(angle) => (0.0, 0.0, 1.0, angle.radians()),
        GenericRotate::Rotate3D(x, y, z, angle) => (x, y, z, angle.radians()),
        GenericRotate::None => (0.0, 0.0, 1.0, 0.0),
    };
    let scale = match box_style.scale {
        GenericScale::Scale(sx, sy, sz) => (sx, sy, sz),
        GenericScale::None => (1.0, 1.0, 1.0),
    };
    let translate: Transform3D<f32, UnknownUnit, UnknownUnit> = match &box_style.translate {
        GenericTranslate::Translate(x, y, z) => Transform3D::translation(
            x.resolve(CSSPixelLength::new(bw)).px(),
            y.resolve(CSSPixelLength::new(bh)).px(),
            z.px(),
        ),
        GenericTranslate::None => Transform3D::identity(),
    };

    let transform = list_transform
        .then_rotate(rotate.0, rotate.1, rotate.2, euclid::Angle::radians(rotate.3))
        .then_scale(scale.0, scale.1, scale.2)
        .then(&translate);

    let perspective: Option<Transform3D<f32, UnknownUnit, UnknownUnit>> = match box_style.perspective {
        Perspective::Length(length) => {
            let d = length.px();
            if d > 0.0 {
                let m = Transform3D::new(
                    1.0, 0.0, 0.0, 0.0,
                    0.0, 1.0, 0.0, 0.0,
                    0.0, 0.0, 1.0, -1.0 / d,
                    0.0, 0.0, 0.0, 1.0,
                );
                let po = &box_style.perspective_origin;
                let pox = po.horizontal.to_used_value(app_units::Au::from_f32_px(bw)).to_f32_px();
                let poy = po.vertical.to_used_value(app_units::Au::from_f32_px(bh)).to_f32_px();
                Some(change_basis(&m, pox, poy, 0.0))
            } else {
                None
            }
        }
        Perspective::None => None,
    };

    let combined = match perspective {
        Some(p) => p.then(&transform),
        None => transform,
    };

    let origin = &box_style.transform_origin;
    let ox = origin.horizontal.to_used_value(app_units::Au::from_f32_px(bw)).to_f32_px();
    let oy = origin.vertical.to_used_value(app_units::Au::from_f32_px(bh)).to_f32_px();
    let oz = origin.depth.px();

    let result = change_basis(&combined, ox, oy, oz);

    Some([
        result.m11, result.m12, result.m13, result.m14,
        result.m21, result.m22, result.m23, result.m24,
        result.m31, result.m32, result.m33, result.m34,
        result.m41, result.m42, result.m43, result.m44,
    ])
}

/// Check if a column-major 4x4 matrix has 3D components (not a pure 2D affine).
pub(crate) fn is_3d_matrix(m: &[f32; 16]) -> bool {
    // In column-major layout:
    // col0: [m11, m12, m13, m14] = m[0..4]
    // col1: [m21, m22, m23, m24] = m[4..8]
    // col2: [m31, m32, m33, m34] = m[8..12]
    // col3: [m41, m42, m43, m44] = m[12..16]
    // A 2D affine has m13=m14=m23=m24=m31=m32=m34=m43=0 and m33=m44=1.
    m[2].abs() > 1e-5 || m[3].abs() > 1e-5 ||      // m13, m14
    m[6].abs() > 1e-5 || m[7].abs() > 1e-5 ||      // m23, m24
    m[8].abs() > 1e-5 || m[9].abs() > 1e-5 ||      // m31, m32
    (m[10] - 1.0).abs() > 1e-5 ||                    // m33
    m[11].abs() > 1e-5 ||                            // m34
    m[14].abs() > 1e-5 ||                            // m43
    (m[15] - 1.0).abs() > 1e-5                       // m44
}

/// Flatten a 3D reference frame into a 2D affine basis.
///
/// The fallback samples the transformed origin and the transformed unit axes on
/// the element's local z=0 plane, then rebuilds a 2D affine matrix from those
/// projected basis vectors.
fn flatten_3d_reference_frame_to_2d(m: &[f32; 16], bw: f32, bh: f32) -> Option<[f32; 16]> {
    fn project(m: &[f32; 16], x: f32, y: f32) -> Option<(f32, f32)> {
        let p = Mat4f { v: *m }.transform_vec4(makepad_widgets::vec4f(x, y, 0.0, 1.0));
        let w = if p.w.abs() > 1e-6 { p.w } else { 1.0 };
        let px = p.x / w;
        let py = p.y / w;
        if px.is_finite() && py.is_finite() {
            Some((px, py))
        } else {
            None
        }
    }

    let p00 = project(m, 0.0, 0.0)?;
    let px = if bw.abs() > 1e-6 { bw } else { 1.0 };
    let py = if bh.abs() > 1e-6 { bh } else { 1.0 };
    let p10 = project(m, px, 0.0)?;
    let p01 = project(m, 0.0, py)?;

    let basis_x = ((p10.0 - p00.0) / px, (p10.1 - p00.1) / px);
    let basis_y = ((p01.0 - p00.0) / py, (p01.1 - p00.1) / py);

    Some([
        basis_x.0, basis_x.1, 0.0, 0.0,
        basis_y.0, basis_y.1, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        p00.0, p00.1, 0.0, 1.0,
    ])
}

/// T(x,y,z) * M * T(-x,-y,-z)
fn change_basis<U, V>(m: &euclid::Transform3D<f32, U, V>, x: f32, y: f32, z: f32) -> euclid::Transform3D<f32, U, V> {
    m.pre_translate(euclid::Vector3D::new(-x, -y, -z))
     .then_translate(euclid::Vector3D::new(x, y, z))
}
