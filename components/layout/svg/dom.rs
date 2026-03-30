use layout_api::wrapper_traits::ThreadSafeLayoutNode;
use layout_api::{SVGElementData, SVGNodeKind};
use script::layout_dom::ServoThreadSafeLayoutNode;
use servo_arc::Arc as ServoArc;
use style::context::{SharedStyleContext, StyleContext, ThreadLocalStyleContext};
use style::dom::{NodeInfo, TElement, TNode};
use style::properties::ComputedValues;
use style::stylist::RuleInclusion;
use style::traversal::resolve_style;

use crate::fragment_tree::Tag;

use super::style::{
    resolve_geometry_style, resolve_text_style, resolve_viewport_style, SVGGeometryStyle,
    SVGTextStyle, SVGViewportStyle,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGLayoutNodeKind {
    Viewport,
    Group,
    Geometry,
    Text,
    TextPath,
    Defs,
    Use,
    Gradient,
    Stop,
    ClipPath,
    Mask,
    Pattern,
    Filter,
    Marker,
    ForeignObject,
    Image,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SVGLayoutNodeSummary {
    pub kind: SVGLayoutNodeKind,
    pub establishes_viewport: bool,
    pub participates_in_paint: bool,
}

#[derive(Clone, Debug)]
pub enum SVGNodeResolvedStyle {
    Viewport {
        viewport: SVGViewportStyle,
        geometry: SVGGeometryStyle,
    },
    Geometry(SVGGeometryStyle),
    Text(SVGTextStyle),
}

#[derive(Clone, Debug)]
pub struct SVGResolvedNode<'dom> {
    pub node: ServoThreadSafeLayoutNode<'dom>,
    pub tag: Tag,
    pub summary: SVGLayoutNodeSummary,
    pub svg_data: SVGElementData<'dom>,
    pub resolved_style: SVGNodeResolvedStyle,
    pub computed_style: ServoArc<ComputedValues>,
    next_inherited_geometry: Option<SVGGeometryStyle>,
    next_inherited_text: Option<SVGTextStyle>,
}

impl SVGResolvedNode<'_> {
    pub fn inherited_geometry(&self) -> Option<&SVGGeometryStyle> {
        self.next_inherited_geometry.as_ref()
    }

    pub fn inherited_text(&self) -> Option<&SVGTextStyle> {
        self.next_inherited_text.as_ref()
    }
}

pub fn resolve_svg_node<'dom>(
    node: ServoThreadSafeLayoutNode<'dom>,
    context: &SharedStyleContext,
) -> Option<SVGResolvedNode<'dom>> {
    let chain = collect_svg_ancestor_chain(node);
    let mut inherited_geometry: Option<SVGGeometryStyle> = None;
    let mut inherited_text: Option<SVGTextStyle> = None;
    let mut resolved = None;

    for current in chain {
        let current_resolved = resolve_svg_node_with_inheritance(
            current,
            context,
            inherited_geometry.as_ref(),
            inherited_text.as_ref(),
        )?;
        inherited_geometry = current_resolved.inherited_geometry().cloned();
        inherited_text = current_resolved.inherited_text().cloned();
        resolved = Some(current_resolved);
    }

    resolved
}

pub fn resolve_svg_child_node<'dom>(
    child: ServoThreadSafeLayoutNode<'dom>,
    context: &SharedStyleContext,
    parent: &SVGResolvedNode<'dom>,
) -> Option<SVGResolvedNode<'dom>> {
    resolve_svg_node_with_inheritance(
        child,
        context,
        parent.inherited_geometry(),
        parent.inherited_text(),
    )
}

fn collect_svg_ancestor_chain<'dom>(
    node: ServoThreadSafeLayoutNode<'dom>,
) -> Vec<ServoThreadSafeLayoutNode<'dom>> {
    let mut chain = Vec::new();
    let mut current = Some(node.unsafe_get());
    while let Some(layout_node) = current {
        let threadsafe = ServoThreadSafeLayoutNode::new(layout_node);
        if threadsafe.svg_data().is_some() {
            chain.push(threadsafe);
        }
        current = layout_node.traversal_parent().map(|parent| parent.as_node());
    }
    chain.reverse();
    chain
}

fn resolve_svg_node_style<'dom>(
    node: ServoThreadSafeLayoutNode<'dom>,
    context: &SharedStyleContext,
) -> Option<ServoArc<ComputedValues>> {
    if node.style_data().is_some() {
        return Some(node.style(context));
    }

    if let Some(element) = node.unsafe_get().as_element() {
        let mut thread_local = ThreadLocalStyleContext::new();
        let mut style_context = StyleContext {
            shared: context,
            thread_local: &mut thread_local,
        };
        let styles = resolve_style(&mut style_context, element, RuleInclusion::All, None, None);
        return Some(styles.primary().clone());
    }

    node.is_text_node().then(|| node.parent_style(context))
}

fn resolve_svg_node_with_inheritance<'dom>(
    node: ServoThreadSafeLayoutNode<'dom>,
    context: &SharedStyleContext,
    inherited_geometry: Option<&SVGGeometryStyle>,
    inherited_text: Option<&SVGTextStyle>,
) -> Option<SVGResolvedNode<'dom>> {
    let svg_data = node.svg_data()?;
    let tag = Tag::from(node);
    let computed_style = resolve_svg_node_style(node, context)?;
    let summary = summarize_node_kind(&svg_data.node_kind);
    let resolved_style = match &svg_data.node_kind {
        SVGNodeKind::Viewport(_) => SVGNodeResolvedStyle::Viewport {
            viewport: resolve_viewport_style(&svg_data, &computed_style),
            geometry: resolve_geometry_style(&svg_data, &computed_style, inherited_geometry),
        },
        SVGNodeKind::Text(_) | SVGNodeKind::TSpan(_) | SVGNodeKind::TextPath(_) => {
            SVGNodeResolvedStyle::Text(resolve_text_style(
                &svg_data,
                &computed_style,
                inherited_text,
            ))
        }
        _ => SVGNodeResolvedStyle::Geometry(resolve_geometry_style(
            &svg_data,
            &computed_style,
            inherited_geometry,
        )),
    };

    let next_inherited_geometry = match &resolved_style {
        SVGNodeResolvedStyle::Viewport { geometry, .. } => Some(geometry.clone()),
        SVGNodeResolvedStyle::Geometry(geometry) => Some(geometry.clone()),
        SVGNodeResolvedStyle::Text(_) => inherited_geometry.cloned(),
    };
    let next_inherited_text = match &resolved_style {
        SVGNodeResolvedStyle::Text(text) => Some(text.clone()),
        _ => inherited_text.cloned(),
    };

    Some(SVGResolvedNode {
        node,
        tag,
        summary,
        svg_data,
        resolved_style,
        computed_style,
        next_inherited_geometry,
        next_inherited_text,
    })
}

pub fn summarize_node_kind(node_kind: &SVGNodeKind<'_>) -> SVGLayoutNodeSummary {
    match node_kind {
        SVGNodeKind::Viewport(_) => SVGLayoutNodeSummary {
            kind: SVGLayoutNodeKind::Viewport,
            establishes_viewport: true,
            participates_in_paint: true,
        },
        SVGNodeKind::Group => SVGLayoutNodeSummary {
            kind: SVGLayoutNodeKind::Group,
            establishes_viewport: false,
            participates_in_paint: true,
        },
        SVGNodeKind::Geometry(_) => SVGLayoutNodeSummary {
            kind: SVGLayoutNodeKind::Geometry,
            establishes_viewport: false,
            participates_in_paint: true,
        },
        SVGNodeKind::Text(_) | SVGNodeKind::TSpan(_) => SVGLayoutNodeSummary {
            kind: SVGLayoutNodeKind::Text,
            establishes_viewport: false,
            participates_in_paint: true,
        },
        SVGNodeKind::TextPath(_) => SVGLayoutNodeSummary {
            kind: SVGLayoutNodeKind::TextPath,
            establishes_viewport: false,
            participates_in_paint: true,
        },
        SVGNodeKind::Defs => SVGLayoutNodeSummary {
            kind: SVGLayoutNodeKind::Defs,
            establishes_viewport: false,
            participates_in_paint: false,
        },
        SVGNodeKind::Use(_) => SVGLayoutNodeSummary {
            kind: SVGLayoutNodeKind::Use,
            establishes_viewport: false,
            participates_in_paint: true,
        },
        SVGNodeKind::Gradient(_) => SVGLayoutNodeSummary {
            kind: SVGLayoutNodeKind::Gradient,
            establishes_viewport: false,
            participates_in_paint: false,
        },
        SVGNodeKind::Stop(_) => SVGLayoutNodeSummary {
            kind: SVGLayoutNodeKind::Stop,
            establishes_viewport: false,
            participates_in_paint: false,
        },
        SVGNodeKind::ClipPath(_) => SVGLayoutNodeSummary {
            kind: SVGLayoutNodeKind::ClipPath,
            establishes_viewport: false,
            participates_in_paint: false,
        },
        SVGNodeKind::Mask(_) => SVGLayoutNodeSummary {
            kind: SVGLayoutNodeKind::Mask,
            establishes_viewport: false,
            participates_in_paint: false,
        },
        SVGNodeKind::ForeignObject(_) => SVGLayoutNodeSummary {
            kind: SVGLayoutNodeKind::ForeignObject,
            establishes_viewport: false,
            participates_in_paint: true,
        },
        SVGNodeKind::Pattern(_) => SVGLayoutNodeSummary {
            kind: SVGLayoutNodeKind::Pattern,
            establishes_viewport: false,
            participates_in_paint: false,
        },
        SVGNodeKind::Filter(_) => SVGLayoutNodeSummary {
            kind: SVGLayoutNodeKind::Filter,
            establishes_viewport: false,
            participates_in_paint: false,
        },
        SVGNodeKind::Marker(_) => SVGLayoutNodeSummary {
            kind: SVGLayoutNodeKind::Marker,
            establishes_viewport: false,
            participates_in_paint: false,
        },
        SVGNodeKind::Image(_) => SVGLayoutNodeSummary {
            kind: SVGLayoutNodeKind::Image,
            establishes_viewport: false,
            participates_in_paint: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use layout_api::{
        SVGFilterData, SVGNodeKind, SVGPatternData, SVGPreserveAspectRatioValue, SVGTextData,
        SVGTextPathData,
    };

    use super::*;

    fn empty_text() -> SVGTextData {
        SVGTextData {
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
    fn summarizes_phase3_resource_nodes_and_textpath() {
        let pattern = summarize_node_kind(&SVGNodeKind::Pattern(SVGPatternData {
            href: None,
            x: None,
            y: None,
            width: None,
            height: None,
            pattern_units: None,
            pattern_content_units: None,
            pattern_transform: Vec::new(),
            view_box: None,
            preserve_aspect_ratio: SVGPreserveAspectRatioValue::default(),
        }));
        assert_eq!(pattern.kind, SVGLayoutNodeKind::Pattern);
        assert!(!pattern.participates_in_paint);

        let filter = summarize_node_kind(&SVGNodeKind::Filter(SVGFilterData {
            x: None,
            y: None,
            width: None,
            height: None,
            filter_units: None,
            primitive_units: None,
        }));
        assert_eq!(filter.kind, SVGLayoutNodeKind::Filter);
        assert!(!filter.participates_in_paint);

        let marker = summarize_node_kind(&SVGNodeKind::Marker(layout_api::SVGMarkerData {
            ref_x: None,
            ref_y: None,
            marker_width: None,
            marker_height: None,
            marker_units: None,
            orient_auto: true,
            view_box: None,
            preserve_aspect_ratio: SVGPreserveAspectRatioValue::default(),
        }));
        assert_eq!(marker.kind, SVGLayoutNodeKind::Marker);
        assert!(!marker.participates_in_paint);

        let text_path = summarize_node_kind(&SVGNodeKind::TextPath(SVGTextPathData {
            href: None,
            start_offset: None,
            text: empty_text(),
        }));
        assert_eq!(text_path.kind, SVGLayoutNodeKind::TextPath);
        assert!(text_path.participates_in_paint);
    }
}
