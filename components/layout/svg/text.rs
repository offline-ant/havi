use havi_types::fragment_tree::{SVGGlyphRun, SVGRect, SVGTransform};
use layout_api::wrapper_traits::ThreadSafeLayoutNode;
use layout_api::SVGNodeKind;
use style::context::SharedStyleContext;
use style::dom::NodeInfo;

use super::dom::{collect_direct_text_content, resolve_svg_child_node, SVGNodeResolvedStyle, SVGResolvedNode};
use super::path::parse_svg_length;
use super::style::SVGTextAnchor;

#[derive(Clone, Debug, Default)]
pub struct SVGTextLayoutResult {
    pub glyph_runs: Vec<SVGGlyphRun>,
    pub object_bounding_box: SVGRect,
    pub decorated_bounding_box: SVGRect,
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

pub fn layout_svg_text(
    node: &SVGResolvedNode<'_>,
    style_context: &SharedStyleContext,
) -> SVGTextLayoutResult {
    let mut cursor = SVGTextCursor::default();
    let mut glyph_runs = Vec::new();
    layout_svg_text_node(node, style_context, &mut cursor, &mut glyph_runs);
    let object_bounding_box = glyph_run_bounds(&glyph_runs).unwrap_or_default();
    SVGTextLayoutResult {
        glyph_runs,
        object_bounding_box,
        decorated_bounding_box: object_bounding_box,
    }
}

fn layout_svg_text_node(
    node: &SVGResolvedNode<'_>,
    style_context: &SharedStyleContext,
    cursor: &mut SVGTextCursor,
    glyph_runs: &mut Vec<SVGGlyphRun>,
) {
    let Some(text_data) = text_node_data(node) else {
        return;
    };

    let font_size = node
        .computed_style
        .get_font()
        .font_size
        .computed_size()
        .px()
        .max(1.0);
    let mut x = parse_svg_length(text_data.x).unwrap_or(cursor.x);
    let mut y = parse_svg_length(text_data.y).unwrap_or(cursor.y);
    x += parse_svg_length(text_data.dx).unwrap_or(0.0);
    y += parse_svg_length(text_data.dy).unwrap_or(0.0);

    let text_content = collect_direct_text_content(node.node);
    if !text_content.is_empty() {
        let advance = estimate_text_advance(&text_content, font_size);
        let origin_x = match text_anchor(node) {
            SVGTextAnchor::Start => x,
            SVGTextAnchor::Middle => x - advance * 0.5,
            SVGTextAnchor::End => x - advance,
        };
        glyph_runs.push(SVGGlyphRun {
            text: text_content,
            origin: euclid::point2(origin_x, resolve_baseline_y(node, y, font_size)),
            advance,
            transform: SVGTransform::identity(),
        });
        cursor.x = x + advance;
    } else {
        cursor.x = x;
    }
    cursor.y = y;

    for child in node.node.children() {
        if !child.is_element() {
            continue;
        }
        let Some(child) = resolve_svg_child_node(child, style_context, node) else {
            continue;
        };
        if matches!(child.svg_data.node_kind, SVGNodeKind::TSpan(_)) {
            layout_svg_text_node(&child, style_context, cursor, glyph_runs);
        }
    }
}

fn text_node_data<'a>(
    node: &'a SVGResolvedNode<'a>,
) -> Option<&'a layout_api::SVGTextData<'a>> {
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

fn resolve_baseline_y(node: &SVGResolvedNode<'_>, y: f32, font_size: f32) -> f32 {
    let baseline = match &node.resolved_style {
        SVGNodeResolvedStyle::Text(style) => style
            .alignment_baseline
            .as_deref()
            .or(style.dominant_baseline.as_deref()),
        _ => None,
    };
    match baseline {
        Some("middle") | Some("central") => y + font_size * 0.35,
        Some("hanging") | Some("text-before-edge") => y + font_size * 0.8,
        Some("text-after-edge") => y - font_size * 0.2,
        _ => y,
    }
}

fn estimate_text_advance(text: &str, font_size: f32) -> f32 {
    text.chars().count() as f32 * font_size * 0.6
}

fn glyph_run_bounds(glyph_runs: &[SVGGlyphRun]) -> Option<SVGRect> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut saw_run = false;

    for run in glyph_runs {
        let font_size = (run.advance / run.text.chars().count().max(1) as f32 / 0.6).max(1.0);
        let ascent = font_size * 0.8;
        let descent = font_size * 0.2;
        let top = run.origin.y - ascent;
        let bottom = run.origin.y + descent;
        min_x = min_x.min(run.origin.x);
        min_y = min_y.min(top);
        max_x = max_x.max(run.origin.x + run.advance);
        max_y = max_y.max(bottom);
        saw_run = true;
    }

    saw_run.then(|| {
        SVGRect::new(
            euclid::point2(min_x, min_y),
            euclid::size2(max_x - min_x, max_y - min_y),
        )
    })
}
