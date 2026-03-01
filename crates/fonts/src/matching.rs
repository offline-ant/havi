use havi_platform_fonts::{FontStyle, SystemFontFace};

/// A font matching request (CSS font properties).
pub struct FontRequest {
    pub weight: f32,
    pub stretch: f32,
    pub style: FontStyle,
}

impl Default for FontRequest {
    fn default() -> Self {
        Self {
            weight: 400.0,
            stretch: 1.0,
            style: FontStyle::Normal,
        }
    }
}

/// Returns index of best matching face, or None if faces is empty.
pub fn match_font(faces: &[SystemFontFace], request: &FontRequest) -> Option<usize> {
    if faces.is_empty() {
        return None;
    }
    let mut best_index = 0;
    let mut best_distance = f64::MAX;
    for (i, face) in faces.iter().enumerate() {
        let d = total_distance(face, request);
        if d < best_distance {
            best_distance = d;
            best_index = i;
        }
    }
    Some(best_index)
}

fn total_distance(face: &SystemFontFace, request: &FontRequest) -> f64 {
    let stretch = stretch_distance(request.stretch, face.stretch);
    let style = style_distance(&request.style, &face.style);
    let weight = weight_distance(request.weight, face.weight);
    stretch as f64 * 1e8 + style as f64 * 1e4 + weight as f64
}

/// Stretch distance in [0, 2000].
/// Stretch values are percentages (1.0 = 100% = normal).
fn stretch_distance(target: f32, face_stretch: f32) -> f32 {
    const REVERSE_DISTANCE: f32 = 1000.0;
    let target_pct = target * 100.0;
    let face_pct = face_stretch * 100.0;

    if target_pct > 100.0 {
        // Prefer wider first
        if face_pct < target_pct {
            return (target_pct - face_pct) + REVERSE_DISTANCE;
        }
        return face_pct - target_pct;
    }
    // target <= 100%: prefer narrower first
    if face_pct > target_pct {
        return (face_pct - target_pct) + REVERSE_DISTANCE;
    }
    target_pct - face_pct
}

/// Weight distance in [0, 1600].
fn weight_distance(target: f32, face_weight: f32) -> f32 {
    const NOT_WITHIN_CENTRAL_RANGE: f32 = 100.0;
    const REVERSE_DISTANCE: f32 = 600.0;

    if (target - face_weight).abs() < f32::EPSILON {
        return 0.0;
    }

    if target < 400.0 {
        if face_weight <= target {
            return target - face_weight;
        }
        return (face_weight - target) + REVERSE_DISTANCE;
    }

    if target > 500.0 {
        if face_weight >= target {
            return face_weight - target;
        }
        return (target - face_weight) + REVERSE_DISTANCE;
    }

    // Special [400, 500] range
    if face_weight > target {
        if face_weight <= 500.0 {
            return face_weight - target;
        }
        return (face_weight - target) + REVERSE_DISTANCE;
    }
    // Lighter weights within [400,500] target range
    (target - face_weight) + NOT_WITHIN_CENTRAL_RANGE
}

/// Style distance in [0, 500].
fn style_distance(target: &FontStyle, face: &FontStyle) -> f32 {
    if target == face {
        return 0.0;
    }

    const REVERSE: f32 = 100.0;
    const NEGATE: f32 = 200.0;
    const DEFAULT_OBLIQUE_ANGLE: f32 = 14.0;

    match target {
        FontStyle::Normal => match face {
            FontStyle::Normal => 0.0,
            FontStyle::Oblique(angle) => {
                if *angle >= 0.0 {
                    1.0 + angle
                } else {
                    NEGATE - angle
                }
            }
            FontStyle::Italic => REVERSE,
        },
        FontStyle::Italic => match face {
            FontStyle::Oblique(angle) => {
                if *angle >= DEFAULT_OBLIQUE_ANGLE {
                    1.0 + (angle - DEFAULT_OBLIQUE_ANGLE)
                } else if *angle > 0.0 {
                    REVERSE + (DEFAULT_OBLIQUE_ANGLE - angle)
                } else {
                    REVERSE + NEGATE + (DEFAULT_OBLIQUE_ANGLE - angle)
                }
            }
            FontStyle::Normal => NEGATE,
            FontStyle::Italic => 0.0,
        },
        FontStyle::Oblique(target_angle) => {
            let target_angle = *target_angle;
            match face {
                FontStyle::Normal => {
                    if target_angle >= 0.0 {
                        REVERSE + NEGATE + 1.0
                    } else {
                        REVERSE + NEGATE + 1.0
                    }
                }
                FontStyle::Italic => {
                    if target_angle >= DEFAULT_OBLIQUE_ANGLE
                        || target_angle <= -DEFAULT_OBLIQUE_ANGLE
                    {
                        REVERSE + NEGATE
                    } else if target_angle >= 0.0 {
                        REVERSE + NEGATE - 2.0
                    } else {
                        REVERSE + NEGATE - 2.0
                    }
                }
                FontStyle::Oblique(face_angle) => {
                    let face_angle = *face_angle;
                    oblique_vs_oblique_distance(target_angle, face_angle)
                }
            }
        }
    }
}

fn oblique_vs_oblique_distance(target_angle: f32, face_angle: f32) -> f32 {
    const REVERSE: f32 = 100.0;
    const NEGATE: f32 = 200.0;
    const DEFAULT_OBLIQUE_ANGLE: f32 = 14.0;

    if target_angle >= DEFAULT_OBLIQUE_ANGLE {
        if face_angle >= target_angle {
            return face_angle - target_angle;
        }
        if face_angle > 0.0 {
            return REVERSE + (target_angle - face_angle);
        }
        REVERSE + NEGATE + (target_angle - face_angle)
    } else if target_angle <= -DEFAULT_OBLIQUE_ANGLE {
        if face_angle <= target_angle {
            return target_angle - face_angle;
        }
        if face_angle < 0.0 {
            return REVERSE + (face_angle - target_angle);
        }
        REVERSE + NEGATE + (face_angle - target_angle)
    } else if target_angle >= 0.0 {
        if face_angle > target_angle {
            return REVERSE + (face_angle - target_angle);
        }
        if face_angle >= 0.0 {
            return target_angle - face_angle;
        }
        REVERSE + NEGATE + (target_angle - face_angle)
    } else {
        // target_angle < 0 && target_angle > -DEFAULT_OBLIQUE_ANGLE
        if face_angle < target_angle {
            return REVERSE + (target_angle - face_angle);
        }
        if face_angle <= 0.0 {
            return face_angle - target_angle;
        }
        REVERSE + NEGATE + (face_angle - target_angle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(weight: f32, stretch: f32, style: FontStyle) -> SystemFontFace {
        SystemFontFace {
            path: std::path::PathBuf::new(),
            index: 0,
            weight,
            stretch,
            style,
        }
    }

    #[test]
    fn exact_match() {
        let faces = vec![
            face(400.0, 1.0, FontStyle::Normal),
            face(700.0, 1.0, FontStyle::Normal),
        ];
        let req = FontRequest { weight: 700.0, stretch: 1.0, style: FontStyle::Normal };
        assert_eq!(match_font(&faces, &req), Some(1));
    }

    #[test]
    fn prefer_heavier_for_bold() {
        let faces = vec![
            face(400.0, 1.0, FontStyle::Normal),
            face(800.0, 1.0, FontStyle::Normal),
        ];
        let req = FontRequest { weight: 600.0, stretch: 1.0, style: FontStyle::Normal };
        assert_eq!(match_font(&faces, &req), Some(1));
    }

    #[test]
    fn prefer_lighter_for_light() {
        let faces = vec![
            face(200.0, 1.0, FontStyle::Normal),
            face(500.0, 1.0, FontStyle::Normal),
        ];
        let req = FontRequest { weight: 300.0, stretch: 1.0, style: FontStyle::Normal };
        assert_eq!(match_font(&faces, &req), Some(0));
    }

    #[test]
    fn italic_preferred_over_normal() {
        let faces = vec![
            face(400.0, 1.0, FontStyle::Normal),
            face(400.0, 1.0, FontStyle::Italic),
        ];
        let req = FontRequest { weight: 400.0, stretch: 1.0, style: FontStyle::Italic };
        assert_eq!(match_font(&faces, &req), Some(1));
    }

    #[test]
    fn empty_faces() {
        let faces: Vec<SystemFontFace> = vec![];
        let req = FontRequest::default();
        assert_eq!(match_font(&faces, &req), None);
    }
}
