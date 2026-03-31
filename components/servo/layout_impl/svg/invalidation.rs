use std::collections::VecDeque;

use bitflags::bitflags;
use havi_types::fragment_tree::SVGResourceId;
use rustc_hash::FxHashMap;
use style::dom::OpaqueNode;

use super::dom::SVGLayoutNodeKind;
use super::resources::{SVGDependencyKind, SVGDependencyTarget, SVGResourceGraph};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGInvalidationKind {
    Paint,
    Geometry,
    Transform,
    ResourceDependency,
    Bounds,
    HitTest,
}

bitflags! {
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct SVGInvalidationFlags: u8 {
        const PAINT = 1 << 0;
        const GEOMETRY = 1 << 1;
        const TRANSFORM = 1 << 2;
        const RESOURCE_DEPENDENCY = 1 << 3;
        const BOUNDS = 1 << 4;
        const HIT_TEST = 1 << 5;
    }
}

impl From<SVGInvalidationKind> for SVGInvalidationFlags {
    fn from(value: SVGInvalidationKind) -> Self {
        match value {
            SVGInvalidationKind::Paint => Self::PAINT,
            SVGInvalidationKind::Geometry => Self::GEOMETRY,
            SVGInvalidationKind::Transform => Self::TRANSFORM,
            SVGInvalidationKind::ResourceDependency => Self::RESOURCE_DEPENDENCY,
            SVGInvalidationKind::Bounds => Self::BOUNDS,
            SVGInvalidationKind::HitTest => Self::HIT_TEST,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGInvalidationRoot {
    Node(OpaqueNode),
    Resource(SVGResourceId),
}

#[derive(Clone, Debug, Default)]
pub struct SVGInvalidationSet {
    pub nodes: FxHashMap<OpaqueNode, SVGInvalidationFlags>,
    pub resources: FxHashMap<SVGResourceId, SVGInvalidationFlags>,
}

impl SVGInvalidationSet {
    pub fn add_node(&mut self, node: OpaqueNode, flags: SVGInvalidationFlags) -> bool {
        add_flags(&mut self.nodes, node, flags)
    }

    pub fn add_resource(&mut self, resource: SVGResourceId, flags: SVGInvalidationFlags) -> bool {
        add_flags(&mut self.resources, resource, flags)
    }

    pub fn node_flags(&self, node: OpaqueNode) -> SVGInvalidationFlags {
        self.nodes.get(&node).copied().unwrap_or_default()
    }

    pub fn resource_flags(&self, resource: SVGResourceId) -> SVGInvalidationFlags {
        self.resources.get(&resource).copied().unwrap_or_default()
    }

    pub fn kinds(&self) -> Vec<SVGInvalidationKind> {
        let mut flags = SVGInvalidationFlags::empty();
        for value in self.nodes.values().chain(self.resources.values()) {
            flags |= *value;
        }
        let mut kinds = Vec::new();
        for (flag, kind) in [
            (SVGInvalidationFlags::PAINT, SVGInvalidationKind::Paint),
            (SVGInvalidationFlags::GEOMETRY, SVGInvalidationKind::Geometry),
            (SVGInvalidationFlags::TRANSFORM, SVGInvalidationKind::Transform),
            (
                SVGInvalidationFlags::RESOURCE_DEPENDENCY,
                SVGInvalidationKind::ResourceDependency,
            ),
            (SVGInvalidationFlags::BOUNDS, SVGInvalidationKind::Bounds),
            (SVGInvalidationFlags::HIT_TEST, SVGInvalidationKind::HitTest),
        ] {
            if flags.contains(flag) {
                kinds.push(kind);
            }
        }
        kinds
    }
}

pub fn classify_attribute_invalidation(
    node_kind: SVGLayoutNodeKind,
    attribute: &str,
) -> SVGInvalidationFlags {
    match attribute {
        "fill" | "fill-opacity" | "color" | "opacity" | "stop-color" | "stop-opacity" => {
            SVGInvalidationFlags::PAINT
        }
        "stroke" => SVGInvalidationFlags::PAINT,
        "stroke-width" | "stroke-linecap" | "stroke-linejoin" | "stroke-miterlimit" | "vector-effect" => {
            SVGInvalidationFlags::PAINT | SVGInvalidationFlags::BOUNDS | SVGInvalidationFlags::HIT_TEST
        }
        "transform" => {
            SVGInvalidationFlags::TRANSFORM | SVGInvalidationFlags::BOUNDS | SVGInvalidationFlags::HIT_TEST
        }
        "display" | "visibility" | "pointer-events" => {
            SVGInvalidationFlags::PAINT | SVGInvalidationFlags::BOUNDS | SVGInvalidationFlags::HIT_TEST
        }
        "clip-path" | "mask" | "filter" | "marker-start" | "marker-mid" | "marker-end" => {
            SVGInvalidationFlags::PAINT |
                SVGInvalidationFlags::RESOURCE_DEPENDENCY |
                SVGInvalidationFlags::BOUNDS |
                SVGInvalidationFlags::HIT_TEST
        }
        "viewBox" | "preserveAspectRatio" | "overflow"
            if matches!(node_kind, SVGLayoutNodeKind::Viewport) =>
        {
            SVGInvalidationFlags::TRANSFORM |
                SVGInvalidationFlags::BOUNDS |
                SVGInvalidationFlags::HIT_TEST
        }
        "href" if matches!(node_kind, SVGLayoutNodeKind::Use) => {
            SVGInvalidationFlags::PAINT |
                SVGInvalidationFlags::GEOMETRY |
                SVGInvalidationFlags::TRANSFORM |
                SVGInvalidationFlags::RESOURCE_DEPENDENCY |
                SVGInvalidationFlags::BOUNDS |
                SVGInvalidationFlags::HIT_TEST
        }
        "href" if matches!(node_kind, SVGLayoutNodeKind::Gradient) => {
            SVGInvalidationFlags::PAINT | SVGInvalidationFlags::RESOURCE_DEPENDENCY
        }
        "href" if matches!(node_kind, SVGLayoutNodeKind::Pattern) => {
            SVGInvalidationFlags::PAINT | SVGInvalidationFlags::RESOURCE_DEPENDENCY
        }
        "href" if matches!(node_kind, SVGLayoutNodeKind::TextPath) => {
            SVGInvalidationFlags::GEOMETRY |
                SVGInvalidationFlags::BOUNDS |
                SVGInvalidationFlags::PAINT
        }
        "startOffset" if matches!(node_kind, SVGLayoutNodeKind::TextPath) => {
            SVGInvalidationFlags::GEOMETRY | SVGInvalidationFlags::BOUNDS
        }
        "patternUnits" | "patternContentUnits" | "patternTransform"
            if matches!(node_kind, SVGLayoutNodeKind::Pattern) =>
        {
            SVGInvalidationFlags::PAINT |
                SVGInvalidationFlags::RESOURCE_DEPENDENCY |
                SVGInvalidationFlags::BOUNDS
        }
        "filterUnits" | "primitiveUnits"
            if matches!(node_kind, SVGLayoutNodeKind::Filter) =>
        {
            SVGInvalidationFlags::PAINT |
                SVGInvalidationFlags::RESOURCE_DEPENDENCY |
                SVGInvalidationFlags::BOUNDS
        }
        "markerUnits" | "orient" | "refX" | "refY" | "markerWidth" | "markerHeight"
            if matches!(node_kind, SVGLayoutNodeKind::Marker) =>
        {
            SVGInvalidationFlags::PAINT |
                SVGInvalidationFlags::RESOURCE_DEPENDENCY |
                SVGInvalidationFlags::BOUNDS
        }
        "x" | "y" | "width" | "height" | "rx" | "ry" | "cx" | "cy" | "r" | "x1" | "y1" | "x2" | "y2" | "points" | "d" => {
            SVGInvalidationFlags::GEOMETRY |
                SVGInvalidationFlags::BOUNDS |
                SVGInvalidationFlags::HIT_TEST
        }
        "textLength" | "lengthAdjust" | "rotate" | "dx" | "dy" | "text-anchor" | "alignment-baseline" | "dominant-baseline" => {
            SVGInvalidationFlags::GEOMETRY |
                SVGInvalidationFlags::BOUNDS |
                SVGInvalidationFlags::HIT_TEST
        }
        _ => SVGInvalidationFlags::PAINT,
    }
}

pub fn propagate_invalidation(
    graph: &SVGResourceGraph,
    root: SVGInvalidationRoot,
    flags: SVGInvalidationFlags,
) -> SVGInvalidationSet {
    let mut set = SVGInvalidationSet::default();
    let mut queue = VecDeque::from([(root, flags)]);

    while let Some((root, flags)) = queue.pop_front() {
        match root {
            SVGInvalidationRoot::Node(node) => {
                if !set.add_node(node, flags) {
                    continue;
                }

                if let Some(info) = graph.node_info(node) {
                    if info.establishes_viewport && flags.contains(SVGInvalidationFlags::TRANSFORM) {
                        let descendant_flags =
                            SVGInvalidationFlags::TRANSFORM |
                                SVGInvalidationFlags::BOUNDS |
                                SVGInvalidationFlags::HIT_TEST;
                        for descendant in graph.descendants_of(node) {
                            queue.push_back((SVGInvalidationRoot::Node(descendant), descendant_flags));
                        }
                    }
                    if let Some(resource) = info.defined_resource {
                        queue.push_back((
                            SVGInvalidationRoot::Resource(resource),
                            flags | SVGInvalidationFlags::RESOURCE_DEPENDENCY,
                        ));
                    }
                }

                for dependency in graph.reverse_dependencies_for_node(node) {
                    queue.push_back((
                        convert_target(dependency.target),
                        flags_for_dependency(dependency.kind, flags),
                    ));
                }
            }
            SVGInvalidationRoot::Resource(resource) => {
                if !set.add_resource(resource, flags) {
                    continue;
                }
                for dependency in graph.reverse_dependencies_for_resource(resource) {
                    queue.push_back((
                        convert_target(dependency.target),
                        flags_for_dependency(dependency.kind, flags),
                    ));
                }
            }
        }
    }

    set
}

fn convert_target(target: SVGDependencyTarget) -> SVGInvalidationRoot {
    match target {
        SVGDependencyTarget::Node(node) => SVGInvalidationRoot::Node(node),
        SVGDependencyTarget::Resource(resource) => SVGInvalidationRoot::Resource(resource),
    }
}

fn flags_for_dependency(
    kind: SVGDependencyKind,
    source_flags: SVGInvalidationFlags,
) -> SVGInvalidationFlags {
    let propagated = match kind {
        SVGDependencyKind::PaintServer |
        SVGDependencyKind::GradientContent |
        SVGDependencyKind::GradientTemplate => {
            SVGInvalidationFlags::PAINT | SVGInvalidationFlags::RESOURCE_DEPENDENCY
        }
        SVGDependencyKind::ClipPath |
        SVGDependencyKind::Mask |
        SVGDependencyKind::Filter |
        SVGDependencyKind::Marker |
        SVGDependencyKind::Pattern => {
            SVGInvalidationFlags::PAINT |
                SVGInvalidationFlags::RESOURCE_DEPENDENCY |
                SVGInvalidationFlags::BOUNDS |
                SVGInvalidationFlags::HIT_TEST
        }
        SVGDependencyKind::UseSource => {
            SVGInvalidationFlags::PAINT |
                SVGInvalidationFlags::GEOMETRY |
                SVGInvalidationFlags::TRANSFORM |
                SVGInvalidationFlags::RESOURCE_DEPENDENCY |
                SVGInvalidationFlags::BOUNDS |
                SVGInvalidationFlags::HIT_TEST
        }
        SVGDependencyKind::TextPathSource => {
            SVGInvalidationFlags::PAINT |
                SVGInvalidationFlags::GEOMETRY |
                SVGInvalidationFlags::BOUNDS
        }
    };
    propagated | (source_flags & SVGInvalidationFlags::GEOMETRY)
}

fn add_flags<K: Eq + std::hash::Hash + Copy>(
    map: &mut FxHashMap<K, SVGInvalidationFlags>,
    key: K,
    flags: SVGInvalidationFlags,
) -> bool {
    let entry = map.entry(key).or_default();
    let previous = *entry;
    *entry |= flags;
    *entry != previous
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::svg::resources::{SVGResourceGraph, SVGResourceGraphNode, SVGResourceReferenceInputs};

    fn node(id: usize, kind: SVGLayoutNodeKind) -> SVGResourceGraphNode {
        SVGResourceGraphNode::new(OpaqueNode(id), kind)
    }

    #[test]
    fn stop_changes_invalidate_gradient_users() {
        let graph = SVGResourceGraph::build(&[
            node(1, SVGLayoutNodeKind::Viewport),
            node(2, SVGLayoutNodeKind::Gradient)
                .with_parent(OpaqueNode(1))
                .with_element_id("grad"),
            node(3, SVGLayoutNodeKind::Stop).with_parent(OpaqueNode(2)),
            SVGResourceGraphNode {
                node: OpaqueNode(4),
                parent: Some(OpaqueNode(1)),
                kind: SVGLayoutNodeKind::Geometry,
                element_id: None,
                fill_paint_server: Some("url(#grad)".to_string()),
                stroke_paint_server: None,
                resources: SVGResourceReferenceInputs::default(),
                href: None,
                defined_resource: None,
                establishes_viewport: false,
                participates_in_paint: true,
            },
        ]);

        let invalidation = propagate_invalidation(
            &graph,
            SVGInvalidationRoot::Node(OpaqueNode(3)),
            classify_attribute_invalidation(SVGLayoutNodeKind::Stop, "stop-color"),
        );

        assert!(
            invalidation
                .node_flags(OpaqueNode(4))
                .contains(SVGInvalidationFlags::PAINT)
        );
        assert!(
            invalidation
                .node_flags(OpaqueNode(4))
                .contains(SVGInvalidationFlags::RESOURCE_DEPENDENCY)
        );
    }

    #[test]
    fn referenced_subtree_changes_invalidate_use_instances() {
        let graph = SVGResourceGraph::build(&[
            node(1, SVGLayoutNodeKind::Viewport),
            node(2, SVGLayoutNodeKind::Geometry)
                .with_parent(OpaqueNode(1))
                .with_element_id("shape"),
            node(3, SVGLayoutNodeKind::Use)
                .with_parent(OpaqueNode(1))
                .with_href("#shape"),
        ]);

        let invalidation = propagate_invalidation(
            &graph,
            SVGInvalidationRoot::Node(OpaqueNode(2)),
            classify_attribute_invalidation(SVGLayoutNodeKind::Geometry, "d"),
        );

        let flags = invalidation.node_flags(OpaqueNode(3));
        assert!(flags.contains(SVGInvalidationFlags::GEOMETRY));
        assert!(flags.contains(SVGInvalidationFlags::TRANSFORM));
        assert!(flags.contains(SVGInvalidationFlags::BOUNDS));
    }

    #[test]
    fn clip_path_changes_invalidate_clipped_users() {
        let graph = SVGResourceGraph::build(&[
            node(1, SVGLayoutNodeKind::Viewport),
            node(2, SVGLayoutNodeKind::ClipPath)
                .with_parent(OpaqueNode(1))
                .with_element_id("clip"),
            node(3, SVGLayoutNodeKind::Geometry).with_parent(OpaqueNode(2)),
            SVGResourceGraphNode {
                node: OpaqueNode(4),
                parent: Some(OpaqueNode(1)),
                kind: SVGLayoutNodeKind::Geometry,
                element_id: None,
                fill_paint_server: None,
                stroke_paint_server: None,
                resources: SVGResourceReferenceInputs {
                    clip_path: Some("url(#clip)".to_string()),
                    ..Default::default()
                },
                href: None,
                defined_resource: None,
                establishes_viewport: false,
                participates_in_paint: true,
            },
        ]);

        let invalidation = propagate_invalidation(
            &graph,
            SVGInvalidationRoot::Node(OpaqueNode(3)),
            classify_attribute_invalidation(SVGLayoutNodeKind::Geometry, "d"),
        );

        let flags = invalidation.node_flags(OpaqueNode(4));
        assert!(flags.contains(SVGInvalidationFlags::PAINT));
        assert!(flags.contains(SVGInvalidationFlags::BOUNDS));
        assert!(flags.contains(SVGInvalidationFlags::HIT_TEST));
    }

    #[test]
    fn text_position_mutations_are_geometry_invalidations() {
        let flags = classify_attribute_invalidation(SVGLayoutNodeKind::Text, "x");
        assert!(flags.contains(SVGInvalidationFlags::GEOMETRY));
        assert!(flags.contains(SVGInvalidationFlags::BOUNDS));
        assert!(flags.contains(SVGInvalidationFlags::HIT_TEST));
    }

    #[test]
    fn resource_reference_mutations_invalidate_dependencies() {
        let flags = classify_attribute_invalidation(SVGLayoutNodeKind::Geometry, "clip-path");
        assert!(flags.contains(SVGInvalidationFlags::PAINT));
        assert!(flags.contains(SVGInvalidationFlags::RESOURCE_DEPENDENCY));
        assert!(flags.contains(SVGInvalidationFlags::BOUNDS));
        assert!(flags.contains(SVGInvalidationFlags::HIT_TEST));
    }

    #[test]
    fn view_box_changes_invalidate_descendant_transforms() {
        let graph = SVGResourceGraph::build(&[
            node(1, SVGLayoutNodeKind::Viewport),
            node(2, SVGLayoutNodeKind::Group).with_parent(OpaqueNode(1)),
            node(3, SVGLayoutNodeKind::Geometry).with_parent(OpaqueNode(2)),
        ]);

        let invalidation = propagate_invalidation(
            &graph,
            SVGInvalidationRoot::Node(OpaqueNode(1)),
            classify_attribute_invalidation(SVGLayoutNodeKind::Viewport, "viewBox"),
        );

        let flags = invalidation.node_flags(OpaqueNode(3));
        assert!(flags.contains(SVGInvalidationFlags::TRANSFORM));
        assert!(flags.contains(SVGInvalidationFlags::BOUNDS));
        assert!(flags.contains(SVGInvalidationFlags::HIT_TEST));
    }

    // Phase 3 tests: pattern/filter/marker/textPath invalidation

    #[test]
    fn pattern_reference_mutations_invalidate_dependencies() {
        let flags = classify_attribute_invalidation(SVGLayoutNodeKind::Pattern, "href");
        assert!(flags.contains(SVGInvalidationFlags::PAINT));
        assert!(flags.contains(SVGInvalidationFlags::RESOURCE_DEPENDENCY));
    }

    #[test]
    fn filter_attribute_mutations_invalidate_dependencies() {
        let flags = classify_attribute_invalidation(SVGLayoutNodeKind::Filter, "filterUnits");
        assert!(flags.contains(SVGInvalidationFlags::PAINT));
        assert!(flags.contains(SVGInvalidationFlags::RESOURCE_DEPENDENCY));
        assert!(flags.contains(SVGInvalidationFlags::BOUNDS));
    }

    #[test]
    fn marker_attribute_mutations_invalidate_dependencies() {
        let flags = classify_attribute_invalidation(SVGLayoutNodeKind::Marker, "markerWidth");
        assert!(flags.contains(SVGInvalidationFlags::PAINT));
        assert!(flags.contains(SVGInvalidationFlags::RESOURCE_DEPENDENCY));
        assert!(flags.contains(SVGInvalidationFlags::BOUNDS));
    }

    #[test]
    fn textpath_attribute_mutations_invalidate_geometry() {
        let href_flags = classify_attribute_invalidation(SVGLayoutNodeKind::TextPath, "href");
        assert!(href_flags.contains(SVGInvalidationFlags::PAINT));
        assert!(href_flags.contains(SVGInvalidationFlags::GEOMETRY));
        assert!(href_flags.contains(SVGInvalidationFlags::BOUNDS));

        let offset_flags =
            classify_attribute_invalidation(SVGLayoutNodeKind::TextPath, "startOffset");
        assert!(offset_flags.contains(SVGInvalidationFlags::GEOMETRY));
        assert!(offset_flags.contains(SVGInvalidationFlags::BOUNDS));
    }

    #[test]
    fn textpath_source_changes_invalidate_textpath_node() {
        let graph = SVGResourceGraph::build(&[
            node(1, SVGLayoutNodeKind::Viewport),
            node(2, SVGLayoutNodeKind::Geometry)
                .with_parent(OpaqueNode(1))
                .with_element_id("curve"),
            node(3, SVGLayoutNodeKind::Text).with_parent(OpaqueNode(1)),
            SVGResourceGraphNode {
                node: OpaqueNode(4),
                parent: Some(OpaqueNode(3)),
                kind: SVGLayoutNodeKind::TextPath,
                element_id: None,
                fill_paint_server: None,
                stroke_paint_server: None,
                resources: SVGResourceReferenceInputs::default(),
                href: Some("curve".to_string()),
                defined_resource: None,
                establishes_viewport: false,
                participates_in_paint: true,
            },
        ]);

        let invalidation = propagate_invalidation(
            &graph,
            SVGInvalidationRoot::Node(OpaqueNode(2)),
            classify_attribute_invalidation(SVGLayoutNodeKind::Geometry, "d"),
        );

        let flags = invalidation.node_flags(OpaqueNode(4));
        assert!(flags.contains(SVGInvalidationFlags::PAINT));
        assert!(flags.contains(SVGInvalidationFlags::GEOMETRY));
        assert!(flags.contains(SVGInvalidationFlags::BOUNDS));
    }
}
