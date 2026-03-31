use std::ops::Range;
use std::sync::Arc;

use app_units::Au;
use base::id::PainterId;
use base::text::is_bidi_control;
use crate::fonts::{
    FontContext, FontMetrics, FontRef, LAST_RESORT_GLYPH_ADVANCE, ShapingFlags, ShapingOptions,
};
use crate::layout::{SVGNodeKind, SVGTextBaselineValue};
use style::Zero;
use style::computed_values::text_rendering::T as TextRendering;
use style::dom::OpaqueNode;
use unicode_bidi::{BidiInfo, Level};
use unicode_script::Script;
use xi_unicode::linebreak_property;

use super::dom::SVGNodeResolvedStyle;
use super::tree::{SVGOwnedNodeKind, SVGResolvedChild, SVGResolvedNode, SVGResolvedNodeMap};
use super::path::{
    normalize_svg_geometry, resolve_length, svg_path_point_and_tangent_at_length,
    svg_path_total_length, transform_svg_path_data,
};
use super::resources::SVGResourceGraph;
use super::transform::{parse_svg_transform, then_svg_transform};
use crate::layout::context::LayoutContext;
use havi_types::fragment_tree::{
    SVGAddressableChar, SVGBounds, SVGGlyphRun, SVGPoint, SVGTextAnchor, SVGTextChunk,
    SVGTextPayload, ShapedGlyph,
};

const XI_LINE_BREAKING_CLASS_CM: u8 = 9;
const XI_LINE_BREAKING_CLASS_GL: u8 = 12;
const XI_LINE_BREAKING_CLASS_ZW: u8 = 28;
const XI_LINE_BREAKING_CLASS_WJ: u8 = 30;
const XI_LINE_BREAKING_CLASS_ZWJ: u8 = 42;

#[derive(Default)]
pub(crate) struct SVGTextLayoutResult {
    pub payload: SVGTextPayload,
    pub bounds: SVGBounds,
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
        text_layout_context: &SVGTextLayoutContext,
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

        if new_font.key(text_layout_context.painter_id, &text_layout_context.font_context) !=
            self.font.key(text_layout_context.painter_id, &text_layout_context.font_context) ||
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

#[derive(Clone)]
struct ShapedSVGRun {
    run: SVGGlyphRun,
    font_metrics: std::sync::Arc<FontMetrics>,
}

#[derive(Clone)]
pub(crate) struct SVGTextLayoutContext {
    pub font_context: Arc<FontContext>,
    pub painter_id: PainterId,
}

impl From<&LayoutContext<'_>> for SVGTextLayoutContext {
    fn from(layout_context: &LayoutContext<'_>) -> Self {
        Self {
            font_context: layout_context.font_context.clone(),
            painter_id: layout_context.painter_id,
        }
    }
}

pub(crate) fn layout_svg_text(
    node: &SVGResolvedNode,
    text_layout_context: &SVGTextLayoutContext,
    resource_graph: &SVGResourceGraph,
    nodes_by_opaque: &SVGResolvedNodeMap<'_>,
) -> SVGTextLayoutResult {
    let Some(text_data) = text_node_data(node) else {
        return SVGTextLayoutResult::default();
    };

    let mut state = SVGTextLayoutState::default();
    let mut contexts = vec![SVGTextPositioningContext::new(text_data)];
    layout_svg_text_container(
        node,
        text_layout_context,
        resource_graph,
        nodes_by_opaque,
        &mut contexts,
        &mut state,
    );
    state.finish(node)
}

#[derive(Clone, Copy, Debug, Default)]
struct SVGTextCharDirective {
    absolute_x: Option<f32>,
    absolute_y: Option<f32>,
    dx: f32,
    dy: f32,
    rotation: f32,
    starts_new_chunk: bool,
}

#[derive(Clone, Debug, Default)]
struct SVGTextSegmentPlan {
    text: String,
    directives: Vec<SVGTextCharDirective>,
    starts_new_chunk: bool,
}

#[derive(Clone, Debug)]
struct SVGTextPositioningContext {
    x: Vec<crate::layout::SVGLengthValue>,
    y: Vec<crate::layout::SVGLengthValue>,
    dx: Vec<crate::layout::SVGLengthValue>,
    dy: Vec<crate::layout::SVGLengthValue>,
    rotate: Vec<f32>,
    x_index: usize,
    y_index: usize,
    dx_index: usize,
    dy_index: usize,
    rotate_index: usize,
}

impl SVGTextPositioningContext {
    fn new(text: &crate::layout::SVGTextData) -> Self {
        Self {
            x: text.x.clone(),
            y: text.y.clone(),
            dx: text.dx.clone(),
            dy: text.dy.clone(),
            rotate: text.rotate.clone(),
            x_index: 0,
            y_index: 0,
            dx_index: 0,
            dy_index: 0,
            rotate_index: 0,
        }
    }

    fn has_x(&self) -> bool {
        !self.x.is_empty()
    }

    fn has_y(&self) -> bool {
        !self.y.is_empty()
    }

    fn has_dx(&self) -> bool {
        !self.dx.is_empty()
    }

    fn has_dy(&self) -> bool {
        !self.dy.is_empty()
    }

    fn has_rotate(&self) -> bool {
        !self.rotate.is_empty()
    }

    fn consume_x(&mut self) -> Option<f32> {
        let value = self.x.get(self.x_index).copied();
        self.x_index = self.x_index.saturating_add(1);
        value.and_then(|value| resolve_length(Some(value)))
    }

    fn consume_y(&mut self) -> Option<f32> {
        let value = self.y.get(self.y_index).copied();
        self.y_index = self.y_index.saturating_add(1);
        value.and_then(|value| resolve_length(Some(value)))
    }

    fn consume_dx(&mut self) -> Option<f32> {
        let value = self.dx.get(self.dx_index).copied();
        self.dx_index = self.dx_index.saturating_add(1);
        value.and_then(|value| resolve_length(Some(value)))
    }

    fn consume_dy(&mut self) -> Option<f32> {
        let value = self.dy.get(self.dy_index).copied();
        self.dy_index = self.dy_index.saturating_add(1);
        value.and_then(|value| resolve_length(Some(value)))
    }

    fn consume_rotate(&mut self) -> Option<f32> {
        if self.rotate.is_empty() {
            return None;
        }
        let value = self
            .rotate
            .get(self.rotate_index)
            .copied()
            .or_else(|| self.rotate.last().copied());
        self.rotate_index = self.rotate_index.saturating_add(1);
        value
    }
}

#[derive(Clone, Copy, Debug)]
struct SVGTextRange {
    start_run: usize,
    end_run: usize,
    start_address: usize,
    end_address: usize,
    start_cursor_x: f32,
}

#[derive(Clone, Debug)]
struct SVGTextLayoutState {
    cursor: SVGTextCursor,
    runs: Vec<SVGGlyphRun>,
    chunks: Vec<SVGTextChunk>,
    addressing: Vec<SVGAddressableChar>,
    current_chunk_start_run: Option<u32>,
    current_chunk_anchor: SVGTextAnchor,
    current_chunk_address_start: usize,
    current_chunk_origin_x: f32,
}

impl Default for SVGTextLayoutState {
    fn default() -> Self {
        Self {
            cursor: SVGTextCursor::default(),
            runs: Vec::new(),
            chunks: Vec::new(),
            addressing: Vec::new(),
            current_chunk_start_run: None,
            current_chunk_anchor: SVGTextAnchor::Start,
            current_chunk_address_start: 0,
            current_chunk_origin_x: 0.0,
        }
    }
}

impl SVGTextLayoutState {
    fn begin_chunk(&mut self, anchor: SVGTextAnchor, origin_x: f32) {
        self.finish_current_chunk();
        self.current_chunk_start_run = Some(self.runs.len() as u32);
        self.current_chunk_anchor = anchor;
        self.current_chunk_address_start = self.addressing.len();
        self.current_chunk_origin_x = origin_x;
    }

    fn ensure_chunk(&mut self, anchor: SVGTextAnchor, origin_x: f32) {
        if self.current_chunk_start_run.is_none() {
            self.current_chunk_start_run = Some(self.runs.len() as u32);
            self.current_chunk_anchor = anchor;
            self.current_chunk_address_start = self.addressing.len();
            self.current_chunk_origin_x = origin_x;
        }
    }

    fn finish_current_chunk(&mut self) {
        let Some(start) = self.current_chunk_start_run.take() else {
            return;
        };
        let end = self.runs.len() as u32;
        if end <= start {
            return;
        }

        let total_advance = self.cursor.x - self.current_chunk_origin_x;
        let shift_x = match self.current_chunk_anchor {
            SVGTextAnchor::Start => 0.0,
            SVGTextAnchor::Middle => -total_advance * 0.5,
            SVGTextAnchor::End => -total_advance,
        };
        if shift_x != 0.0 {
            let shift = Au::from_f32_px(shift_x);
            for run in &mut self.runs[start as usize..end as usize] {
                run.rect.origin.x += shift;
            }
            for addressable in &mut self.addressing[self.current_chunk_address_start..] {
                addressable.position.x += shift_x;
            }
        }

        self.chunks.push(SVGTextChunk {
            run_range: start..end,
            anchor: self.current_chunk_anchor,
        });
    }

    fn finish(mut self, node: &SVGResolvedNode) -> SVGTextLayoutResult {
        self.finish_current_chunk();
        let object_bounding_box = text_run_bounds(&self.runs).unwrap_or_default();
        let stroke_bounding_box = inflate_text_bounds(
            object_bounding_box,
            text_stroke_half_width(node).unwrap_or(0.0),
        );
        let decorated_bounding_box = stroke_bounding_box;
        let visual_bounding_box = decorated_bounding_box;
        SVGTextLayoutResult {
            payload: SVGTextPayload {
                runs: self.runs,
                chunks: self.chunks,
                addressing: self.addressing,
            },
            bounds: SVGBounds {
                object_bounding_box,
                stroke_bounding_box,
                decorated_bounding_box,
                visual_bounding_box,
            },
        }
    }
}

fn layout_svg_text_container(
    node: &SVGResolvedNode,
    text_layout_context: &SVGTextLayoutContext,
    resource_graph: &SVGResourceGraph,
    nodes_by_opaque: &SVGResolvedNodeMap<'_>,
    contexts: &mut Vec<SVGTextPositioningContext>,
    state: &mut SVGTextLayoutState,
) {
    let range_start = SVGTextRange {
        start_run: state.runs.len(),
        end_run: 0,
        start_address: state.addressing.len(),
        end_address: 0,
        start_cursor_x: state.cursor.x,
    };

    for child in &node.children {
        match child {
            SVGResolvedChild::Text(text) => {
                layout_svg_text_content(node, text_layout_context, text, contexts, state);
            }
            SVGResolvedChild::Node(child) => {
                let child_svg_data = child.svg_data();
                if matches!(child_svg_data.node_kind, SVGNodeKind::TextPath(_)) {
                    state.finish_current_chunk();
                }
                if matches!(
                    child_svg_data.node_kind,
                    SVGNodeKind::Text(_) | SVGNodeKind::TSpan(_) | SVGNodeKind::TextPath(_)
                ) {
                    let Some(text_data) = text_node_data(child) else {
                        continue;
                    };
                    contexts.push(SVGTextPositioningContext::new(text_data));
                    layout_svg_text_container(
                        child,
                        text_layout_context,
                        resource_graph,
                        nodes_by_opaque,
                        contexts,
                        state,
                    );
                    contexts.pop();
                    if matches!(child_svg_data.node_kind, SVGNodeKind::TextPath(_)) {
                        state.finish_current_chunk();
                    }
                }
            }
        }
    }

    let range = SVGTextRange {
        end_run: state.runs.len(),
        end_address: state.addressing.len(),
        ..range_start
    };
    apply_text_length_adjustment(node, range, state);
    apply_text_path_layout(node, range, resource_graph, nodes_by_opaque, state);
}

fn layout_svg_text_content(
    node: &SVGResolvedNode,
    text_layout_context: &SVGTextLayoutContext,
    text: &str,
    contexts: &mut Vec<SVGTextPositioningContext>,
    state: &mut SVGTextLayoutState,
) {
    if text.is_empty() {
        return;
    }

    let directives = collect_char_directives(text, contexts);
    let segments = split_text_segments_by_chunk_boundaries(text, &directives);
    let anchor = text_anchor(node);

    for segment in segments {
        if segment.text.is_empty() {
            continue;
        }
        let shaped_runs = shape_svg_text_runs(node, text_layout_context, &segment.text);
        if shaped_runs.is_empty() {
            continue;
        }

        let chunk_origin_x = segment_chunk_origin_x(&segment, state.cursor.x);
        if segment.starts_new_chunk {
            state.begin_chunk(anchor, chunk_origin_x);
        } else {
            state.ensure_chunk(anchor, chunk_origin_x);
        }
        let chunk_address_start = state.current_chunk_address_start;

        let mut directive_offset = 0usize;
        for shaped in shaped_runs {
            let run_char_count = shaped.run.text.chars().count();
            let directive_end = (directive_offset + run_char_count).min(segment.directives.len());
            let run_directives = &segment.directives[directive_offset..directive_end];
            directive_offset = directive_end;
            let run_index = state.runs.len() as u32;
            let run = position_shaped_svg_run(
                node,
                run_index,
                shaped.run,
                shaped.font_metrics.as_ref(),
                run_directives,
                &mut state.cursor,
                &mut state.addressing,
                chunk_address_start,
            );
            state.runs.push(run);
        }
    }
}

fn collect_char_directives(
    text: &str,
    contexts: &mut [SVGTextPositioningContext],
) -> Vec<SVGTextCharDirective> {
    text.chars()
        .map(|_| {
            let absolute_x = consume_context_x(contexts);
            let absolute_y = consume_context_y(contexts);
            SVGTextCharDirective {
                absolute_x,
                absolute_y,
                dx: consume_context_dx(contexts).unwrap_or(0.0),
                dy: consume_context_dy(contexts).unwrap_or(0.0),
                rotation: consume_context_rotate(contexts).unwrap_or(0.0),
                starts_new_chunk: absolute_x.is_some() || absolute_y.is_some(),
            }
        })
        .collect()
}

fn split_text_segments_by_chunk_boundaries(
    text: &str,
    directives: &[SVGTextCharDirective],
) -> Vec<SVGTextSegmentPlan> {
    let mut segments = Vec::new();
    let mut current = SVGTextSegmentPlan::default();

    for (index, ch) in text.chars().enumerate() {
        let directive = directives.get(index).copied().unwrap_or_default();
        if !current.text.is_empty() && directive.starts_new_chunk {
            segments.push(current);
            current = SVGTextSegmentPlan::default();
        }
        if current.text.is_empty() {
            current.starts_new_chunk = directive.starts_new_chunk;
        }
        current.text.push(ch);
        current.directives.push(directive);
    }

    if !current.text.is_empty() {
        segments.push(current);
    }

    segments
}

fn segment_chunk_origin_x(segment: &SVGTextSegmentPlan, current_x: f32) -> f32 {
    let Some(first) = segment.directives.first().copied() else {
        return current_x;
    };
    first.absolute_x.unwrap_or(current_x) + first.dx
}

fn apply_text_length_adjustment(
    node: &SVGResolvedNode,
    range: SVGTextRange,
    state: &mut SVGTextLayoutState,
) {
    let Some(text_data) = text_node_data(node) else {
        return;
    };
    let Some(target_length) = text_data.text_length.and_then(|length| resolve_length(Some(length))) else {
        return;
    };
    let current_length = state.cursor.x - range.start_cursor_x;
    let delta = target_length - current_length;
    if !delta.is_finite() || delta == 0.0 {
        return;
    }

    match text_data.length_adjust.unwrap_or(crate::layout::SVGLengthAdjustValue::Spacing) {
        crate::layout::SVGLengthAdjustValue::Spacing => {
            let count = range.end_address.saturating_sub(range.start_address);
            if count <= 1 {
                return;
            }
            let gaps = (count - 1) as f32;
            for (index, addressable) in state.addressing[range.start_address..range.end_address]
                .iter_mut()
                .enumerate()
            {
                let shift = delta * (index as f32 / gaps);
                addressable.position.x += shift;
            }
            state.cursor.x += delta;
            recompute_run_bounds_for_range(range, state);
        }
        crate::layout::SVGLengthAdjustValue::SpacingAndGlyphs => {
            // Phase 5 subset: spacingAndGlyphs remains explicitly unsupported for now.
        }
    }
}

fn apply_text_path_layout(
    node: &SVGResolvedNode,
    range: SVGTextRange,
    resource_graph: &SVGResourceGraph,
    nodes_by_opaque: &SVGResolvedNodeMap<'_>,
    state: &mut SVGTextLayoutState,
) {
    let SVGNodeKind::TextPath(text_path) = &node.svg_data().node_kind else {
        return;
    };
    let Some(path_node) = text_path
        .href
        .and_then(|reference| reference.local_reference)
        .and_then(|id| resource_graph.node_for_element_id(id))
    else {
        return;
    };
    let Some(path) = resolve_text_path_source_path(path_node, nodes_by_opaque) else {
        return;
    };
    let total_path_length = svg_path_total_length(&path);
    if !total_path_length.is_finite() || total_path_length <= 0.0 {
        return;
    }

    let start_offset = text_path
        .start_offset
        .and_then(|length| resolve_length(Some(length)))
        .unwrap_or(0.0);
    let linear_length = (state.cursor.x - range.start_cursor_x).max(0.0);
    let anchor_adjust = match text_anchor(node) {
        SVGTextAnchor::Start => 0.0,
        SVGTextAnchor::Middle => -linear_length * 0.5,
        SVGTextAnchor::End => -linear_length,
    };

    for addressable in &mut state.addressing[range.start_address..range.end_address] {
        let distance = start_offset + anchor_adjust + (addressable.position.x - range.start_cursor_x);
        if distance < 0.0 || distance > total_path_length {
            addressable.hidden = true;
            continue;
        }
        let Some((point, tangent)) = svg_path_point_and_tangent_at_length(&path, distance) else {
            addressable.hidden = true;
            continue;
        };
        addressable.position = point;
        addressable.rotation += tangent.y.atan2(tangent.x).to_degrees();
        addressable.hidden = false;
    }
    state.current_chunk_anchor = SVGTextAnchor::Start;
    state.current_chunk_origin_x = state.cursor.x;
    recompute_run_bounds_for_range(range, state);
}

fn resolve_text_path_source_path(
    path_node: OpaqueNode,
    nodes_by_opaque: &SVGResolvedNodeMap<'_>,
) -> Option<havi_types::fragment_tree::SVGPathData> {
    let resolved = nodes_by_opaque.get(&path_node).copied()?;
    let svg_data = resolved.svg_data();
    let (geometry, style) = match (&svg_data.node_kind, &resolved.resolved_style) {
        (SVGNodeKind::Geometry(geometry), SVGNodeResolvedStyle::Geometry(style)) => (geometry, style),
        _ => return None,
    };
    let path = normalize_svg_geometry(geometry, style.fill_rule).into();
    let transform = accumulate_svg_transform_chain(resolved, nodes_by_opaque);
    Some(transform_svg_path_data(&path, transform))
}

fn accumulate_svg_transform_chain(
    node: &SVGResolvedNode,
    nodes_by_opaque: &SVGResolvedNodeMap<'_>,
) -> havi_types::fragment_tree::SVGTransform {
    let mut chain = Vec::new();
    let mut current = Some(node);
    while let Some(node) = current {
        chain.push(parse_svg_transform(&node.svg_data().common.transform));
        current = node
            .parent_node
            .and_then(|parent| nodes_by_opaque.get(&parent).copied());
    }
    chain.into_iter().rev().fold(
        havi_types::fragment_tree::SVGTransform::identity(),
        then_svg_transform,
    )
}

fn recompute_run_bounds_for_range(range: SVGTextRange, state: &mut SVGTextLayoutState) {
    for run_index in range.start_run..range.end_run {
        let Some(run) = state.runs.get_mut(run_index) else {
            continue;
        };
        run.rect = recompute_run_rect(run_index as u32, run, &state.addressing[range.start_address..range.end_address]);
    }
}

fn recompute_run_rect(
    run_index: u32,
    run: &SVGGlyphRun,
    addressing: &[SVGAddressableChar],
) -> crate::layout::geom::PhysicalRect<Au> {
    let ascent = run.baseline_ascent.to_f32_px();
    let descent = (run.rect.size.height.to_f32_px() - ascent).max(0.0);
    let mut run_addressing = addressing.iter().filter(|char| char.run_index == run_index);
    let mut bounds: Option<crate::layout::geom::PhysicalRect<Au>> = None;

    for glyph in &run.glyphs {
        let cluster_char_count = glyph.char_count.max(1) as usize;
        let mut first_visible = None;
        for cluster_index in 0..cluster_char_count {
            let Some(char) = run_addressing.next() else {
                break;
            };
            if cluster_index == 0 && !char.hidden {
                first_visible = Some((char.position, char.rotation));
            }
        }
        let Some((position, rotation)) = first_visible else {
            continue;
        };
        let glyph_rect = glyph_bounds_rect_with_metrics(
            position,
            glyph.advance.to_f32_px(),
            ascent,
            descent,
            rotation,
        );
        bounds = Some(match bounds.take() {
            Some(existing) => existing.union(&glyph_rect),
            None => glyph_rect,
        });
    }

    bounds.unwrap_or_default()
}

fn consume_context_x(contexts: &mut [SVGTextPositioningContext]) -> Option<f32> {
    contexts
        .iter_mut()
        .rev()
        .find(|context| context.has_x())
        .and_then(SVGTextPositioningContext::consume_x)
}

fn consume_context_y(contexts: &mut [SVGTextPositioningContext]) -> Option<f32> {
    contexts
        .iter_mut()
        .rev()
        .find(|context| context.has_y())
        .and_then(SVGTextPositioningContext::consume_y)
}

fn consume_context_dx(contexts: &mut [SVGTextPositioningContext]) -> Option<f32> {
    contexts
        .iter_mut()
        .rev()
        .find(|context| context.has_dx())
        .and_then(SVGTextPositioningContext::consume_dx)
}

fn consume_context_dy(contexts: &mut [SVGTextPositioningContext]) -> Option<f32> {
    contexts
        .iter_mut()
        .rev()
        .find(|context| context.has_dy())
        .and_then(SVGTextPositioningContext::consume_dy)
}

fn consume_context_rotate(contexts: &mut [SVGTextPositioningContext]) -> Option<f32> {
    contexts
        .iter_mut()
        .rev()
        .find(|context| context.has_rotate())
        .and_then(SVGTextPositioningContext::consume_rotate)
}

fn position_shaped_svg_run(
    node: &SVGResolvedNode,
    run_index: u32,
    mut run: SVGGlyphRun,
    font_metrics: &FontMetrics,
    directives: &[SVGTextCharDirective],
    cursor: &mut SVGTextCursor,
    addressing: &mut Vec<SVGAddressableChar>,
    chunk_address_start: usize,
) -> SVGGlyphRun {
    let mut run_bounds: Option<crate::layout::geom::PhysicalRect<Au>> = None;
    let mut directive_index = 0usize;
    let mut utf8_offset = 0usize;

    for glyph in &run.glyphs {
        let cluster_char_count = glyph.char_count.max(1) as usize;
        let mut first_position = None;
        let mut first_rotation = 0.0;

        for cluster_index in 0..cluster_char_count {
            let directive = directives.get(directive_index).copied().unwrap_or_default();
            if let Some(x) = directive.absolute_x {
                cursor.x = x;
            }
            if let Some(y) = directive.absolute_y {
                cursor.y = y;
            }
            cursor.x += directive.dx;
            cursor.y += directive.dy;

            let baseline_y = resolve_baseline_y(font_metrics, node, Au::from_f32_px(cursor.y));
            let glyph_position = SVGPoint::new(
                cursor.x + glyph.x_offset.to_f32_px(),
                baseline_y.to_f32_px() + glyph.y_offset.to_f32_px(),
            );
            if first_position.is_none() {
                first_position = Some(glyph_position);
                first_rotation = directive.rotation;
            }

            let Some(ch) = run.text[utf8_offset..].chars().next() else {
                break;
            };
            let start = utf8_offset;
            utf8_offset += ch.len_utf8();
            let is_first_chunk_char = addressing.len() == chunk_address_start;
            addressing.push(SVGAddressableChar {
                run_index,
                utf8_range: start as u32..utf8_offset as u32,
                position: glyph_position,
                rotation: directive.rotation,
                hidden: false,
                middle_of_cluster: cluster_index > 0,
                anchored_chunk_start: is_first_chunk_char,
            });
            directive_index += 1;
        }

        if let Some(position) = first_position {
            let glyph_rect = glyph_bounds_rect(position, glyph.advance.to_f32_px(), font_metrics, first_rotation);
            run_bounds = Some(match run_bounds.take() {
                Some(bounds) => bounds.union(&glyph_rect),
                None => glyph_rect,
            });
        }
        cursor.x += glyph.advance.to_f32_px();
    }

    while let Some(ch) = run.text[utf8_offset..].chars().next() {
        let directive = directives.get(directive_index).copied().unwrap_or_default();
        if let Some(x) = directive.absolute_x {
            cursor.x = x;
        }
        if let Some(y) = directive.absolute_y {
            cursor.y = y;
        }
        cursor.x += directive.dx;
        cursor.y += directive.dy;
        let baseline_y = resolve_baseline_y(font_metrics, node, Au::from_f32_px(cursor.y));
        let start = utf8_offset;
        utf8_offset += ch.len_utf8();
        let is_first_chunk_char = addressing.len() == chunk_address_start;
        addressing.push(SVGAddressableChar {
            run_index,
            utf8_range: start as u32..utf8_offset as u32,
            position: SVGPoint::new(cursor.x, baseline_y.to_f32_px()),
            rotation: directive.rotation,
            hidden: false,
            middle_of_cluster: false,
            anchored_chunk_start: is_first_chunk_char,
        });
        directive_index += 1;
    }

    run.rect = run_bounds.unwrap_or_default();
    run
}

fn glyph_bounds_rect(
    position: SVGPoint,
    advance_width: f32,
    font_metrics: &FontMetrics,
    rotation_degrees: f32,
) -> crate::layout::geom::PhysicalRect<Au> {
    glyph_bounds_rect_with_metrics(
        position,
        advance_width,
        font_metrics.ascent.to_f32_px(),
        font_metrics.descent.to_f32_px(),
        rotation_degrees,
    )
}

fn glyph_bounds_rect_with_metrics(
    position: SVGPoint,
    advance_width: f32,
    ascent: f32,
    descent: f32,
    rotation_degrees: f32,
) -> crate::layout::geom::PhysicalRect<Au> {
    let width = advance_width.max(0.0);
    let height = (ascent + descent).max(0.0);
    let origin_x = position.x;
    let origin_y = position.y;

    if rotation_degrees == 0.0 {
        return crate::layout::geom::PhysicalRect::new(
            crate::layout::geom::PhysicalPoint::new(
                Au::from_f32_px(origin_x),
                Au::from_f32_px(origin_y - ascent),
            ),
            crate::layout::geom::PhysicalSize::new(
                Au::from_f32_px(width),
                Au::from_f32_px(height),
            ),
        );
    }

    let angle = rotation_degrees.to_radians();
    let sin = angle.sin();
    let cos = angle.cos();
    let corners = [
        (0.0, -ascent),
        (width, -ascent),
        (0.0, descent),
        (width, descent),
    ];
    let transformed = corners.map(|(x, y)| {
        let rotated_x = x * cos - y * sin;
        let rotated_y = x * sin + y * cos;
        (origin_x + rotated_x, origin_y + rotated_y)
    });
    let min_x = transformed.iter().map(|(x, _)| *x).fold(f32::INFINITY, f32::min);
    let min_y = transformed.iter().map(|(_, y)| *y).fold(f32::INFINITY, f32::min);
    let max_x = transformed.iter().map(|(x, _)| *x).fold(f32::NEG_INFINITY, f32::max);
    let max_y = transformed.iter().map(|(_, y)| *y).fold(f32::NEG_INFINITY, f32::max);

    crate::layout::geom::PhysicalRect::new(
        crate::layout::geom::PhysicalPoint::new(Au::from_f32_px(min_x), Au::from_f32_px(min_y)),
        crate::layout::geom::PhysicalSize::new(
            Au::from_f32_px(max_x - min_x),
            Au::from_f32_px(max_y - min_y),
        ),
    )
}

fn shape_svg_text_runs(
    node: &SVGResolvedNode,
    text_layout_context: &SVGTextLayoutContext,
    text: &str,
) -> Vec<ShapedSVGRun> {
    let bidi_info = BidiInfo::new(text, None);
    let segments = segment_text_by_font(node, text_layout_context, text, &bidi_info);
    segments
        .into_iter()
        .filter_map(|segment| shape_svg_text_segment(node, text, segment))
        .collect()
}

fn segment_text_by_font(
    node: &SVGResolvedNode,
    text_layout_context: &SVGTextLayoutContext,
    text: &str,
    bidi_info: &BidiInfo,
) -> Vec<SVGTextSegment> {
    let font_group = text_node_data(node)
        .and_then(|text| text.font_size)
        .and_then(crate::layout::resolve_svg_length_to_user_units)
        .map(|font_size_px| {
            text_layout_context
                .font_context
                .font_group_with_size(node.computed_style.clone_font(), Au::from_f32_px(font_size_px))
        })
        .unwrap_or_else(|| text_layout_context.font_context.font_group(node.computed_style.clone_font()));
    let lang = node.computed_style.get_font()._x_lang.clone();

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
            &text_layout_context.font_context,
            character,
            next_character,
            lang.clone(),
        ) else {
            continue;
        };

        if let Some(current) = current.as_mut() {
            if current.update_if_compatible(text_layout_context, &font, script, bidi_level) {
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
            .first(&text_layout_context.font_context)
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
    node: &SVGResolvedNode,
    text: &str,
    segment: SVGTextSegment,
) -> Option<ShapedSVGRun> {
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

    let font_data_and_index = segment.font.font_data_and_index().ok();
    let font_data = font_data_and_index
        .as_ref()
        .map(|data_and_index| std::sync::Arc::new(data_and_index.data.as_ref().to_vec()));
    let glyphs = glyph_store
        .glyphs()
        .map(|glyph| ShapedGlyph {
            glyph_id: glyph.id(),
            advance: glyph.advance(),
            x_offset: glyph.offset().map_or(Au::zero(), |offset| offset.x),
            y_offset: glyph.offset().map_or(Au::zero(), |offset| offset.y),
            char_count: glyph.character_count() as u32,
        })
        .collect();

    Some(ShapedSVGRun {
        run: SVGGlyphRun {
            text: segment_text.to_string(),
            rect: crate::layout::geom::PhysicalRect::zero(),
            font_size_px: segment.font.descriptor.pt_size.to_f32_px(),
            glyphs,
            font_data,
            font_index: font_data_and_index.map(|data_and_index| data_and_index.index).unwrap_or(0),
            baseline_ascent: segment.font.metrics.ascent,
        },
        font_metrics: segment.font.metrics.clone(),
    })
}

fn text_node_data(node: &SVGResolvedNode) -> Option<&crate::layout::SVGTextData> {
    match &node.node_data.node_kind {
        SVGOwnedNodeKind::Text(data) | SVGOwnedNodeKind::TSpan(data) => Some(data),
        // TextPath contributes its inline text positioning; path-following is stubbed (svg-missing.md).
        SVGOwnedNodeKind::TextPath(data) => Some(&data.text),
        _ => None,
    }
}

fn text_anchor(node: &SVGResolvedNode) -> SVGTextAnchor {
    match &node.resolved_style {
        SVGNodeResolvedStyle::Text(style) => style.text_anchor,
        _ => SVGTextAnchor::Start,
    }
}

fn text_stroke_half_width(node: &SVGResolvedNode) -> Option<f32> {
    match &node.resolved_style {
        SVGNodeResolvedStyle::Text(style) => style.paint.stroke.as_ref().map(|stroke| stroke.width.max(0.0) * 0.5),
        _ => None,
    }
}

fn resolve_baseline_y(font_metrics: &FontMetrics, node: &SVGResolvedNode, y: Au) -> Au {
    let baseline = match &node.resolved_style {
        SVGNodeResolvedStyle::Text(style) => style
            .alignment_baseline
            .or(style.dominant_baseline),
        _ => None,
    };
    match baseline {
        Some(SVGTextBaselineValue::Middle) | Some(SVGTextBaselineValue::Central) => {
            y + (font_metrics.ascent - font_metrics.descent).scale_by(0.5)
        }
        Some(SVGTextBaselineValue::Hanging) | Some(SVGTextBaselineValue::TextBeforeEdge) => {
            y + font_metrics.ascent
        }
        Some(SVGTextBaselineValue::TextAfterEdge) => y - font_metrics.descent,
        _ => y,
    }
}

fn text_run_bounds(text_runs: &[SVGGlyphRun]) -> Option<havi_types::fragment_tree::SVGRect> {
    let mut rects = text_runs.iter().map(|run| run.rect);
    let first = rects.next()?;
    let union = rects.fold(first, |union, rect| union.union(&rect));
    Some(havi_types::fragment_tree::SVGRect::new(
        euclid::point2(union.origin.x.to_f32_px(), union.origin.y.to_f32_px()),
        euclid::size2(union.size.width.to_f32_px(), union.size.height.to_f32_px()),
    ))
}

fn inflate_text_bounds(mut rect: havi_types::fragment_tree::SVGRect, inflate: f32) -> havi_types::fragment_tree::SVGRect {
    rect.origin.x -= inflate;
    rect.origin.y -= inflate;
    rect.size.width += inflate * 2.0;
    rect.size.height += inflate * 2.0;
    rect
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

#[cfg(test)]
mod tests {
    use super::*;

    fn text_data() -> crate::layout::SVGTextData {
        crate::layout::SVGTextData {
            x: Vec::new(),
            y: Vec::new(),
            dx: Vec::new(),
            dy: Vec::new(),
            rotate: Vec::new(),
            font_size: None,
            text_length: None,
            length_adjust: None,
            text_anchor: None,
            alignment_baseline: None,
            dominant_baseline: None,
        }
    }

    #[test]
    fn directives_use_nearest_positioning_context_and_repeat_rotate() {
        let mut parent = text_data();
        parent.x = vec![crate::layout::SVGLengthValue {
            unit_type: crate::layout::SVG_LENGTHTYPE_NUMBER,
            value: 10.0,
        }];
        parent.rotate = vec![15.0];

        let mut child = text_data();
        child.dx = vec![
            crate::layout::SVGLengthValue {
                unit_type: crate::layout::SVG_LENGTHTYPE_NUMBER,
                value: 2.0,
            },
            crate::layout::SVGLengthValue {
                unit_type: crate::layout::SVG_LENGTHTYPE_NUMBER,
                value: 3.0,
            },
        ];
        child.rotate = vec![30.0, 45.0];

        let mut contexts = vec![
            SVGTextPositioningContext::new(&parent),
            SVGTextPositioningContext::new(&child),
        ];
        let directives = collect_char_directives("abz", &mut contexts);

        assert_eq!(directives[0].absolute_x, Some(10.0));
        assert_eq!(directives[0].dx, 2.0);
        assert_eq!(directives[0].rotation, 30.0);
        assert_eq!(directives[1].dx, 3.0);
        assert_eq!(directives[1].rotation, 45.0);
        assert_eq!(directives[2].dx, 0.0);
        assert_eq!(directives[2].rotation, 45.0);
    }

    #[test]
    fn segments_split_on_explicit_chunk_boundaries() {
        let directives = vec![
            SVGTextCharDirective::default(),
            SVGTextCharDirective {
                absolute_x: Some(40.0),
                starts_new_chunk: true,
                ..Default::default()
            },
            SVGTextCharDirective::default(),
        ];
        let segments = split_text_segments_by_chunk_boundaries("abc", &directives);

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "a");
        assert_eq!(segments[1].text, "bc");
        assert!(segments[1].starts_new_chunk);
    }

    #[test]
    fn segment_origin_uses_absolute_x_and_dx() {
        let segment = SVGTextSegmentPlan {
            text: "a".to_owned(),
            directives: vec![SVGTextCharDirective {
                absolute_x: Some(20.0),
                dx: 5.0,
                ..Default::default()
            }],
            starts_new_chunk: true,
        };
        assert_eq!(segment_chunk_origin_x(&segment, 3.0), 25.0);
    }

    #[test]
    fn finishing_middle_chunk_shifts_runs_and_addressing() {
        let mut state = SVGTextLayoutState::default();
        state.cursor.x = 30.0;
        state.current_chunk_start_run = Some(0);
        state.current_chunk_anchor = SVGTextAnchor::Middle;
        state.current_chunk_address_start = 0;
        state.current_chunk_origin_x = 10.0;
        state.runs.push(SVGGlyphRun {
            text: "ab".to_owned(),
            rect: crate::layout::geom::PhysicalRect::new(
                crate::layout::geom::PhysicalPoint::new(Au::from_f32_px(10.0), Au::zero()),
                crate::layout::geom::PhysicalSize::new(Au::from_f32_px(20.0), Au::from_f32_px(10.0)),
            ),
            font_size_px: 12.0,
            glyphs: Vec::new(),
            font_data: None,
            font_index: 0,
            baseline_ascent: Au::zero(),
        });
        state.addressing.push(SVGAddressableChar {
            run_index: 0,
            utf8_range: 0..1,
            position: SVGPoint::new(10.0, 0.0),
            rotation: 0.0,
            hidden: false,
            middle_of_cluster: false,
            anchored_chunk_start: true,
        });

        state.finish_current_chunk();

        assert_eq!(state.runs[0].rect.origin.x.to_f32_px(), 0.0);
        assert_eq!(state.addressing[0].position.x, 0.0);
        assert_eq!(state.chunks.len(), 1);
        assert_eq!(state.chunks[0].anchor, SVGTextAnchor::Middle);
    }

    #[test]
    fn rotated_glyph_bounds_expand_vertical_extent() {
        let metrics = FontMetrics {
            ascent: Au::from_f32_px(8.0),
            descent: Au::from_f32_px(2.0),
            line_gap: Au::from_f32_px(10.0),
            underline_size: Au::zero(),
            underline_offset: Au::zero(),
            strikeout_size: Au::zero(),
            strikeout_offset: Au::zero(),
            x_height: Au::zero(),
            em_size: Au::from_f32_px(10.0),
            average_advance: Au::from_f32_px(10.0),
            max_advance: Au::from_f32_px(10.0),
            leading: Au::zero(),
            zero_horizontal_advance: None,
            ic_horizontal_advance: None,
            space_advance: Au::from_f32_px(10.0),
        };
        let unrotated = glyph_bounds_rect(SVGPoint::new(10.0, 20.0), 10.0, &metrics, 0.0);
        let rotated = glyph_bounds_rect(SVGPoint::new(10.0, 20.0), 10.0, &metrics, 45.0);

        assert!(
            rotated.size.height > unrotated.size.height ||
                rotated.size.width > unrotated.size.width
        );
    }
}
