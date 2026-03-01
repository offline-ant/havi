//! CSS transform extraction.

use style::properties::ComputedValues;

/// Full 2D affine transform: | m11 m21 tx |
///                           | m12 m22 ty |
/// Applied with transform-origin baked in.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Transform2D {
    pub m11: f32,
    pub m12: f32,
    pub m21: f32,
    pub m22: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Transform2D {
    /// True if this is a pure translation (no rotation, skew, or scale != 1).
    pub fn is_translate_only(&self) -> bool {
        (self.m11 - 1.0).abs() < 1e-5 && self.m12.abs() < 1e-5
            && self.m21.abs() < 1e-5 && (self.m22 - 1.0).abs() < 1e-5
    }

    /// Convert to a Makepad Mat4f (column-major 4x4).
    pub fn to_mat4f(&self) -> [f32; 16] {
        [
            self.m11, self.m12, 0.0, 0.0,
            self.m21, self.m22, 0.0, 0.0,
            0.0,      0.0,      1.0, 0.0,
            self.tx,  self.ty,  0.0, 1.0,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::Transform2D;

    #[test]
    fn translate_only_detected() {
        let t = Transform2D { m11: 1.0, m12: 0.0, m21: 0.0, m22: 1.0, tx: 10.0, ty: 20.0 };
        assert!(t.is_translate_only());
    }

    #[test]
    fn rotation_detected_as_non_trivial() {
        // 45-degree rotation: cos(45) ≈ 0.707, sin(45) ≈ 0.707
        let c = std::f32::consts::FRAC_1_SQRT_2;
        let t = Transform2D { m11: c, m12: c, m21: -c, m22: c, tx: 0.0, ty: 0.0 };
        assert!(!t.is_translate_only());
    }

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
    fn mat4f_identity_for_translate() {
        let t = Transform2D { m11: 1.0, m12: 0.0, m21: 0.0, m22: 1.0, tx: 5.0, ty: 3.0 };
        let m = t.to_mat4f();
        // Column-major: m[12]=tx, m[13]=ty
        assert_eq!(m[12], 5.0);
        assert_eq!(m[13], 3.0);
        assert_eq!(m[0], 1.0);
        assert_eq!(m[5], 1.0);
    }
}

/// Compute a full 2D affine transform from CSS `transform` property,
/// with transform-origin baked into the translation.
pub(crate) fn compute_css_transform_2d(
    computed: &ComputedValues,
    bw: f32,
    bh: f32,
) -> Option<Transform2D> {
    let transform_list = &computed.get_box().transform;
    if transform_list.0.is_empty() {
        return None;
    }

    use style::values::computed::length::CSSPixelLength;
    use euclid::{Rect, Point2D, Size2D, UnknownUnit};

    let reference_box: Rect<CSSPixelLength, UnknownUnit> = Rect::new(
        Point2D::new(CSSPixelLength::new(0.0), CSSPixelLength::new(0.0)),
        Size2D::new(CSSPixelLength::new(bw), CSSPixelLength::new(bh)),
    );

    let (matrix, _is_3d) = transform_list.to_transform_3d_matrix(Some(&reference_box)).ok()?;

    if !matrix.is_invertible() {
        return None;
    }

    let origin = &computed.get_box().transform_origin;
    let ox = origin.horizontal.to_used_value(app_units::Au::from_f32_px(bw)).to_f32_px();
    let oy = origin.vertical.to_used_value(app_units::Au::from_f32_px(bh)).to_f32_px();

    // T(ox,oy) * M * T(-ox,-oy) = full affine with origin baked in.
    // Result matrix:
    //   m11' = m11,  m21' = m21
    //   m12' = m12,  m22' = m22
    //   tx'  = ox*(1-m11) - oy*m21 + m41
    //   ty'  = -ox*m12 + oy*(1-m22) + m42
    let tx = ox * (1.0 - matrix.m11) - oy * matrix.m21 + matrix.m41;
    let ty = -ox * matrix.m12 + oy * (1.0 - matrix.m22) + matrix.m42;

    Some(Transform2D {
        m11: matrix.m11,
        m12: matrix.m12,
        m21: matrix.m21,
        m22: matrix.m22,
        tx,
        ty,
    })
}

/// Compute a full 3D transform matrix (4x4, column-major) from CSS `transform`
/// and `perspective` properties, with transform-origin baked in.
/// Returns None if there is no effective transform or perspective.
pub(crate) fn compute_css_transform_3d(
    computed: &ComputedValues,
    bw: f32,
    bh: f32,
) -> Option<[f32; 16]> {
    use style::values::computed::length::CSSPixelLength;
    use style::values::generics::box_::Perspective;
    use euclid::{Rect, Point2D, Size2D, Transform3D, UnknownUnit};

    let box_style = computed.get_box();
    let transform_list = &box_style.transform;
    let has_transform = !transform_list.0.is_empty();
    let has_perspective = !matches!(box_style.perspective, Perspective::None);

    if !has_transform && !has_perspective {
        return None;
    }

    let reference_box: Rect<CSSPixelLength, UnknownUnit> = Rect::new(
        Point2D::new(CSSPixelLength::new(0.0), CSSPixelLength::new(0.0)),
        Size2D::new(CSSPixelLength::new(bw), CSSPixelLength::new(bh)),
    );

    // Compute the transform matrix.
    let transform: Transform3D<f32, UnknownUnit, UnknownUnit> = if has_transform {
        let (matrix, _is_3d) = transform_list.to_transform_3d_matrix(Some(&reference_box)).ok()?;
        if !matrix.is_invertible() {
            return None;
        }
        matrix
    } else {
        Transform3D::identity()
    };

    // Compute the perspective matrix.
    let perspective: Option<Transform3D<f32, UnknownUnit, UnknownUnit>> = match box_style.perspective {
        Perspective::Length(length) => {
            let d = length.px();
            if d > 0.0 {
                // CSS perspective matrix: identical to WebRender's create_perspective_matrix.
                let m = Transform3D::new(
                    1.0, 0.0, 0.0, 0.0,
                    0.0, 1.0, 0.0, 0.0,
                    0.0, 0.0, 1.0, -1.0 / d,
                    0.0, 0.0, 0.0, 1.0,
                );
                // Apply perspective-origin.
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

    // Combine: perspective * transform (perspective first, then transform).
    let combined = match perspective {
        Some(p) => p.then(&transform),
        None => transform,
    };

    // Apply transform-origin.
    let origin = &box_style.transform_origin;
    let ox = origin.horizontal.to_used_value(app_units::Au::from_f32_px(bw)).to_f32_px();
    let oy = origin.vertical.to_used_value(app_units::Au::from_f32_px(bh)).to_f32_px();
    let oz = origin.depth.px();

    let result = change_basis(&combined, ox, oy, oz);

    // Column-major layout for Mat4f.
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

/// T(x,y,z) * M * T(-x,-y,-z)
fn change_basis<U, V>(m: &euclid::Transform3D<f32, U, V>, x: f32, y: f32, z: f32) -> euclid::Transform3D<f32, U, V> {
    m.pre_translate(euclid::Vector3D::new(-x, -y, -z))
     .then_translate(euclid::Vector3D::new(x, y, z))
}
