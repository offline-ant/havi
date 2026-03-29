use havi_types::fragment_tree::{SVGResourceId, SVGTransform};
use rustc_hash::FxHashMap;
use style::dom::OpaqueNode;

use super::dom::SVGResolvedNode;
use super::path::resolve_length;
use super::resources::SVGResourceGraph;
use super::transform::{parse_svg_transform, then_svg_transform, translate_svg_transform};

#[derive(Clone, Debug, Default)]
pub struct SVGUseExpansionResult<'dom> {
    pub referenced_resource: Option<SVGResourceId>,
    pub referenced_node: Option<script::layout_dom::ServoThreadSafeLayoutNode<'dom>>,
    pub instance_transform: SVGTransform,
}

pub fn expand_use_node<'dom>(
    use_node: &SVGResolvedNode<'dom>,
    nodes_by_opaque: &FxHashMap<OpaqueNode, script::layout_dom::ServoThreadSafeLayoutNode<'dom>>,
    resource_graph: &SVGResourceGraph,
) -> SVGUseExpansionResult<'dom> {
    let mut result = SVGUseExpansionResult::default();
    let Some(resolved) = resource_graph.node_resources(use_node.tag.node) else {
        return result;
    };

    result.referenced_resource = resolved.use_instance_source;
    result.referenced_node = resolved
        .referenced_node
        .and_then(|node| nodes_by_opaque.get(&node).copied());

    result.instance_transform = use_instance_transform(
        match &use_node.svg_data.node_kind {
            layout_api::SVGNodeKind::Use(data) => Some(data),
            _ => None,
        },
        &use_node.svg_data.common.transform,
    );
    result
}

fn use_instance_transform(
    use_data: Option<&layout_api::SVGUseData<'_>>,
    transform: &[layout_api::SVGTransformValue],
) -> SVGTransform {
    let translation = use_data.map_or(SVGTransform::identity(), |data| {
        translate_svg_transform(
            resolve_length(data.x).unwrap_or(0.0),
            resolve_length(data.y).unwrap_or(0.0),
        )
    });
    let node_transform = parse_svg_transform(transform);
    then_svg_transform(translation, node_transform)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combines_use_translation_and_transform() {
        let transform = use_instance_transform(
            Some(&layout_api::SVGUseData {
                href: layout_api::parse_svg_reference(Some("#shape")),
                x: Some(layout_api::parse_svg_length(Some("10"))),
                y: Some(layout_api::parse_svg_length(Some("20"))),
                width: None,
                height: None,
            }),
            &layout_api::parse_svg_transform_list(Some("scale(2)")),
        );
        let point = super::super::transform::transform_svg_point(
            transform,
            havi_types::fragment_tree::SVGPoint::new(1.0, 1.0),
        );
        assert_eq!(point, havi_types::fragment_tree::SVGPoint::new(22.0, 42.0));
    }
}
