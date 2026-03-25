
use makepad_widgets::Mat4f;
use style::properties::ComputedValues;
use style::values::generics::transform::{GenericRotate, GenericScale, GenericTranslate};

#[cfg(test)]
mod tests {
    use makepad_widgets::Mat4f;

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
    fn descendant_perspective_matrix_contains_perspective_without_self_transform() {
        let mut style = style::properties::ComputedValues::initial_values_with_font_override(
            style::properties::generated::style_structs::Font::initial_values(),
        );
        servo_arc::Arc::make_mut(&mut style)
            .mutate_box()
            .set_perspective(style::values::generics::box_::Perspective::Length(
                style::values::computed::length::NonNegativeLength::new(600.0),
            ));

        let self_transform = super::compute_css_self_transform_3d(&style.to_arc(), 400.0, 300.0);
        let perspective = super::compute_css_descendant_perspective_matrix(&style.to_arc(), 400.0, 300.0).unwrap();

        assert!(self_transform.is_none());
        assert!(super::is_3d_matrix(&perspective));
        assert!((perspective[11] + (1.0 / 600.0)).abs() < 0.0001);
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

    #[test]
    fn change_basis_rotate_90deg_center_maps_points_and_matrix_as_expected() {
        let rotate = euclid::Transform3D::<f32, euclid::UnknownUnit, euclid::UnknownUnit>::new(
            0.0, 1.0, 0.0, 0.0,
            -1.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        );
        let transformed = super::change_basis(&rotate, 75.0, 50.0, 0.0);
        let matrix = super::transform_to_array(&transformed);
        let m = Mat4f { v: matrix };

        let p00 = m.transform_vec4(makepad_widgets::vec4f(0.0, 0.0, 0.0, 1.0));
        let p10 = m.transform_vec4(makepad_widgets::vec4f(150.0, 0.0, 0.0, 1.0));
        let p01 = m.transform_vec4(makepad_widgets::vec4f(0.0, 100.0, 0.0, 1.0));

        assert!((p00.x - 125.0).abs() < 0.01);
        assert!((p00.y + 25.0).abs() < 0.01);
        assert!((p10.x - 125.0).abs() < 0.01);
        assert!((p10.y - 125.0).abs() < 0.01);
        assert!((p01.x - 25.0).abs() < 0.01);
        assert!((p01.y + 25.0).abs() < 0.01);

        let expected = [
            0.0, 1.0, 0.0, 0.0,
            -1.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            125.0, -25.0, 0.0, 1.0,
        ];
        for (actual, expected) in matrix.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 0.01, "actual={actual} expected={expected}");
        }
    }
}

pub(crate) fn compute_css_reference_frame_matrix(
    computed: &ComputedValues,
    bw: f32,
    bh: f32,
) -> Option<Mat4f> {
    let box_style = computed.get_box();
    let has_transform = !box_style.transform.0.is_empty()
        || box_style.scale != GenericScale::None
        || box_style.rotate != GenericRotate::None
        || box_style.translate != GenericTranslate::None;
    if !has_transform {
        return None;
    }

    let matrix = compute_css_self_transform_3d(computed, bw, bh)?;
    Some(Mat4f { v: matrix })
}

pub(crate) fn compute_css_self_transform_3d(
    computed: &ComputedValues,
    bw: f32,
    bh: f32,
) -> Option<[f32; 16]> {
    let transform = compute_css_self_transform_euclid(computed, bw, bh)?;
    Some(transform_to_array(&transform))
}

// CSS `perspective` affects descendants, not the element's own geometry, so it
// is lowered separately from the element's self transform.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn compute_css_descendant_perspective_matrix(
    computed: &ComputedValues,
    bw: f32,
    bh: f32,
) -> Option<[f32; 16]> {
    let perspective = compute_css_descendant_perspective_euclid(computed, bw, bh)?;
    Some(transform_to_array(&perspective))
}

fn compute_css_self_transform_euclid(
    computed: &ComputedValues,
    bw: f32,
    bh: f32,
) -> Option<euclid::Transform3D<f32, euclid::UnknownUnit, euclid::UnknownUnit>> {
    use euclid::{Point2D, Rect, Size2D, Transform3D, UnknownUnit};
    use style::values::computed::length::CSSPixelLength;

    let box_style = computed.get_box();
    let transform_list = &box_style.transform;
    let has_transform = !transform_list.0.is_empty();
    let has_individual_transform = box_style.scale != GenericScale::None
        || box_style.rotate != GenericRotate::None
        || box_style.translate != GenericTranslate::None;

    if !has_transform && !has_individual_transform {
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

    let origin = &box_style.transform_origin;
    let ox = origin.horizontal.to_used_value(app_units::Au::from_f32_px(bw)).to_f32_px();
    let oy = origin.vertical.to_used_value(app_units::Au::from_f32_px(bh)).to_f32_px();
    let oz = origin.depth.px();

    Some(change_basis(&transform, ox, oy, oz))
}

#[cfg_attr(not(test), allow(dead_code))]
fn compute_css_descendant_perspective_euclid(
    computed: &ComputedValues,
    bw: f32,
    bh: f32,
) -> Option<euclid::Transform3D<f32, euclid::UnknownUnit, euclid::UnknownUnit>> {
    use euclid::Transform3D;
    use style::values::generics::box_::Perspective;

    let box_style = computed.get_box();
    match box_style.perspective {
        Perspective::Length(length) => {
            let d = length.px();
            if d <= 0.0 {
                return None;
            }
            let matrix = Transform3D::new(
                1.0, 0.0, 0.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, -1.0 / d,
                0.0, 0.0, 0.0, 1.0,
            );
            let origin = &box_style.perspective_origin;
            let ox = origin.horizontal.to_used_value(app_units::Au::from_f32_px(bw)).to_f32_px();
            let oy = origin.vertical.to_used_value(app_units::Au::from_f32_px(bh)).to_f32_px();
            Some(change_basis(&matrix, ox, oy, 0.0))
        }
        Perspective::None => None,
    }
}

fn transform_to_array<U, V>(transform: &euclid::Transform3D<f32, U, V>) -> [f32; 16] {
    [
        transform.m11, transform.m12, transform.m13, transform.m14,
        transform.m21, transform.m22, transform.m23, transform.m24,
        transform.m31, transform.m32, transform.m33, transform.m34,
        transform.m41, transform.m42, transform.m43, transform.m44,
    ]
}

#[cfg(test)]
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

// Flatten a 3D reference frame by solving the affine map from projected corner
// positions on the local z=0 plane. Sampling only basis vectors is not enough
// once perspective and shear are involved.
#[cfg(test)]
fn flatten_3d_reference_frame_to_2d(m: &[f32; 16], bw: f32, bh: f32) -> Option<[f32; 16]> {
    fn project(m: &[f32; 16], x: f32, y: f32) -> Option<(f32, f32)> {
        let p = Mat4f { v: *m }.transform_vec4(makepad_widgets::vec4f(x, y, 0.0, 1.0));
        if p.w.abs() <= 1e-6 {
            return None;
        }
        let px = p.x / p.w;
        let py = p.y / p.w;
        if px.is_finite() && py.is_finite() {
            Some((px, py))
        } else {
            None
        }
    }

    let px = if bw.abs() > 1e-6 { bw } else { 1.0 };
    let py = if bh.abs() > 1e-6 { bh } else { 1.0 };

    let p00 = project(m, 0.0, 0.0)?;
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

// Apply CSS transform-origin in euclid's row-vector convention.
fn change_basis<U, V>(m: &euclid::Transform3D<f32, U, V>, x: f32, y: f32, z: f32) -> euclid::Transform3D<f32, U, V> {
    euclid::Transform3D::translation(-x, -y, -z)
        .then(m)
        .then_translate(euclid::Vector3D::new(x, y, z))
}
