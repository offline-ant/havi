use havi_types::fragment_tree::{SVGResourceId, SVGTransform};
use rustc_hash::FxHashMap;
use style::dom::OpaqueNode;

use super::dom::{SVGDOMNode, SVGNodeKindOwned};
use super::path::parse_svg_length;
use super::resources::SVGResourceGraph;
use super::transform::{parse_svg_transform, then_svg_transform, translate_svg_transform};

#[derive(Clone, Debug, Default)]
pub struct SVGUseExpansionResult<'a> {
    pub referenced_resource: Option<SVGResourceId>,
    pub referenced_node: Option<&'a SVGDOMNode>,
    pub instance_transform: SVGTransform,
}

pub fn expand_use_node<'a>(
    use_node: &'a SVGDOMNode,
    nodes_by_opaque: &'a FxHashMap<OpaqueNode, &'a SVGDOMNode>,
    resource_graph: &SVGResourceGraph,
) -> SVGUseExpansionResult<'a> {
    let mut result = SVGUseExpansionResult::default();
    let Some(resolved) = resource_graph.node_resources(use_node.tag.node) else {
        return result;
    };

    result.referenced_resource = resolved.use_instance_source;
    result.referenced_node = resolved
        .referenced_node
        .and_then(|node| nodes_by_opaque.get(&node).copied());

    let translation = match &use_node.node_kind {
        SVGNodeKindOwned::Use(data) => translate_svg_transform(
            parse_svg_length(data.x.as_deref()).unwrap_or(0.0),
            parse_svg_length(data.y.as_deref()).unwrap_or(0.0),
        ),
        _ => SVGTransform::identity(),
    };
    let node_transform = parse_svg_transform(use_node.common.transform.as_deref());
    result.instance_transform = then_svg_transform(translation, node_transform);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fragment_tree::Tag;
    use crate::svg::dom::{
        SVGCommonDataOwned, SVGDOMNode, SVGDOMNodeSummary, SVGGeometryDataOwned,
        SVGLayoutNodeKind, SVGNodeKindOwned, SVGNodeResolvedStyle,
    };
    use crate::svg::resources::SVGResourceGraphNode;
    use crate::svg::style::SVGGeometryStyle;

    fn geometry_node(node: usize) -> SVGDOMNode {
        SVGDOMNode {
            tag: Tag {
                node: OpaqueNode(node),
                pseudo_element_chain: Default::default(),
            },
            summary: SVGDOMNodeSummary {
                kind: SVGLayoutNodeKind::Geometry,
                establishes_viewport: false,
                participates_in_paint: true,
            },
            common: SVGCommonDataOwned {
                element_id: Some("shape".to_string()),
                transform: None,
            },
            node_kind: SVGNodeKindOwned::Geometry(SVGGeometryDataOwned::Rect {
                x: None,
                y: None,
                width: Some("10".to_string()),
                height: Some("10".to_string()),
                rx: None,
                ry: None,
            }),
            resolved_style: SVGNodeResolvedStyle::Geometry(SVGGeometryStyle::default()),
            computed_style: servo_arc::Arc::new(style::properties::ComputedValues::default_values()),
            children: Vec::new(),
        }
    }

    #[test]
    fn expands_use_translation_and_reference() {
        let source = geometry_node(2);
        let use_node = SVGDOMNode {
            tag: Tag {
                node: OpaqueNode(3),
                pseudo_element_chain: Default::default(),
            },
            summary: SVGDOMNodeSummary {
                kind: SVGLayoutNodeKind::Use,
                establishes_viewport: false,
                participates_in_paint: true,
            },
            common: SVGCommonDataOwned {
                element_id: None,
                transform: Some("scale(2)".to_string()),
            },
            node_kind: SVGNodeKindOwned::Use(super::super::dom::SVGUseDataOwned {
                href: Some("#shape".to_string()),
                x: Some("10".to_string()),
                y: Some("20".to_string()),
                width: None,
                height: None,
            }),
            resolved_style: SVGNodeResolvedStyle::Geometry(SVGGeometryStyle::default()),
            computed_style: servo_arc::Arc::new(style::properties::ComputedValues::default_values()),
            children: Vec::new(),
        };
        let graph = SVGResourceGraph::build(&[
            SVGResourceGraphNode::new(OpaqueNode(2), SVGLayoutNodeKind::Geometry)
                .with_element_id("shape"),
            SVGResourceGraphNode::new(OpaqueNode(3), SVGLayoutNodeKind::Use).with_href("#shape"),
        ]);
        let mut nodes = FxHashMap::default();
        nodes.insert(source.tag.node, &source);
        nodes.insert(use_node.tag.node, &use_node);

        let result = expand_use_node(&use_node, &nodes, &graph);
        assert_eq!(result.referenced_node.map(|node| node.tag.node), Some(source.tag.node));
        let point = super::super::transform::transform_svg_point(
            result.instance_transform,
            havi_types::fragment_tree::SVGPoint::new(1.0, 1.0),
        );
        assert_eq!(point, havi_types::fragment_tree::SVGPoint::new(22.0, 42.0));
    }
}
