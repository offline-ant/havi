//! Element box rendering: backgrounds, borders, outlines, box shadows, gradients.

use servo_arc::Arc;
use makepad_widgets::*;
use style::color::{AbsoluteColor, ColorSpace};
use style::properties::ComputedValues;

use crate::color::{inherited_color, resolve_color};
use crate::shaders::{DrawBoxShadow, DrawGradient, DrawRoundedColor};

/// Per-corner border radii resolved to px.
#[derive(Clone, Copy)]
pub(crate) struct BorderRadii {
    pub tl: f32,
    pub tr: f32,
    pub br: f32,
    pub bl: f32,
}

impl BorderRadii {
    pub fn max(&self) -> f32 {
        self.tl.max(self.tr).max(self.br).max(self.bl)
    }
}

/// Extract per-corner border radii from computed values.
pub(crate) fn resolve_border_radii(computed: &ComputedValues) -> BorderRadii {
    let border = computed.get_border();
    let resolve = |r: &style::values::computed::LengthPercentage| -> f32 {
        r.to_length().map_or(0.0, |l| l.px())
    };
    BorderRadii {
        tl: resolve(&border.border_top_left_radius.0.width.0),
        tr: resolve(&border.border_top_right_radius.0.width.0),
        br: resolve(&border.border_bottom_right_radius.0.width.0),
        bl: resolve(&border.border_bottom_left_radius.0.width.0),
    }
}

pub(crate) fn draw_element_box(
    cx: &mut Cx2d,
    computed: &Arc<ComputedValues>,
    x: f64, y: f64, w: f32, h: f32,
    draw_bg: &mut DrawColor,
    draw_rounded_bg: &mut DrawRoundedColor,
    draw_box_shadow: &mut DrawBoxShadow,
    draw_gradient: &mut DrawGradient,
    opacity: f32,
) {
    let current = inherited_color(computed);
    let current_abs = AbsoluteColor::new(ColorSpace::Srgb, current.x, current.y, current.z, current.w);
    let radii = resolve_border_radii(computed);

    draw_box_shadows(cx, computed, x, y, w, h, radii.max(), &current_abs, draw_box_shadow, opacity);

    let mut bg_color = resolve_color(&computed.get_background().background_color, &current_abs);
    bg_color.w *= opacity;

    if bg_color.w > 0.001 {
        let rect = Rect { pos: dvec2(x, y), size: dvec2(w as f64, h as f64) };
        if radii.max() > 0.0 {
            draw_rounded_bg.color = bg_color;
            draw_rounded_bg.border_radius_tl = radii.tl;
            draw_rounded_bg.border_radius_tr = radii.tr;
            draw_rounded_bg.border_radius_br = radii.br;
            draw_rounded_bg.border_radius_bl = radii.bl;
            draw_rounded_bg.draw_abs(cx, rect);
        } else {
            draw_bg.color = bg_color;
            draw_bg.draw_abs(cx, rect);
        }
    }

    draw_background_images(cx, computed, x, y, w, h, &current_abs, draw_gradient, opacity);
    draw_element_borders(cx, computed, x, y, w, h, &current_abs, draw_bg, opacity);
    draw_element_outline(cx, computed, x, y, w, h, &current_abs, draw_bg, opacity);
}

fn draw_box_shadows(
    cx: &mut Cx2d, computed: &ComputedValues,
    x: f64, y: f64, w: f32, h: f32, corner: f32,
    current_abs: &AbsoluteColor, draw_box_shadow: &mut DrawBoxShadow, opacity: f32,
) {
    let shadows = &computed.get_effects().box_shadow.0;
    if shadows.is_empty() { return; }

    for shadow in shadows.iter().rev() {
        let h_off = shadow.base.horizontal.px();
        let v_off = shadow.base.vertical.px();
        let blur = shadow.base.blur.px();
        let spread = shadow.spread.px();
        let mut color = resolve_color(&shadow.base.color, current_abs);
        color.w *= opacity;
        if color.w < 0.001 { continue; }

        let sigma = blur * 0.5;
        let extent = (sigma * 3.0).max(0.0);

        if shadow.inset {
            draw_box_shadow.shadow_color = color;
            draw_box_shadow.sigma = sigma;
            draw_box_shadow.corner = corner;
            draw_box_shadow.inset = 1.0;
            draw_box_shadow.box_offset = Vec2f { x: spread + h_off, y: spread + v_off };
            draw_box_shadow.box_size = Vec2f { x: w - 2.0 * spread, y: h - 2.0 * spread };
            draw_box_shadow.draw_abs(cx, Rect { pos: dvec2(x, y), size: dvec2(w as f64, h as f64) });
        } else {
            let shadow_w = w + 2.0 * spread;
            let shadow_h = h + 2.0 * spread;
            draw_box_shadow.shadow_color = color;
            draw_box_shadow.sigma = sigma;
            draw_box_shadow.corner = corner;
            draw_box_shadow.inset = 0.0;
            draw_box_shadow.box_offset = Vec2f { x: extent, y: extent };
            draw_box_shadow.box_size = Vec2f { x: shadow_w, y: shadow_h };
            draw_box_shadow.draw_abs(cx, Rect {
                pos: dvec2(x + (h_off - spread - extent) as f64, y + (v_off - spread - extent) as f64),
                size: dvec2((shadow_w + 2.0 * extent) as f64, (shadow_h + 2.0 * extent) as f64),
            });
        }
    }
}

fn draw_background_images(
    cx: &mut Cx2d, computed: &ComputedValues,
    x: f64, y: f64, w: f32, h: f32,
    current_abs: &AbsoluteColor, draw_gradient: &mut DrawGradient, opacity: f32,
) {
    use style::values::computed::image::Image;
    let bg = computed.get_background();
    for image in bg.background_image.0.iter().rev() {
        match image {
            Image::Gradient(ref gradient) => {
                draw_css_gradient(cx, gradient, x, y, w, h, current_abs, draw_gradient, opacity);
            }
            Image::Url(_) => {
                // URL images are resolved during fragment conversion and stored
                // in BoxFragment::background_images. Drawn by draw_background_url_images().
            }
            _ => {}
        }
    }
}

/// Draw resolved CSS background-image: url() images for a box fragment.
/// Called from makepad_builder after draw_element_box.
pub(crate) fn draw_background_url_images(
    cx: &mut Cx2d,
    background_images: &[havi_types::BackgroundImage],
    x: f64, y: f64, w: f32, h: f32,
    draw_image: &mut DrawImage,
    texture_cache: &mut crate::TextureCache,
    opacity: f32,
) {
    use makepad_widgets::makepad_draw::ImageBuffer;

    for (i, img) in background_images.iter().enumerate() {
        if img.width == 0 || img.height == 0 || img.pixels.is_empty() {
            continue;
        }
        // Use a synthetic cache key based on position + index to avoid re-uploading.
        let cache_key = (x.to_bits() as usize)
            .wrapping_mul(31)
            .wrapping_add(y.to_bits() as usize)
            .wrapping_mul(31)
            .wrapping_add(i);
        let texture = texture_cache.entry(cache_key).or_insert_with(|| {
            let data: Vec<u32> = img.pixels.chunks_exact(4).map(|px| {
                (px[2] as u32) | ((px[1] as u32) << 8) | ((px[0] as u32) << 16) | ((px[3] as u32) << 24)
            }).collect();
            let image_buffer = ImageBuffer {
                width: img.width as usize,
                height: img.height as usize,
                data,
                animation: None,
            };
            image_buffer.into_new_texture(cx.cx)
        });
        draw_image.draw_vars.set_texture(0, texture);
        draw_image.opacity = opacity;
        // Default: cover the element's padding box (same as background-size: auto).
        // TODO: Support background-size, background-position, background-repeat.
        draw_image.draw_abs(cx, Rect { pos: dvec2(x, y), size: dvec2(w as f64, h as f64) });
    }
}

fn draw_css_gradient(
    cx: &mut Cx2d,
    gradient: &style::values::computed::image::Gradient,
    x: f64, y: f64, w: f32, h: f32,
    current_abs: &AbsoluteColor, draw_gradient: &mut DrawGradient, opacity: f32,
) {
    use style::values::computed::image::{Gradient as G, LineDirection};
    use style::values::generics::image::GradientFlags;
    let rect = Rect { pos: dvec2(x, y), size: dvec2(w as f64, h as f64) };

    match gradient {
        G::Linear { items, direction, flags, .. } => {
            let (dx, dy) = match direction {
                LineDirection::Horizontal(h) => {
                    use style::values::specified::position::HorizontalPositionKeyword::*;
                    match h { Right => (1.0f32, 0.0), Left => (-1.0, 0.0) }
                }
                LineDirection::Vertical(v) => {
                    use style::values::specified::position::VerticalPositionKeyword::*;
                    match v { Top => (0.0f32, -1.0), Bottom => (0.0, 1.0) }
                }
                LineDirection::Angle(angle) => { let r = angle.radians(); (r.sin(), -r.cos()) }
                LineDirection::Corner(h, v) => {
                    use style::values::specified::position::HorizontalPositionKeyword::*;
                    use style::values::specified::position::VerticalPositionKeyword::*;
                    let hx = if matches!(h, Right) { 1.0f32 } else { -1.0 };
                    let vy = if matches!(v, Bottom) { 1.0f32 } else { -1.0 };
                    let len = (hx * hx + vy * vy).sqrt();
                    (hx / len, vy / len)
                }
            };
            let grad_len = (w * dx).abs() + (h * dy).abs();
            if grad_len < 0.001 { return; }
            let half = grad_len / 2.0;
            draw_gradient.grad_type = 0.0;
            draw_gradient.repeating = if flags.contains(GradientFlags::REPEATING) { 1.0 } else { 0.0 };
            draw_gradient.param0 = 0.5 - (dx * half) / w;
            draw_gradient.param1 = 0.5 - (dy * half) / h;
            draw_gradient.param2 = 0.5 + (dx * half) / w;
            draw_gradient.param3 = 0.5 + (dy * half) / h;
            set_gradient_stops(draw_gradient, items, grad_len, current_abs, opacity);
            draw_gradient.draw_abs(cx, rect);
        }
        G::Radial { items, shape, position, flags, .. } => {
            let cx_pos = position.horizontal.to_used_value(app_units::Au::from_f32_px(w)).to_f32_px();
            let cy_pos = position.vertical.to_used_value(app_units::Au::from_f32_px(h)).to_f32_px();
            let (rx, ry) = resolve_radial_shape(shape, w, h, cx_pos, cy_pos);
            draw_gradient.grad_type = 1.0;
            draw_gradient.repeating = if flags.contains(GradientFlags::REPEATING) { 1.0 } else { 0.0 };
            draw_gradient.param0 = cx_pos / w;
            draw_gradient.param1 = cy_pos / h;
            draw_gradient.param2 = rx / w;
            draw_gradient.param3 = ry / h;
            set_gradient_stops(draw_gradient, items, rx, current_abs, opacity);
            draw_gradient.draw_abs(cx, rect);
        }
        G::Conic { angle, position, items, flags, .. } => {
            let cx_pos = position.horizontal.to_used_value(app_units::Au::from_f32_px(w)).to_f32_px();
            let cy_pos = position.vertical.to_used_value(app_units::Au::from_f32_px(h)).to_f32_px();
            draw_gradient.grad_type = 2.0;
            draw_gradient.repeating = if flags.contains(GradientFlags::REPEATING) { 1.0 } else { 0.0 };
            draw_gradient.param0 = cx_pos / w;
            draw_gradient.param1 = cy_pos / h;
            draw_gradient.param2 = angle.radians();
            draw_gradient.param3 = 0.0;
            set_conic_gradient_stops(draw_gradient, items, current_abs, opacity);
            draw_gradient.draw_abs(cx, rect);
        }
    }
}

fn resolve_radial_shape(
    shape: &style::values::computed::image::EndingShape,
    w: f32, h: f32, cx: f32, cy: f32,
) -> (f32, f32) {
    use style::values::computed::image::EndingShape;
    use style::values::generics::image::{Circle, Ellipse, ShapeExtent};
    match shape {
        EndingShape::Circle(circle) => match circle {
            Circle::Radius(r) => { let r = r.px(); (r, r) }
            Circle::Extent(extent) => {
                let r = match extent {
                    ShapeExtent::ClosestSide => cx.min(cy).min(w - cx).min(h - cy),
                    ShapeExtent::FarthestSide => cx.max(cy).max(w - cx).max(h - cy),
                    ShapeExtent::ClosestCorner => {
                        let (dx, dy) = (cx.min(w - cx), cy.min(h - cy));
                        (dx * dx + dy * dy).sqrt()
                    }
                    ShapeExtent::FarthestCorner | ShapeExtent::Contain | ShapeExtent::Cover => {
                        let (dx, dy) = (cx.max(w - cx), cy.max(h - cy));
                        (dx * dx + dy * dy).sqrt()
                    }
                };
                (r, r)
            }
        }
        EndingShape::Ellipse(ellipse) => match ellipse {
            Ellipse::Radii(rx, ry) => (
                rx.to_used_value(app_units::Au::from_f32_px(w)).to_f32_px(),
                ry.to_used_value(app_units::Au::from_f32_px(h)).to_f32_px(),
            ),
            Ellipse::Extent(extent) => {
                let (dxc, dyc) = (cx.min(w - cx), cy.min(h - cy));
                let (dxf, dyf) = (cx.max(w - cx), cy.max(h - cy));
                match extent {
                    ShapeExtent::ClosestSide => (dxc, dyc),
                    ShapeExtent::FarthestSide => (dxf, dyf),
                    ShapeExtent::ClosestCorner | ShapeExtent::Contain => {
                        let d = (dxc * dxc + dyc * dyc).sqrt();
                        if d < 0.001 { (0.0, 0.0) } else { (dxc * d / dxc.max(0.001), dyc * d / dyc.max(0.001)) }
                    }
                    ShapeExtent::FarthestCorner | ShapeExtent::Cover => {
                        let d = (dxf * dxf + dyf * dyf).sqrt();
                        if d < 0.001 { (0.0, 0.0) } else { (dxf * d / dxf.max(0.001), dyf * d / dyf.max(0.001)) }
                    }
                }
            }
        }
    }
}

fn set_gradient_stops(
    dg: &mut DrawGradient,
    items: &[style::values::generics::image::GradientItem<
        style::values::computed::Color, style::values::computed::LengthPercentage,
    >],
    gradient_length: f32, current_abs: &AbsoluteColor, opacity: f32,
) {
    let mut stops: Vec<(Vec4f, f32)> = Vec::new();
    for item in items {
        match item {
            style::values::generics::image::GradientItem::SimpleColorStop(color) => {
                let mut c = resolve_color(color, current_abs);
                c.w *= opacity;
                stops.push((c, -1.0));
            }
            style::values::generics::image::GradientItem::ComplexColorStop { color, position } => {
                let mut c = resolve_color(color, current_abs);
                c.w *= opacity;
                let pos = position.to_used_value(app_units::Au::from_f32_px(gradient_length)).to_f32_px() / gradient_length;
                stops.push((c, pos));
            }
            _ => {}
        }
    }
    if !stops.is_empty() {
        if stops[0].1 < 0.0 { stops[0].1 = 0.0; }
        let last = stops.len() - 1;
        if stops[last].1 < 0.0 { stops[last].1 = 1.0; }
        let mut i = 0;
        while i < stops.len() {
            if stops[i].1 < 0.0 {
                let start = i - 1;
                let mut end = i + 1;
                while end < stops.len() && stops[end].1 < 0.0 { end += 1; }
                let count = end - start;
                let (p0, p1) = (stops[start].1, stops[end].1);
                for j in (start + 1)..end {
                    stops[j].1 = p0 + (p1 - p0) * ((j - start) as f32) / (count as f32);
                }
                i = end + 1;
            } else { i += 1; }
        }
    }
    if stops.len() > 8 { stops.truncate(8); }
    dg.stop_count = stops.len() as f32;
    macro_rules! set_stop {
        ($i:expr, $c:ident, $p:ident) => { if $i < stops.len() { dg.$c = stops[$i].0; dg.$p = stops[$i].1; } };
    }
    set_stop!(0, stop0_color, stop0_pos); set_stop!(1, stop1_color, stop1_pos);
    set_stop!(2, stop2_color, stop2_pos); set_stop!(3, stop3_color, stop3_pos);
    set_stop!(4, stop4_color, stop4_pos); set_stop!(5, stop5_color, stop5_pos);
    set_stop!(6, stop6_color, stop6_pos); set_stop!(7, stop7_color, stop7_pos);
}

/// Set gradient stops for conic gradients (AngleOrPercentage positions).
fn set_conic_gradient_stops(
    dg: &mut DrawGradient,
    items: &[style::values::generics::image::GradientItem<
        style::values::computed::Color, style::values::computed::AngleOrPercentage,
    >],
    current_abs: &AbsoluteColor, opacity: f32,
) {
    use style::values::computed::AngleOrPercentage;
    let mut stops: Vec<(Vec4f, f32)> = Vec::new();
    for item in items {
        match item {
            style::values::generics::image::GradientItem::SimpleColorStop(color) => {
                let mut c = resolve_color(color, current_abs);
                c.w *= opacity;
                stops.push((c, -1.0));
            }
            style::values::generics::image::GradientItem::ComplexColorStop { color, position } => {
                let mut c = resolve_color(color, current_abs);
                c.w *= opacity;
                let pos = match position {
                    AngleOrPercentage::Percentage(p) => p.0,
                    AngleOrPercentage::Angle(a) => a.degrees() / 360.0,
                };
                stops.push((c, pos));
            }
            _ => {}
        }
    }
    // Same fixup as linear/radial stops.
    if !stops.is_empty() {
        if stops[0].1 < 0.0 { stops[0].1 = 0.0; }
        let last = stops.len() - 1;
        if stops[last].1 < 0.0 { stops[last].1 = 1.0; }
        let mut i = 0;
        while i < stops.len() {
            if stops[i].1 < 0.0 {
                let start = i - 1;
                let mut end = i + 1;
                while end < stops.len() && stops[end].1 < 0.0 { end += 1; }
                let count = end - start;
                let (p0, p1) = (stops[start].1, stops[end].1);
                for j in (start + 1)..end {
                    stops[j].1 = p0 + (p1 - p0) * ((j - start) as f32) / (count as f32);
                }
                i = end + 1;
            } else { i += 1; }
        }
    }
    if stops.len() > 8 { stops.truncate(8); }
    dg.stop_count = stops.len() as f32;
    macro_rules! set_stop {
        ($i:expr, $c:ident, $p:ident) => { if $i < stops.len() { dg.$c = stops[$i].0; dg.$p = stops[$i].1; } };
    }
    set_stop!(0, stop0_color, stop0_pos); set_stop!(1, stop1_color, stop1_pos);
    set_stop!(2, stop2_color, stop2_pos); set_stop!(3, stop3_color, stop3_pos);
    set_stop!(4, stop4_color, stop4_pos); set_stop!(5, stop5_color, stop5_pos);
    set_stop!(6, stop6_color, stop6_pos); set_stop!(7, stop7_color, stop7_pos);
}

fn draw_element_borders(
    cx: &mut Cx2d, computed: &ComputedValues,
    x: f64, y: f64, w: f32, h: f32,
    current_abs: &AbsoluteColor, draw_bg: &mut DrawColor, opacity: f32,
) {
    use style::values::specified::border::BorderStyle;
    let border = computed.get_border();
    let bw = |style: BorderStyle, width: style::values::computed::BorderSideWidth| -> f32 {
        if matches!(style, BorderStyle::None | BorderStyle::Hidden) { 0.0 }
        else { width.0.to_f32_px().max(0.0) }
    };
    let top_s = border.clone_border_top_style();
    let right_s = border.clone_border_right_style();
    let bottom_s = border.clone_border_bottom_style();
    let left_s = border.clone_border_left_style();
    let top_w = bw(top_s, border.clone_border_top_width());
    let right_w = bw(right_s, border.clone_border_right_width());
    let bottom_w = bw(bottom_s, border.clone_border_bottom_width());
    let left_w = bw(left_s, border.clone_border_left_width());

    let mut top_c = resolve_color(&border.clone_border_top_color(), current_abs); top_c.w *= opacity;
    let mut right_c = resolve_color(&border.clone_border_right_color(), current_abs); right_c.w *= opacity;
    let mut bottom_c = resolve_color(&border.clone_border_bottom_color(), current_abs); bottom_c.w *= opacity;
    let mut left_c = resolve_color(&border.clone_border_left_color(), current_abs); left_c.w *= opacity;

    // Top border.
    if top_w > 0.0 {
        draw_border_side(cx, draw_bg, top_s, top_c, x, y, w as f64, top_w as f64, BorderSide::Top);
    }
    // Right border.
    if right_w > 0.0 {
        draw_border_side(cx, draw_bg, right_s, right_c,
            x + (w - right_w) as f64, y, right_w as f64, h as f64, BorderSide::Right);
    }
    // Bottom border.
    if bottom_w > 0.0 {
        draw_border_side(cx, draw_bg, bottom_s, bottom_c,
            x, y + (h - bottom_w) as f64, w as f64, bottom_w as f64, BorderSide::Bottom);
    }
    // Left border.
    if left_w > 0.0 {
        draw_border_side(cx, draw_bg, left_s, left_c, x, y, left_w as f64, h as f64, BorderSide::Left);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum BorderSide { Top, Right, Bottom, Left }

fn draw_border_side(
    cx: &mut Cx2d, draw_bg: &mut DrawColor,
    style: style::values::specified::border::BorderStyle,
    color: Vec4f,
    x: f64, y: f64, w: f64, h: f64,
    side: BorderSide,
) {
    use style::values::specified::border::BorderStyle;
    match style {
        BorderStyle::Solid | BorderStyle::None | BorderStyle::Hidden => {
            draw_bg.color = color;
            draw_bg.draw_abs(cx, Rect { pos: dvec2(x, y), size: dvec2(w, h) });
        }
        BorderStyle::Double => {
            // Outer line, gap, inner line. Each gets 1/3 of the border width.
            let is_horiz = matches!(side, BorderSide::Top | BorderSide::Bottom);
            let thickness = if is_horiz { h } else { w };
            let line = (thickness / 3.0).max(1.0);
            draw_bg.color = color;
            if is_horiz {
                draw_bg.draw_abs(cx, Rect { pos: dvec2(x, y), size: dvec2(w, line) });
                draw_bg.draw_abs(cx, Rect { pos: dvec2(x, y + thickness - line), size: dvec2(w, line) });
            } else {
                draw_bg.draw_abs(cx, Rect { pos: dvec2(x, y), size: dvec2(line, h) });
                draw_bg.draw_abs(cx, Rect { pos: dvec2(x + thickness - line, y), size: dvec2(line, h) });
            }
        }
        BorderStyle::Dotted => {
            let is_horiz = matches!(side, BorderSide::Top | BorderSide::Bottom);
            let thickness = if is_horiz { h } else { w };
            let dot_size = thickness.max(1.0);
            let length = if is_horiz { w } else { h };
            let count = (length / (dot_size * 2.0)).max(1.0) as i32;
            let spacing = length / count as f64;
            draw_bg.color = color;
            for i in 0..count {
                let offset = i as f64 * spacing;
                if is_horiz {
                    draw_bg.draw_abs(cx, Rect { pos: dvec2(x + offset, y), size: dvec2(dot_size.min(spacing * 0.5), h) });
                } else {
                    draw_bg.draw_abs(cx, Rect { pos: dvec2(x, y + offset), size: dvec2(w, dot_size.min(spacing * 0.5)) });
                }
            }
        }
        BorderStyle::Dashed => {
            let is_horiz = matches!(side, BorderSide::Top | BorderSide::Bottom);
            let thickness = if is_horiz { h } else { w };
            let dash_len = (thickness * 3.0).max(1.0);
            let length = if is_horiz { w } else { h };
            let count = (length / (dash_len * 2.0)).max(1.0) as i32;
            let spacing = length / count as f64;
            draw_bg.color = color;
            for i in 0..count {
                let offset = i as f64 * spacing;
                if is_horiz {
                    draw_bg.draw_abs(cx, Rect { pos: dvec2(x + offset, y), size: dvec2(dash_len.min(spacing * 0.5), h) });
                } else {
                    draw_bg.draw_abs(cx, Rect { pos: dvec2(x, y + offset), size: dvec2(w, dash_len.min(spacing * 0.5)) });
                }
            }
        }
        BorderStyle::Groove => {
            // 3D groove: outer half dark, inner half light.
            let (dark, light) = groove_colors(color, side);
            let is_horiz = matches!(side, BorderSide::Top | BorderSide::Bottom);
            let thickness = if is_horiz { h } else { w };
            let half = (thickness / 2.0).max(0.5);
            if is_horiz {
                draw_bg.color = dark; draw_bg.draw_abs(cx, Rect { pos: dvec2(x, y), size: dvec2(w, half) });
                draw_bg.color = light; draw_bg.draw_abs(cx, Rect { pos: dvec2(x, y + half), size: dvec2(w, thickness - half) });
            } else {
                draw_bg.color = dark; draw_bg.draw_abs(cx, Rect { pos: dvec2(x, y), size: dvec2(half, h) });
                draw_bg.color = light; draw_bg.draw_abs(cx, Rect { pos: dvec2(x + half, y), size: dvec2(thickness - half, h) });
            }
        }
        BorderStyle::Ridge => {
            // 3D ridge: outer half light, inner half dark (reverse of groove).
            let (dark, light) = groove_colors(color, side);
            let is_horiz = matches!(side, BorderSide::Top | BorderSide::Bottom);
            let thickness = if is_horiz { h } else { w };
            let half = (thickness / 2.0).max(0.5);
            if is_horiz {
                draw_bg.color = light; draw_bg.draw_abs(cx, Rect { pos: dvec2(x, y), size: dvec2(w, half) });
                draw_bg.color = dark; draw_bg.draw_abs(cx, Rect { pos: dvec2(x, y + half), size: dvec2(w, thickness - half) });
            } else {
                draw_bg.color = light; draw_bg.draw_abs(cx, Rect { pos: dvec2(x, y), size: dvec2(half, h) });
                draw_bg.color = dark; draw_bg.draw_abs(cx, Rect { pos: dvec2(x + half, y), size: dvec2(thickness - half, h) });
            }
        }
        BorderStyle::Inset => {
            // Top/left dark, bottom/right normal.
            let c = if matches!(side, BorderSide::Top | BorderSide::Left) {
                darken(color, 0.6)
            } else { color };
            draw_bg.color = c;
            draw_bg.draw_abs(cx, Rect { pos: dvec2(x, y), size: dvec2(w, h) });
        }
        BorderStyle::Outset => {
            // Top/left normal, bottom/right dark.
            let c = if matches!(side, BorderSide::Bottom | BorderSide::Right) {
                darken(color, 0.6)
            } else { color };
            draw_bg.color = c;
            draw_bg.draw_abs(cx, Rect { pos: dvec2(x, y), size: dvec2(w, h) });
        }
    }
}

/// For groove/ridge: top/left gets dark shade, bottom/right gets light shade.
fn groove_colors(color: Vec4f, side: BorderSide) -> (Vec4f, Vec4f) {
    let dark = darken(color, 0.6);
    let light = lighten(color, 1.4);
    match side {
        BorderSide::Top | BorderSide::Left => (dark, light),
        BorderSide::Bottom | BorderSide::Right => (light, dark),
    }
}

fn darken(c: Vec4f, factor: f32) -> Vec4f {
    Vec4f { x: c.x * factor, y: c.y * factor, z: c.z * factor, w: c.w }
}

fn lighten(c: Vec4f, factor: f32) -> Vec4f {
    Vec4f { x: (c.x * factor).min(1.0), y: (c.y * factor).min(1.0), z: (c.z * factor).min(1.0), w: c.w }
}

fn draw_element_outline(
    cx: &mut Cx2d, computed: &ComputedValues,
    x: f64, y: f64, w: f32, h: f32,
    current_abs: &AbsoluteColor, draw_bg: &mut DrawColor, opacity: f32,
) {
    let outline = computed.get_outline();
    if outline.outline_style.none_or_hidden() { return; }
    let ow = outline.outline_width.0.to_f32_px();
    if ow <= 0.0 { return; }
    let offset = outline.outline_offset.to_f32_px() + ow;
    let mut color = resolve_color(&outline.outline_color, current_abs);
    color.w *= opacity;
    draw_bg.color = color;
    draw_bg.draw_abs(cx, Rect { pos: dvec2(x - offset as f64, y - offset as f64), size: dvec2(w as f64 + 2.0 * offset as f64, ow as f64) });
    draw_bg.draw_abs(cx, Rect { pos: dvec2(x - offset as f64, y + h as f64 + offset as f64 - ow as f64), size: dvec2(w as f64 + 2.0 * offset as f64, ow as f64) });
    draw_bg.draw_abs(cx, Rect { pos: dvec2(x - offset as f64, y - offset as f64 + ow as f64), size: dvec2(ow as f64, h as f64 + 2.0 * (offset - ow) as f64) });
    draw_bg.draw_abs(cx, Rect { pos: dvec2(x + w as f64 + offset as f64 - ow as f64, y - offset as f64 + ow as f64), size: dvec2(ow as f64, h as f64 + 2.0 * (offset - ow) as f64) });
}
