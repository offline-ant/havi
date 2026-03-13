//! Text rendering: glyphs, decorations, font selection.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use havi_fonts::FontHandle;
use havi_types::ShapedGlyph;
use makepad_widgets::*;
use makepad_widgets::makepad_draw::text::font::FontId;
use makepad_widgets::makepad_draw::text::geom::Point as TextPoint;
use makepad_widgets::makepad_draw::text::loader::{FontDefinition, FontFamilyDefinition};
use makepad_widgets::makepad_draw::text::font_family::FontFamilyId;
use style::color::{AbsoluteColor, ColorSpace};
use style::properties::ComputedValues;
use style::values::computed::font::{GenericFontFamily, SingleFontFamily};

use crate::color::{abs_to_vec4, inherited_color, resolve_color};

thread_local! {
    /// Maps (path, index) → Makepad FontId for fonts registered during this session.
    static REGISTERED_FONTS: RefCell<HashMap<(PathBuf, u32), FontId>> = RefCell::new(HashMap::new());
    static NEXT_FONT_ID: RefCell<u64> = RefCell::new(0x1000_0000);
}

/// Ensure a font handle is registered with Makepad and return its FontId.
/// When `preloaded_data` is provided, uses it directly instead of reading from disk.
fn ensure_font_registered(
    cx: &mut Cx2d,
    handle: &FontHandle,
    preloaded_data: Option<&havi_fonts::FontData>,
) -> Option<FontId> {
    let key = (handle.path.clone(), handle.index);

    let existing = REGISTERED_FONTS.with(|map| map.borrow().get(&key).copied());
    if let Some(id) = existing {
        return Some(id);
    }

    // Use pre-loaded data if available, otherwise fall back to disk read.
    let font_data = match preloaded_data {
        Some(data) => SharedBytes::Owned((**data).clone().into()),
        None => SharedBytes::Owned(std::fs::read(&handle.path).ok()?.into()),
    };

    let font_id = NEXT_FONT_ID.with(|cell| {
        let id = *cell.borrow();
        *cell.borrow_mut() = id + 1;
        FontId::from(id)
    });

    let fonts_rc = cx.cx.get_global::<Rc<RefCell<makepad_widgets::makepad_draw::text::fonts::Fonts>>>().clone();
    let mut fonts = fonts_rc.borrow_mut();

    if !fonts.is_font_known(font_id) {
        fonts.define_font(font_id, FontDefinition {
            data: font_data,
            index: handle.index,
            ascender_fudge_in_ems: 0.0,
            descender_fudge_in_ems: 0.0,
            variations: Vec::new(),
        });
    }

    REGISTERED_FONTS.with(|map| map.borrow_mut().insert(key, font_id));
    Some(font_id)
}

/// Get or create a Makepad FontFamilyId with a single font.
fn ensure_font_family(cx: &mut Cx2d, font_id: FontId) -> FontFamilyId {
    // Use a deterministic family id. We hash the FontId to get a stable u64.
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    font_id.hash(&mut hasher);
    let family_id = FontFamilyId::from(hasher.finish());

    let fonts_rc = cx.cx.get_global::<Rc<RefCell<makepad_widgets::makepad_draw::text::fonts::Fonts>>>().clone();
    let mut fonts = fonts_rc.borrow_mut();

    if !fonts.is_font_family_known(family_id) {
        fonts.define_font_family(family_id, FontFamilyDefinition {
            font_ids: vec![font_id],
            expected_member_count: 1,
        });
    }

    family_id
}

pub(crate) fn draw_text_run(
    cx: &mut Cx2d,
    tf: &havi_types::TextFragment,
    x: f64,
    y: f64,
    w: f32,
    h: f32,
    opacity: f32,
    draw_bg: &mut DrawColor,
    draw_text: &mut DrawText,
    draw_text_bold: &mut DrawText,
    draw_text_mono: &mut DrawText,
) {
    let computed = &tf.base.style;
    let mut color = inherited_color(computed);
    color.w *= opacity;

    let mut bg = resolve_color(
        &computed.get_background().background_color,
        &computed.get_inherited_text().color,
    );
    bg.w *= opacity;
    if bg.w > 0.001 && w > 0.0 && h > 0.0 {
        draw_bg.color = bg;
        draw_bg.draw_abs(cx, Rect { pos: dvec2(x, y), size: dvec2(w as f64, h as f64) });
    }

    let font_size = computed.get_font().font_size.computed_size().px();
    let baseline_ascent_px = tf.baseline_ascent.to_f32_px();

    // Text shadows
    let shadows = &computed.get_inherited_text().text_shadow.0;
    if !shadows.is_empty() {
        for shadow in shadows.iter().rev() {
            let h_off = shadow.horizontal.px() as f64;
            let v_off = shadow.vertical.px() as f64;
            let mut shadow_color = abs_to_vec4(&shadow.color.resolve_to_absolute(&computed.get_inherited_text().color));
            shadow_color.w *= opacity;
            if shadow_color.w < 0.001 { continue; }
            draw_text_at(cx, tf, x + h_off, y + v_off, w, h, font_size, baseline_ascent_px,
                shadow_color, draw_bg, draw_text, draw_text_bold, draw_text_mono);
        }
    }

    let deco_line = computed.get_text().clone_text_decoration_line();
    let has_decorations = !deco_line.is_empty();

    if has_decorations {
        let deco_color = decoration_color(computed, color);
        let baseline_y = y + baseline_ascent_px as f64;

        if deco_line.contains(style::values::specified::TextDecorationLine::UNDERLINE) {
            let ul_y = baseline_y + tf.underline_offset.to_f32_px() as f64;
            let ul_h = tf.underline_size.to_f32_px() as f64;
            draw_bg.color = deco_color;
            draw_bg.draw_abs(cx, Rect { pos: dvec2(x, ul_y), size: dvec2(w as f64, ul_h) });
        }

        if deco_line.contains(style::values::specified::TextDecorationLine::OVERLINE) {
            let ol_h = tf.underline_size.to_f32_px() as f64;
            draw_bg.color = deco_color;
            draw_bg.draw_abs(cx, Rect { pos: dvec2(x, y), size: dvec2(w as f64, ol_h) });
        }
    }

    draw_text_at(cx, tf, x, y, w, h, font_size, baseline_ascent_px,
        color, draw_bg, draw_text, draw_text_bold, draw_text_mono);

    if has_decorations && deco_line.contains(style::values::specified::TextDecorationLine::LINE_THROUGH) {
        let deco_color = decoration_color(computed, color);
        let baseline_y = y + baseline_ascent_px as f64;
        let lt_y = baseline_y - tf.strikeout_offset.to_f32_px() as f64;
        let lt_h = tf.strikeout_size.to_f32_px() as f64;
        draw_bg.color = deco_color;
        draw_bg.draw_abs(cx, Rect { pos: dvec2(x, lt_y), size: dvec2(w as f64, lt_h) });
    }
}

fn draw_text_at(
    cx: &mut Cx2d,
    tf: &havi_types::TextFragment,
    x: f64, y: f64, _w: f32, h: f32,
    font_size: f32, baseline_ascent_px: f32,
    color: Vec4f,
    draw_bg: &mut DrawColor,
    draw_text: &mut DrawText,
    draw_text_bold: &mut DrawText,
    draw_text_mono: &mut DrawText,
) {
    let computed = &tf.base.style;
    if uses_ahem_font(computed) {
        let advance = font_size as f64;
        let mut glyph_x = x;
        for ch in tf.text.chars() {
            if !ch.is_whitespace() {
                draw_bg.color = color;
                draw_bg.draw_abs(cx, Rect { pos: dvec2(glyph_x, y), size: dvec2(advance, h as f64) });
            }
            glyph_x += advance;
        }
    } else if !tf.glyphs.is_empty() {
        // Use font_handle if available, otherwise fall back to DrawText's built-in font
        if let Some(ref handle) = tf.font_handle {
            if let Some(font_id) = ensure_font_registered(cx, handle, tf.font_data.as_ref()) {
                let family_id = ensure_font_family(cx, font_id);
                draw_positioned_glyphs_with_font(cx, draw_text, &tf.glyphs, font_size, x, y,
                    baseline_ascent_px, color, family_id);
                return;
            }
        }
        // Fallback: use the default DrawText font
        let dt = select_draw_text(computed, draw_text, draw_text_bold, draw_text_mono);
        const PX_TO_PT: f32 = 72.0 / 96.0;
        dt.text_style.font_size = font_size * PX_TO_PT;
        dt.color = color;
        draw_positioned_glyphs(cx, dt, &tf.glyphs, font_size, x, y, baseline_ascent_px);
    } else {
        // No glyphs (e.g., plain text without shaping) — use DrawText
        let dt = select_draw_text(computed, draw_text, draw_text_bold, draw_text_mono);
        const PX_TO_PT: f32 = 72.0 / 96.0;
        dt.text_style.font_size = font_size * PX_TO_PT;
        dt.color = color;
        let baseline_y = y + baseline_ascent_px as f64;
        dt.draw_abs(cx, Vec2d { x, y: baseline_y }, &tf.text);
    }
}

fn select_draw_text<'a>(
    computed: &ComputedValues,
    draw_text: &'a mut DrawText,
    draw_text_bold: &'a mut DrawText,
    draw_text_mono: &'a mut DrawText,
) -> &'a mut DrawText {
    if uses_monospace_font(computed) {
        draw_text_mono
    } else if computed.get_font().clone_font_weight().is_bold() {
        draw_text_bold
    } else {
        draw_text
    }
}

fn decoration_color(computed: &ComputedValues, inherited: Vec4f) -> Vec4f {
    let current_abs = AbsoluteColor::new(ColorSpace::Srgb, inherited.x, inherited.y, inherited.z, inherited.w);
    resolve_color(&computed.get_text().clone_text_decoration_color(), &current_abs)
}

fn draw_positioned_glyphs_with_font(
    cx: &mut Cx2d,
    dt: &mut DrawText,
    glyphs: &[ShapedGlyph],
    font_size_px: f32,
    x: f64,
    y: f64,
    baseline_ascent_px: f32,
    color: Vec4f,
    family_id: FontFamilyId,
) {
    let dpi_factor = cx.current_dpi_factor() as f32;
    let dpxs_per_em = font_size_px * dpi_factor;
    let baseline_y = (y as f32) + baseline_ascent_px;

    let fonts_rc = cx.cx.get_global::<Rc<RefCell<makepad_widgets::makepad_draw::text::fonts::Fonts>>>().clone();
    let font_rc = {
        let mut fonts = fonts_rc.borrow_mut();
        let family = fonts.get_or_load_font_family(family_id);
        match family.fonts().first() {
            Some(f) => f.clone(),
            None => return,
        }
    };

    let mut rasterized_glyphs = Vec::with_capacity(glyphs.len());
    let mut pen_x = x as f32;

    for glyph in glyphs {
        let gx = pen_x + glyph.x_offset.to_f32_px();
        let gy = baseline_y + glyph.y_offset.to_f32_px();
        if let Some(rasterized) = font_rc.rasterize_glyph(glyph.glyph_id as u16, dpxs_per_em) {
            rasterized_glyphs.push((TextPoint::new(gx, gy), font_size_px, rasterized));
        }
        pen_x += glyph.advance.to_f32_px();
    }

    if !rasterized_glyphs.is_empty() {
        dt.color = color;
        dt.draw_rasterized_glyphs_abs(cx, &rasterized_glyphs, color);
    }
}

fn draw_positioned_glyphs(
    cx: &mut Cx2d,
    dt: &mut DrawText,
    glyphs: &[ShapedGlyph],
    font_size_px: f32,
    x: f64,
    y: f64,
    baseline_ascent_px: f32,
) {
    let dpi_factor = cx.current_dpi_factor() as f32;
    let dpxs_per_em = font_size_px * dpi_factor;
    let baseline_y = (y as f32) + baseline_ascent_px;

    dt.text_style.ensure_fonts_loaded(cx.cx.cx);
    let fonts_rc = cx.cx.get_global::<Rc<RefCell<makepad_widgets::makepad_draw::text::fonts::Fonts>>>().clone();
    let font_family_id = dt.text_style.font_family_id();
    let font_rc = {
        let mut fonts = fonts_rc.borrow_mut();
        let family = fonts.get_or_load_font_family(font_family_id);
        match family.fonts().first() {
            Some(f) => f.clone(),
            None => return,
        }
    };

    let mut rasterized_glyphs = Vec::with_capacity(glyphs.len());
    let mut pen_x = x as f32;

    for glyph in glyphs {
        let gx = pen_x + glyph.x_offset.to_f32_px();
        let gy = baseline_y + glyph.y_offset.to_f32_px();
        if let Some(rasterized) = font_rc.rasterize_glyph(glyph.glyph_id as u16, dpxs_per_em) {
            rasterized_glyphs.push((TextPoint::new(gx, gy), font_size_px, rasterized));
        }
        pen_x += glyph.advance.to_f32_px();
    }

    if !rasterized_glyphs.is_empty() {
        dt.draw_rasterized_glyphs_abs(cx, &rasterized_glyphs, dt.color);
    }
}

fn uses_ahem_font(computed: &ComputedValues) -> bool {
    computed.get_font().font_family.families.iter().any(|family| match family {
        SingleFontFamily::FamilyName(name) => name.name.as_ref().eq_ignore_ascii_case("ahem"),
        _ => false,
    })
}

fn uses_monospace_font(computed: &ComputedValues) -> bool {
    computed.get_font().font_family.families.iter().any(|family| match family {
        SingleFontFamily::Generic(generic) => *generic == GenericFontFamily::Monospace,
        SingleFontFamily::FamilyName(name) => {
            let n = name.name.as_ref();
            n.eq_ignore_ascii_case("monospace")
                || n.eq_ignore_ascii_case("Courier New")
                || n.eq_ignore_ascii_case("Courier")
        }
    })
}
