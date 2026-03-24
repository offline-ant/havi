use havi_fragment_semantics::fragment_tree::TextFragment;
use makepad_browser_scene::{
    MpClipChainId, MpGlyphRunKey, MpGlyphRunMetrics, MpGlyphRunResource, MpHitTestTag,
    MpPositionedGlyph, MpPrimitive, MpResourceStore, MpTextDecorations, MpTextShadow,
};
use makepad_widgets::{dvec2, Cx2d, Rect};
use style::color::{AbsoluteColor, ColorSpace};
use style::values::specified::TextDecorationLine;

use super::resources::{font_resource_for_text, hash_value};
use crate::color::{inherited_color, resolve_color};

pub(super) fn lower_text_primitive(
    cx: &mut Cx2d,
    resources: &mut MpResourceStore,
    owner_node_id: Option<usize>,
    bounds: Rect,
    tf: &TextFragment,
    spatial_id: makepad_browser_scene::MpSpatialId,
    clip_chain_id: MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
) -> Result<MpPrimitive, String> {
    let (glyph_run_key, glyph_run) = make_glyph_run_resource(cx, owner_node_id, bounds, tf)?;
    resources.glyph_runs.entry(glyph_run_key).or_insert(glyph_run);
    let (font_key, font_resource) = font_resource_for_text(cx, tf)?;
    resources.fonts.entry(font_key).or_insert(font_resource);

    let mut primitive = MpPrimitive::text_run(
        makepad_browser_scene::MpPrimitiveId(0),
        spatial_id,
        clip_chain_id,
        bounds,
        glyph_run_key,
        inherited_color(&tf.base.style),
    );
    primitive.effect_id = effect_id;
    primitive.hit_test_tag = owner_node_id.map(|id| MpHitTestTag(id as u64));
    Ok(primitive)
}

/// Builds a glyph run resource with primitive-local glyph origins.
///
/// Glyph origins are relative to the text fragment bounds, not to any host,
/// widget, or scene space. `origin.x = pen_x + x_offset` and
/// `origin.y = baseline_ascent + y_offset`.
///
/// These origins MUST NOT encode host placement, widget offsets, or
/// scene-space resolution.
fn make_glyph_run_resource(
    cx: &mut Cx2d,
    owner_node_id: Option<usize>,
    bounds: Rect,
    tf: &TextFragment,
) -> Result<(MpGlyphRunKey, MpGlyphRunResource), String> {
    if tf.glyphs.is_empty() {
        return Err("unshaped text not supported by browser-scene adapter yet".to_string());
    }
    let (font_key, _) = font_resource_for_text(cx, tf)?;
    let glyph_run_key = MpGlyphRunKey(hash_value(&(
        owner_node_id,
        tf.text.as_str(),
        tf.base.rect.origin.x.0,
        tf.base.rect.origin.y.0,
        tf.base.rect.size.width.0,
        tf.base.rect.size.height.0,
    )));

    let mut pen_x = 0.0_f64;
    let mut advance_width = 0.0_f32;
    let glyphs = tf
        .glyphs
        .iter()
        .map(|glyph| {
            let origin = dvec2(
                pen_x + glyph.x_offset.to_f32_px() as f64,
                tf.baseline_ascent.to_f32_px() as f64 + glyph.y_offset.to_f32_px() as f64,
            );
            let origin_margin = (tf.font_size_px as f64).max(64.0);
            debug_assert!(
                origin.x >= -origin_margin
                    && origin.x <= bounds.size.x + origin_margin
                    && origin.y >= 0.0
                    && origin.y <= bounds.size.y + origin_margin,
                "glyph origin must stay primitive-local: origin=({}, {}), bounds.size=({}, {}), margin={}",
                origin.x,
                origin.y,
                bounds.size.x,
                bounds.size.y,
                origin_margin,
            );
            pen_x += glyph.advance.to_f32_px() as f64;
            advance_width = advance_width.max((origin.x + glyph.advance.to_f32_px() as f64) as f32);
            MpPositionedGlyph {
                glyph_id: glyph.glyph_id,
                font_size_px: tf.font_size_px,
                origin,
                font_slot: 0,
            }
        })
        .collect();

    let computed = &tf.base.style;
    let current = inherited_color(computed);
    let current_abs = AbsoluteColor::new(ColorSpace::Srgb, current.x, current.y, current.z, current.w);
    let background = resolve_color(
        &computed.get_background().background_color,
        &computed.get_inherited_text().color,
    );
    let decoration_color = resolve_color(&computed.get_text().clone_text_decoration_color(), &current_abs);
    let line = computed.get_text().clone_text_decoration_line();
    let shadows = computed
        .get_inherited_text()
        .text_shadow
        .0
        .iter()
        .map(|shadow| MpTextShadow {
            offset: dvec2(shadow.horizontal.px() as f64, shadow.vertical.px() as f64),
            blur_radius_px: shadow.blur.px(),
            color: resolve_color(
                &shadow.color,
                &computed.get_inherited_text().color,
            ),
        })
        .collect();

    Ok((
        glyph_run_key,
        MpGlyphRunResource {
            text: tf.text.clone(),
            font_keys: vec![font_key],
            glyphs,
            metrics: MpGlyphRunMetrics {
                advance_width_px: advance_width.min(bounds.size.x as f32),
                baseline_ascent_px: tf.baseline_ascent.to_f32_px(),
                underline_offset_px: tf.underline_offset.to_f32_px(),
                underline_thickness_px: tf.underline_size.to_f32_px(),
                strikeout_offset_px: tf.strikeout_offset.to_f32_px(),
                strikeout_thickness_px: tf.strikeout_size.to_f32_px(),
            },
            decorations: MpTextDecorations {
                background_color: (background.w > 0.001).then_some(background),
                decoration_color: Some(decoration_color),
                underline: line.contains(TextDecorationLine::UNDERLINE),
                overline: line.contains(TextDecorationLine::OVERLINE),
                line_through: line.contains(TextDecorationLine::LINE_THROUGH),
                shadows,
            },
        },
    ))
}
