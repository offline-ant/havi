use html5ever::{local_name, ns};
use layout_api::wrapper_traits::{ThreadSafeLayoutElement, ThreadSafeLayoutNode};
use layout_api::SVGNodeKind;
use rustc_hash::FxHashMap;
use crate::script::layout_dom::ServoThreadSafeLayoutNode;
use servo_arc::Arc as ServoArc;
use style::context::{SharedStyleContext, StyleContext, ThreadLocalStyleContext};
use style::dom::{NodeInfo, TNode};
use style::properties::ComputedValues;
use style::stylist::RuleInclusion;
use style::traversal::resolve_style;

use crate::layout::fragment_tree::Tag;

use super::style::{SVGGeometryStyle, SVGTextStyle, SVGViewportStyle};
use super::tree::{
    SVGNodeData, SVGNodeId, SVGNodeMetadata, SVGOwnedNodeKind, SVGResolvedNode,
    SVGStandaloneTree, SVGTreeChild, SVGTreeNode, resolve_svg_tree,
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

pub struct SVGDOMTreeAdapter<'dom> {
    pub tree: SVGStandaloneTree,
    pub dom_nodes: FxHashMap<SVGNodeId, ServoThreadSafeLayoutNode<'dom>>,
    pub computed_styles: FxHashMap<SVGNodeId, ServoArc<ComputedValues>>,
    pub tags: FxHashMap<SVGNodeId, Tag>,
}

impl SVGDOMTreeAdapter<'_> {
    pub fn resolve(&self) -> SVGResolvedNode {
        resolve_svg_tree(
            &self.tree,
            &|node_id| {
                self.tags
                    .get(&node_id)
                    .copied()
                    .expect("missing DOM SVG tag")
            },
            &|node_id| {
                self.computed_styles
                    .get(&node_id)
                    .cloned()
                    .expect("missing DOM SVG computed style")
            },
        )
    }
}

pub fn build_dom_svg_tree<'dom>(
    root: ServoThreadSafeLayoutNode<'dom>,
    context: &SharedStyleContext,
) -> Option<SVGDOMTreeAdapter<'dom>> {
    let mut next_id = 0usize;
    let mut dom_nodes = FxHashMap::default();
    let mut computed_styles = FxHashMap::default();
    let mut tags = FxHashMap::default();
    let root = build_dom_svg_tree_node(
        root,
        context,
        &mut next_id,
        &mut dom_nodes,
        &mut computed_styles,
        &mut tags,
    )?;
    Some(SVGDOMTreeAdapter {
        tree: SVGStandaloneTree::new(root),
        dom_nodes,
        computed_styles,
        tags,
    })
}

fn build_dom_svg_tree_node<'dom>(
    node: ServoThreadSafeLayoutNode<'dom>,
    context: &SharedStyleContext,
    next_id: &mut usize,
    dom_nodes: &mut FxHashMap<SVGNodeId, ServoThreadSafeLayoutNode<'dom>>,
    computed_styles: &mut FxHashMap<SVGNodeId, ServoArc<ComputedValues>>,
    tags: &mut FxHashMap<SVGNodeId, Tag>,
) -> Option<SVGTreeNode> {
    let mut data = SVGNodeData::from(node.svg_data()?);
    if let SVGOwnedNodeKind::Stop(stop) = &mut data.node_kind {
        if stop.stop_color.is_none() {
            stop.stop_color = inline_style_property(node, "stop-color");
        }
        if stop.stop_opacity.is_none() {
            stop.stop_opacity = inline_style_property(node, "stop-opacity");
        }
    }

    let node_id = SVGNodeId(*next_id);
    *next_id += 1;
    dom_nodes.insert(node_id, node);
    computed_styles.insert(node_id, resolve_svg_node_style(node, context)?);
    tags.insert(node_id, Tag::from(node));

    let metadata = SVGNodeMetadata {
        preserve_aspect_ratio_specified: preserve_aspect_ratio_is_specified(node),
    };
    let mut children = Vec::new();
    for child in node.children() {
        if child.is_text_node() {
            let text = child.text_content().into_owned();
            if !text.is_empty() {
                children.push(SVGTreeChild::Text(text));
            }
            continue;
        }
        if let Some(child) = build_dom_svg_tree_node(
            child,
            context,
            next_id,
            dom_nodes,
            computed_styles,
            tags,
        ) {
            children.push(SVGTreeChild::Node(child));
        }
    }

    Some(SVGTreeNode::new(node_id, data, children).with_metadata(metadata))
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

fn inline_style_property(node: ServoThreadSafeLayoutNode<'_>, property: &str) -> Option<String> {
    let element = node.as_element()?;
    let style = element.get_attr(&ns!(), &local_name!("style"))?;
    style.rsplit(';').find_map(|declaration| {
        let (name, value) = declaration.split_once(':')?;
        (name.trim().eq_ignore_ascii_case(property)).then_some(value.trim().to_owned())
    })
}

fn preserve_aspect_ratio_is_specified(node: ServoThreadSafeLayoutNode<'_>) -> bool {
    node.as_element().is_some_and(|element| {
        element
            .get_attr(&ns!(), &local_name!("preserveAspectRatio"))
            .is_some()
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
