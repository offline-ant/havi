use std::ops::Range;
use app_units::Au;
use base::text::is_bidi_control;
use fonts::{FontMetrics, FontRef, LAST_RESORT_GLYPH_ADVANCE, ShapingFlags, ShapingOptions};
use layout_api::wrapper_traits::ThreadSafeLayoutNode;
use layout_api::SVGNodeKind;
use style::computed_values::text_rendering::T as TextRendering;
use style::dom::NodeInfo;
use style::Zero;
use unicode_bidi::{BidiInfo, Level};
use unicode_script::Script;
use xi_unicode::linebreak_property;

use super::dom::{
    collect_direct_text_content, resolve_svg_child_node, SVGNodeResolvedStyle, SVGResolvedNode,
};
use super::path::parse_svg_length;
use super::style::SVGTextAnchor;
use crate::context::LayoutContext;
use crate::fragment_tree::{BaseFragment, BaseFragmentInfo, TextFragment};
use crate::SharedStyle;

const XI_LINE_BREAKING_CLASS_CM: u8 = 9;
const XI_LINE_BREAKING_CLASS_GL: u8 = 12;
const XI_LINE_BREAKING_CLASS_ZW: u8 = 28;
const XI_LINE_BREAKING_CLASS_WJ: u8 = 30;
const XI_LINE_BREAKING_CLASS_ZWJ: u8 = 42;

#[derive(Default)]
pub(crate) struct SVGTextLayoutResult {
    pub text_runs: Vec<TextFragment>,
    pub object_bounding_box: havi_types::fragment_tree::SVGRect,
    pub decorated_bounding_box: havi_types::fragment_tree::SVGRect,
}

#[derive(Clone, Debug)]
struct SVGTextCursor {
    x: f32,
    y: f32,
}

impl Default for SVGTextCursor {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0 }
    }
}

#[derive(Clone)]
struct SVGTextSegment {
    font: FontRef,
    script: Script,
    bidi_level: Level,
    byte_range: Range<usize>,
}

impl SVGTextSegment {
    fn update_if_compatible(
        &mut self,
        layout_context: &LayoutContext,
        new_font: &FontRef,
        script: Script,
        bidi_level: Level,
    ) -> bool {
        fn is_specific(script: Script) -> bool {
            script != Script::Common && script != Script::Inherited
        }

        if bidi_level != self.bidi_level {
            return false;
        }

        if new_font.key(layout_context.painter_id, &layout_context.font_context) !=
            self.font.key(layout_context.painter_id, &layout_context.font_context) ||
            new_font.descriptor.pt_size != self.font.descriptor.pt_size
        {
            return false;
        }

        if !is_specific(self.script) && is_specific(script) {
            self.script = script;
        }
        script == self.script || !is_specific(script)
    }
}

pub(crate) fn layout_svg_text(
    node: &SVGResolvedNode<'_>,
    layout_context: &LayoutContext,
) -> SVGTextLayoutResult {
    let mut cursor = SVGTextCursor::default();
    let mut text_runs = Vec::new();
    layout_svg_text_node(node, layout_context, &mut cursor, &mut text_runs);
    let object_bounding_box = text_run_bounds(&text_runs).unwrap_or_default();
    SVGTextLayoutResult {
        text_runs,
        object_bounding_box,
        decorated_bounding_box: object_bounding_box,
    }
}

fn layout_svg_text_node(
    node: &SVGResolvedNode<'_>,
    layout_context: &LayoutContext,
    cursor: &mut SVGTextCursor,
    text_runs: &mut Vec<TextFragment>,
) {
    let Some(text_data) = text_node_data(node) else {
        return;
    };

    let mut x = parse_svg_length(text_data.x).unwrap_or(cursor.x);
    let mut y = parse_svg_length(text_data.y).unwrap_or(cursor.y);
    x += parse_svg_length(text_data.dx).unwrap_or(0.0);
    y += parse_svg_length(text_data.dy).unwrap_or(0.0);

    let text_content = collect_direct_text_content(node.node);
    if !text_content.is_empty() {
        let mut shaped_runs = shape_svg_text_runs(node, layout_context, &text_content);
        let total_advance = shaped_runs
            .iter()
            .map(|run| run.glyphs.iter().map(|glyphs| glyphs.total_advance()).sum::<Au>())
            .sum::<Au>();
        let anchored_x = match text_anchor(node) {
            SVGTextAnchor::Start => Au::from_f32_px(x),
            SVGTextAnchor::Middle => Au::from_f32_px(x) - total_advance.scale_by(0.5),
            SVGTextAnchor::End => Au::from_f32_px(x) - total_advance,
        };

        let mut run_x = anchored_x;
        for run in &mut shaped_runs {
            let baseline_y = resolve_baseline_y(run.font_metrics.as_ref(), node, Au::from_f32_px(y));
            let inline_advance: Au = run.glyphs.iter().map(|glyphs| glyphs.total_advance()).sum();
            run.base.rect.origin.x = run_x;
            run.base.rect.origin.y = baseline_y - run.font_metrics.ascent;
            run.base.rect.size.width = inline_advance;
            run.base.rect.size.height = run.font_metrics.line_gap;
            run_x += inline_advance;
        }

        cursor.x = x + total_advance.to_f32_px();
        text_runs.extend(shaped_runs);
    } else {
        cursor.x = x;
    }
    cursor.y = y;

    for child in node.node.children() {
        if !child.is_element() {
            continue;
        }
        let Some(child) = resolve_svg_child_node(child, &layout_context.style_context, node) else {
            continue;
        };
        if matches!(child.svg_data.node_kind, SVGNodeKind::TSpan(_)) {
            layout_svg_text_node(&child, layout_context, cursor, text_runs);
        }
    }
}

fn shape_svg_text_runs(
    node: &SVGResolvedNode<'_>,
    layout_context: &LayoutContext,
    text: &str,
) -> Vec<TextFragment> {
    let bidi_info = BidiInfo::new(text, None);
    let segments = segment_text_by_font(node, layout_context, text, &bidi_info);
    segments
        .into_iter()
        .filter_map(|segment| shape_svg_text_segment(node, layout_context, text, segment))
        .collect()
}

fn segment_text_by_font(
    node: &SVGResolvedNode<'_>,
    layout_context: &LayoutContext,
    text: &str,
    bidi_info: &BidiInfo,
) -> Vec<SVGTextSegment> {
    let font_group = layout_context
        .font_context
        .font_group(node.computed_style.clone_font());
    let lang = node
        .computed_style
        .get_font()
        ._x_lang
        .clone();

    let mut current: Option<SVGTextSegment> = None;
    let mut results = Vec::new();
    let mut current_start = 0;
    let mut iter = text.char_indices().peekable();

    while let Some((index, character)) = iter.next() {
        if char_does_not_change_font(character) {
            continue;
        }

        let script = Script::from(character);
        let bidi_level = bidi_info.levels[index];
        let next_character = iter.peek().map(|(_, next)| *next);
        let Some(font) = font_group.find_by_codepoint(
            &layout_context.font_context,
            character,
            next_character,
            lang.clone(),
        ) else {
            continue;
        };

        if let Some(current) = current.as_mut() {
            if current.update_if_compatible(layout_context, &font, script, bidi_level) {
                continue;
            }
        }

        if let Some(mut finished) = current.take() {
            finished.byte_range.end = index;
            results.push(finished);
            current_start = index;
        }

        current = Some(SVGTextSegment {
            font,
            script,
            bidi_level,
            byte_range: current_start..text.len(),
        });
    }

    if current.is_none() {
        current = font_group
            .first(&layout_context.font_context)
            .map(|font| SVGTextSegment {
                font,
                script: Script::Common,
                bidi_level: Level::ltr(),
                byte_range: 0..text.len(),
            });
    }

    if let Some(mut last) = current {
        last.byte_range.end = text.len();
        results.push(last);
    }

    results
}

fn shape_svg_text_segment(
    node: &SVGResolvedNode<'_>,
    layout_context: &LayoutContext,
    text: &str,
    segment: SVGTextSegment,
) -> Option<TextFragment> {
    let inherited_text_style = node.computed_style.get_inherited_text().clone();
    let letter_spacing = inherited_text_style
        .letter_spacing
        .0
        .resolve(node.computed_style.clone_font().font_size.computed_size());
    let letter_spacing = if letter_spacing.px() != 0.0 {
        Some(Au::from(letter_spacing))
    } else {
        None
    };

    let mut flags = ShapingFlags::empty();
    if inherited_text_style.text_rendering == TextRendering::Optimizespeed {
        flags.insert(ShapingFlags::IGNORE_LIGATURES_SHAPING_FLAG);
        flags.insert(ShapingFlags::DISABLE_KERNING_SHAPING_FLAG);
    }
    if segment.bidi_level.is_rtl() {
        flags.insert(ShapingFlags::RTL_FLAG);
    }

    let letter_spacing = if is_cursive_script(segment.script) {
        None
    } else {
        letter_spacing
    };
    if letter_spacing.is_some() {
        flags.insert(ShapingFlags::IGNORE_LIGATURES_SHAPING_FLAG);
    }

    let specified_word_spacing = &inherited_text_style.word_spacing;
    let word_spacing = specified_word_spacing.to_length().map(Au::from).unwrap_or_else(|| {
        let space_width = segment
            .font
            .glyph_index(' ')
            .map(|glyph_id| segment.font.glyph_h_advance(glyph_id))
            .unwrap_or(LAST_RESORT_GLYPH_ADVANCE);
        specified_word_spacing.to_used_value(Au::from_f64_px(space_width))
    });

    let segment_text = &text[segment.byte_range.clone()];
    if segment_text.is_empty() {
        return None;
    }

    let glyph_store = segment.font.shape_text(
        segment_text,
        &ShapingOptions {
            letter_spacing,
            word_spacing,
            script: segment.script,
            flags,
        },
    );

    let font_key = segment
        .font
        .key(layout_context.painter_id, &layout_context.font_context);
    Some(TextFragment {
        base: BaseFragment::new(
            BaseFragmentInfo::anonymous(),
            node.computed_style.clone().into(),
            crate::geom::PhysicalRect::zero(),
        ),
        text: segment_text.to_string(),
        selected_style: SharedStyle::new(node.computed_style.clone()),
        font_metrics: segment.font.metrics.clone(),
        font_key,
        font: segment.font,
        glyphs: vec![glyph_store],
        justification_adjustment: Au::zero(),
        offsets: None,
    })
}

fn text_node_data<'a>(node: &'a SVGResolvedNode<'a>) -> Option<&'a layout_api::SVGTextData<'a>> {
    match &node.svg_data.node_kind {
        SVGNodeKind::Text(data) | SVGNodeKind::TSpan(data) => Some(data),
        _ => None,
    }
}

fn text_anchor(node: &SVGResolvedNode<'_>) -> SVGTextAnchor {
    match &node.resolved_style {
        SVGNodeResolvedStyle::Text(style) => style.text_anchor,
        _ => SVGTextAnchor::Start,
    }
}

fn resolve_baseline_y(
    font_metrics: &FontMetrics,
    node: &SVGResolvedNode<'_>,
    y: Au,
) -> Au {
    let baseline = match &node.resolved_style {
        SVGNodeResolvedStyle::Text(style) => style
            .alignment_baseline
            .as_deref()
            .or(style.dominant_baseline.as_deref()),
        _ => None,
    };
    match baseline {
        Some("middle") | Some("central") => {
            y + (font_metrics.ascent - font_metrics.descent).scale_by(0.5)
        }
        Some("hanging") | Some("text-before-edge") => y + font_metrics.ascent,
        Some("text-after-edge") => y - font_metrics.descent,
        _ => y,
    }
}

fn text_run_bounds(text_runs: &[TextFragment]) -> Option<havi_types::fragment_tree::SVGRect> {
    let mut rects = text_runs.iter().map(|run| run.base.rect);
    let first = rects.next()?;
    let union = rects.fold(first, |union, rect| union.union(&rect));
    Some(havi_types::fragment_tree::SVGRect::new(
        euclid::point2(union.origin.x.to_f32_px(), union.origin.y.to_f32_px()),
        euclid::size2(union.size.width.to_f32_px(), union.size.height.to_f32_px()),
    ))
}

fn is_cursive_script(script: Script) -> bool {
    matches!(
        script,
        Script::Arabic |
            Script::Hanifi_Rohingya |
            Script::Mandaic |
            Script::Mongolian |
            Script::Nko |
            Script::Phags_Pa |
            Script::Syriac
    )
}

fn char_does_not_change_font(character: char) -> bool {
    if character.is_control() {
        return true;
    }
    if character == '\u{00A0}' {
        return true;
    }
    if is_bidi_control(character) {
        return false;
    }

    let class = linebreak_property(character);
    class == XI_LINE_BREAKING_CLASS_CM ||
        class == XI_LINE_BREAKING_CLASS_GL ||
        class == XI_LINE_BREAKING_CLASS_ZW ||
        class == XI_LINE_BREAKING_CLASS_WJ ||
        class == XI_LINE_BREAKING_CLASS_ZWJ
}
