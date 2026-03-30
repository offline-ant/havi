use std::collections::VecDeque;

use havi_types::fragment_tree::{
    SVGClipPathResource, SVGCoordinateUnits, SVGEffectState, SVGGradientKind,
    SVGGradientResource, SVGGradientSpreadMethod, SVGLinearGradient, SVGMaskResource,
    SVGPaintServerResource, SVGPathData, SVGPoint, SVGRect, SVGResourceId, SVGResourceKind,
    SVGResourceNode, SVGTransform, SVGUseInstanceSource,
};
use rustc_hash::FxHashMap;
use style::dom::OpaqueNode;

use super::dom::SVGLayoutNodeKind;

const EMPTY_NODE_LIST: [OpaqueNode; 0] = [];
const EMPTY_DEPENDENCIES: [SVGDependency; 0] = [];

#[derive(Clone, Debug)]
pub struct SVGResourceGraphNode {
    pub node: OpaqueNode,
    pub parent: Option<OpaqueNode>,
    pub kind: SVGLayoutNodeKind,
    pub element_id: Option<String>,
    pub fill_paint_server: Option<String>,
    pub stroke_paint_server: Option<String>,
    pub resources: SVGResourceReferenceInputs,
    pub href: Option<String>,
    pub defined_resource: Option<SVGResourceNode>,
    pub establishes_viewport: bool,
    pub participates_in_paint: bool,
}

impl SVGResourceGraphNode {
    pub fn new(node: OpaqueNode, kind: SVGLayoutNodeKind) -> Self {
        Self {
            node,
            parent: None,
            kind,
            element_id: None,
            fill_paint_server: None,
            stroke_paint_server: None,
            resources: SVGResourceReferenceInputs::default(),
            href: None,
            defined_resource: None,
            establishes_viewport: matches!(kind, SVGLayoutNodeKind::Viewport),
            participates_in_paint: !matches!(
                kind,
                SVGLayoutNodeKind::Defs
                    | SVGLayoutNodeKind::Pattern
                    | SVGLayoutNodeKind::Filter
                    | SVGLayoutNodeKind::Marker
            ),
        }
    }

    pub fn with_parent(mut self, parent: OpaqueNode) -> Self {
        self.parent = Some(parent);
        self
    }

    pub fn with_element_id(mut self, element_id: impl Into<String>) -> Self {
        self.element_id = Some(element_id.into());
        self
    }

    pub fn with_href(mut self, href: impl Into<String>) -> Self {
        self.href = Some(href.into());
        self
    }

    pub fn with_fill_paint_server(mut self, iri: impl Into<String>) -> Self {
        self.fill_paint_server = Some(iri.into());
        self
    }

    pub fn with_stroke_paint_server(mut self, iri: impl Into<String>) -> Self {
        self.stroke_paint_server = Some(iri.into());
        self
    }

    pub fn with_defined_resource(mut self, resource: SVGResourceNode) -> Self {
        self.defined_resource = Some(resource);
        self
    }
}

#[derive(Clone, Debug, Default)]
pub struct SVGResourceReferenceInputs {
    pub clip_path: Option<String>,
    pub mask: Option<String>,
    pub filter: Option<String>,
    pub marker_start: Option<String>,
    pub marker_mid: Option<String>,
    pub marker_end: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SVGDependencySource {
    Node(OpaqueNode),
    Resource(SVGResourceId),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SVGDependencyTarget {
    Node(OpaqueNode),
    Resource(SVGResourceId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGDependencyKind {
    PaintServer,
    ClipPath,
    Mask,
    Filter,
    Marker,
    Pattern,
    GradientContent,
    GradientTemplate,
    UseSource,
    TextPathSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SVGDependency {
    pub kind: SVGDependencyKind,
    pub target: SVGDependencyTarget,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SVGPaintServerUses {
    pub fill: Option<SVGResourceId>,
    pub stroke: Option<SVGResourceId>,
}

#[derive(Clone, Debug, Default)]
pub struct SVGResolvedNodeResources {
    pub paint_servers: SVGPaintServerUses,
    pub resources: SVGEffectState,
    pub use_instance_source: Option<SVGResourceId>,
    pub referenced_node: Option<OpaqueNode>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SVGGraphNodeInfo {
    pub kind: SVGLayoutNodeKind,
    pub establishes_viewport: bool,
    pub participates_in_paint: bool,
    pub defined_resource: Option<SVGResourceId>,
}

#[derive(Clone, Debug, Default)]
pub struct SVGResourceGraph {
    pub resources: Vec<SVGResourceNode>,
    pub resource_owners: FxHashMap<SVGResourceId, OpaqueNode>,
    pub resource_ids_by_node: FxHashMap<OpaqueNode, SVGResourceId>,
    pub resources_by_element_id: FxHashMap<String, SVGResourceId>,
    pub node_info: FxHashMap<OpaqueNode, SVGGraphNodeInfo>,
    pub node_resources: FxHashMap<OpaqueNode, SVGResolvedNodeResources>,
    pub reverse_dependencies: FxHashMap<SVGDependencySource, Vec<SVGDependency>>,
    pub parent_by_node: FxHashMap<OpaqueNode, Option<OpaqueNode>>,
    pub children_by_node: FxHashMap<OpaqueNode, Vec<OpaqueNode>>,
    pub nodes_by_element_id: FxHashMap<String, OpaqueNode>,
}

impl SVGResourceGraph {
    pub fn build(nodes: &[SVGResourceGraphNode]) -> Self {
        let mut graph = Self::default();
        for node in nodes {
            graph.parent_by_node.insert(node.node, node.parent);
            graph
                .children_by_node
                .entry(node.node)
                .or_default();
            if let Some(parent) = node.parent {
                graph.children_by_node.entry(parent).or_default().push(node.node);
            }
            if let Some(element_id) = &node.element_id {
                graph.nodes_by_element_id.insert(element_id.clone(), node.node);
            }
        }

        for node in nodes {
            let defined_resource = node
                .defined_resource
                .clone()
                .or_else(|| default_resource_for_kind(node.kind));
            let defined_resource = defined_resource.map(|resource| {
                let id = SVGResourceId(graph.resources.len() as u32);
                graph.resources.push(resource);
                graph.resource_owners.insert(id, node.node);
                graph.resource_ids_by_node.insert(node.node, id);
                if let Some(element_id) = &node.element_id {
                    graph.resources_by_element_id.insert(element_id.clone(), id);
                }
                id
            });
            graph.node_info.insert(
                node.node,
                SVGGraphNodeInfo {
                    kind: node.kind,
                    establishes_viewport: node.establishes_viewport,
                    participates_in_paint: node.participates_in_paint,
                    defined_resource,
                },
            );
            graph.node_resources.insert(node.node, SVGResolvedNodeResources::default());
        }

        for node in nodes {
            let descendant_resource_owner = resource_owner_for_descendant(node.node, &graph);
            if let Some(owner) = descendant_resource_owner {
                if owner != node.node {
                    if let Some(resource_id) = graph.resource_ids_by_node.get(&owner).copied() {
                        if let Some(kind) = dependency_kind_for_resource_contents(graph.resource(resource_id)) {
                            graph.push_dependency(
                                SVGDependencySource::Node(node.node),
                                kind,
                                SVGDependencyTarget::Resource(resource_id),
                            );
                        }
                    }
                }
            }
        }

        for node in nodes {
            graph.resolve_node_references(node);
        }

        graph
    }

    pub fn resource(&self, id: SVGResourceId) -> Option<&SVGResourceKind> {
        self.resources.get(id.0 as usize).map(|node| &node.kind)
    }

    pub fn resource_mut(&mut self, id: SVGResourceId) -> Option<&mut SVGResourceKind> {
        self.resources.get_mut(id.0 as usize).map(|node| &mut node.kind)
    }

    pub fn resources(&self) -> &[SVGResourceNode] {
        &self.resources
    }

    pub fn resource_id_for_node(&self, node: OpaqueNode) -> Option<SVGResourceId> {
        self.resource_ids_by_node.get(&node).copied()
    }

    pub fn node_for_element_id(&self, id: &str) -> Option<OpaqueNode> {
        self.nodes_by_element_id.get(id).copied()
    }

    pub fn resource_owner(&self, id: SVGResourceId) -> Option<OpaqueNode> {
        self.resource_owners.get(&id).copied()
    }

    pub fn resource_for_element_id(&self, id: &str) -> Option<SVGResourceId> {
        self.resources_by_element_id.get(id).copied()
    }

    pub fn node_info(&self, node: OpaqueNode) -> Option<SVGGraphNodeInfo> {
        self.node_info.get(&node).copied()
    }

    pub fn node_resources(&self, node: OpaqueNode) -> Option<&SVGResolvedNodeResources> {
        self.node_resources.get(&node)
    }

    pub fn reverse_dependencies_for_node(&self, node: OpaqueNode) -> &[SVGDependency] {
        self.reverse_dependencies
            .get(&SVGDependencySource::Node(node))
            .map(Vec::as_slice)
            .unwrap_or(&EMPTY_DEPENDENCIES)
    }

    pub fn reverse_dependencies_for_resource(&self, resource: SVGResourceId) -> &[SVGDependency] {
        self.reverse_dependencies
            .get(&SVGDependencySource::Resource(resource))
            .map(Vec::as_slice)
            .unwrap_or(&EMPTY_DEPENDENCIES)
    }

    pub fn children_of(&self, node: OpaqueNode) -> &[OpaqueNode] {
        self.children_by_node
            .get(&node)
            .map(Vec::as_slice)
            .unwrap_or(&EMPTY_NODE_LIST)
    }

    pub fn descendants_of(&self, node: OpaqueNode) -> Vec<OpaqueNode> {
        let mut descendants = Vec::new();
        let mut queue: VecDeque<_> = self.children_of(node).iter().copied().collect();
        while let Some(current) = queue.pop_front() {
            descendants.push(current);
            queue.extend(self.children_of(current).iter().copied());
        }
        descendants
    }

    fn resolve_node_references(&mut self, node: &SVGResourceGraphNode) {
        let mut resolved = SVGResolvedNodeResources {
            paint_servers: SVGPaintServerUses {
                fill: node
                    .fill_paint_server
                    .as_deref()
                    .and_then(normalize_local_reference)
                    .and_then(|id| self.resource_for_element_id(id)),
                stroke: node
                    .stroke_paint_server
                    .as_deref()
                    .and_then(normalize_local_reference)
                    .and_then(|id| self.resource_for_element_id(id)),
            },
            resources: SVGEffectState {
                clip_path: node
                    .resources
                    .clip_path
                    .as_deref()
                    .and_then(normalize_local_reference)
                    .and_then(|id| self.resource_for_element_id(id)),
                mask: node
                    .resources
                    .mask
                    .as_deref()
                    .and_then(normalize_local_reference)
                    .and_then(|id| self.resource_for_element_id(id)),
                filter: node
                    .resources
                    .filter
                    .as_deref()
                    .and_then(normalize_local_reference)
                    .and_then(|id| self.resource_for_element_id(id)),
                marker_start: node
                    .resources
                    .marker_start
                    .as_deref()
                    .and_then(normalize_local_reference)
                    .and_then(|id| self.resource_for_element_id(id)),
                marker_mid: node
                    .resources
                    .marker_mid
                    .as_deref()
                    .and_then(normalize_local_reference)
                    .and_then(|id| self.resource_for_element_id(id)),
                marker_end: node
                    .resources
                    .marker_end
                    .as_deref()
                    .and_then(normalize_local_reference)
                    .and_then(|id| self.resource_for_element_id(id)),
            },
            use_instance_source: None,
            referenced_node: None,
        };

        if matches!(node.kind, SVGLayoutNodeKind::Use) {
            resolved.referenced_node = node
                .href
                .as_deref()
                .and_then(normalize_local_reference)
                .and_then(|id| self.nodes_by_element_id.get(id).copied());
            if let Some(referenced_node) = resolved.referenced_node {
                let resource_id = SVGResourceId(self.resources.len() as u32);
                self.resources.push(SVGResourceNode {
                    kind: SVGResourceKind::UseInstanceSource(SVGUseInstanceSource {
                        source_fragment_roots: Vec::new(),
                        source_resource_dependencies: Vec::new(),
                    }),
                });
                self.resource_owners.insert(resource_id, node.node);
                self.resource_ids_by_node.insert(node.node, resource_id);
                resolved.use_instance_source = Some(resource_id);
                self.push_dependency(
                    SVGDependencySource::Node(referenced_node),
                    SVGDependencyKind::UseSource,
                    SVGDependencyTarget::Resource(resource_id),
                );
                self.push_dependency(
                    SVGDependencySource::Resource(resource_id),
                    SVGDependencyKind::UseSource,
                    SVGDependencyTarget::Node(node.node),
                );
            }
        }

        self.node_resources.insert(node.node, resolved.clone());

        if let Some(fill) = resolved.paint_servers.fill {
            self.push_dependency(
                SVGDependencySource::Resource(fill),
                SVGDependencyKind::PaintServer,
                SVGDependencyTarget::Node(node.node),
            );
        }
        if let Some(stroke) = resolved.paint_servers.stroke {
            self.push_dependency(
                SVGDependencySource::Resource(stroke),
                SVGDependencyKind::PaintServer,
                SVGDependencyTarget::Node(node.node),
            );
        }

        push_optional_resource_dependency(
            self,
            node.node,
            resolved.resources.clip_path,
            SVGDependencyKind::ClipPath,
        );
        push_optional_resource_dependency(
            self,
            node.node,
            resolved.resources.mask,
            SVGDependencyKind::Mask,
        );
        push_optional_resource_dependency(
            self,
            node.node,
            resolved.resources.filter,
            SVGDependencyKind::Filter,
        );
        push_optional_resource_dependency(
            self,
            node.node,
            resolved.resources.marker_start,
            SVGDependencyKind::Marker,
        );
        push_optional_resource_dependency(
            self,
            node.node,
            resolved.resources.marker_mid,
            SVGDependencyKind::Marker,
        );
        push_optional_resource_dependency(
            self,
            node.node,
            resolved.resources.marker_end,
            SVGDependencyKind::Marker,
        );

        if matches!(node.kind, SVGLayoutNodeKind::Gradient) {
            let Some(owner_resource) = self.resource_ids_by_node.get(&node.node).copied() else {
                return;
            };
            let template = node
                .href
                .as_deref()
                .and_then(normalize_local_reference)
                .and_then(|id| self.resource_for_element_id(id));
            if let Some(template) = template {
                self.push_dependency(
                    SVGDependencySource::Resource(template),
                    SVGDependencyKind::GradientTemplate,
                    SVGDependencyTarget::Resource(owner_resource),
                );
            }
        }

        if matches!(node.kind, SVGLayoutNodeKind::Pattern) {
            let Some(owner_resource) = self.resource_ids_by_node.get(&node.node).copied() else {
                return;
            };
            let template = node
                .href
                .as_deref()
                .and_then(normalize_local_reference)
                .and_then(|id| self.resource_for_element_id(id));
            if let Some(template) = template {
                self.push_dependency(
                    SVGDependencySource::Resource(template),
                    SVGDependencyKind::Pattern,
                    SVGDependencyTarget::Resource(owner_resource),
                );
            }
        }

        if matches!(node.kind, SVGLayoutNodeKind::TextPath) {
            let referenced = node
                .href
                .as_deref()
                .and_then(normalize_local_reference)
                .and_then(|id| self.nodes_by_element_id.get(id).copied());
            if let Some(path_node) = referenced {
                self.push_dependency(
                    SVGDependencySource::Node(path_node),
                    SVGDependencyKind::TextPathSource,
                    SVGDependencyTarget::Node(node.node),
                );
            }
        }
    }

    fn push_dependency(
        &mut self,
        source: SVGDependencySource,
        kind: SVGDependencyKind,
        target: SVGDependencyTarget,
    ) {
        let dependencies = self.reverse_dependencies.entry(source).or_default();
        if dependencies
            .iter()
            .any(|dependency| dependency.kind == kind && dependency.target == target)
        {
            return;
        }
        dependencies.push(SVGDependency { kind, target });
    }
}

fn push_optional_resource_dependency(
    graph: &mut SVGResourceGraph,
    node: OpaqueNode,
    resource: Option<SVGResourceId>,
    kind: SVGDependencyKind,
) {
    if let Some(resource) = resource {
        graph.push_dependency(
            SVGDependencySource::Resource(resource),
            kind,
            SVGDependencyTarget::Node(node),
        );
    }
}

fn resource_owner_for_descendant(
    node: OpaqueNode,
    graph: &SVGResourceGraph,
) -> Option<OpaqueNode> {
    let mut current = graph.parent_by_node.get(&node).copied().flatten();
    while let Some(parent) = current {
        if graph.resource_ids_by_node.contains_key(&parent) {
            return Some(parent);
        }
        current = graph.parent_by_node.get(&parent).copied().flatten();
    }
    None
}

fn dependency_kind_for_resource_contents(resource: Option<&SVGResourceKind>) -> Option<SVGDependencyKind> {
    match resource? {
        SVGResourceKind::PaintServer(SVGPaintServerResource::Gradient(_)) => Some(SVGDependencyKind::GradientContent),
        SVGResourceKind::PaintServer(SVGPaintServerResource::Pattern(_)) => Some(SVGDependencyKind::Pattern),
        SVGResourceKind::ClipPath(_) => Some(SVGDependencyKind::ClipPath),
        SVGResourceKind::Mask(_) => Some(SVGDependencyKind::Mask),
        SVGResourceKind::Filter(_) => Some(SVGDependencyKind::Filter),
        SVGResourceKind::Marker(_) => Some(SVGDependencyKind::Marker),
        SVGResourceKind::UseInstanceSource(_) => None,
    }
}

fn normalize_local_reference(raw: &str) -> Option<&str> {
    let raw = raw.trim();
    if let Some(stripped) = raw.strip_prefix('#') {
        return Some(stripped.trim());
    }
    if let Some(inner) = raw.strip_prefix("url(").and_then(|raw| raw.strip_suffix(')')) {
        let inner = inner.trim();
        if let Some(stripped) = inner.strip_prefix('#') {
            return Some(stripped.trim_matches(|ch| ch == '\'' || ch == '"' || ch == ' '));
        }
        let inner = inner.trim_matches(|ch| ch == '\'' || ch == '"' || ch == ' ');
        return inner.strip_prefix('#');
    }
    Some(raw.trim_matches(|ch| ch == '\'' || ch == '"' || ch == ' '))
}

fn default_resource_for_kind(kind: SVGLayoutNodeKind) -> Option<SVGResourceNode> {
    let resource = match kind {
        SVGLayoutNodeKind::Gradient => SVGResourceKind::PaintServer(SVGPaintServerResource::Gradient(SVGGradientResource {
            units: SVGCoordinateUnits::ObjectBoundingBox,
            gradient_transform: SVGTransform::identity(),
            spread_method: SVGGradientSpreadMethod::Pad,
            kind: SVGGradientKind::Linear(SVGLinearGradient {
                start: SVGPoint::new(0.0, 0.0),
                end: SVGPoint::new(1.0, 0.0),
            }),
            stops: Vec::new(),
        })),
        SVGLayoutNodeKind::ClipPath => SVGResourceKind::ClipPath(SVGClipPathResource {
            units: SVGCoordinateUnits::UserSpaceOnUse,
            transform: SVGTransform::identity(),
            paths: Vec::<SVGPathData>::new(),
        }),
        SVGLayoutNodeKind::Mask => SVGResourceKind::Mask(SVGMaskResource {
            units: SVGCoordinateUnits::ObjectBoundingBox,
            content_units: SVGCoordinateUnits::UserSpaceOnUse,
            rect: SVGRect::zero(),
        }),
        SVGLayoutNodeKind::Pattern => SVGResourceKind::PaintServer(SVGPaintServerResource::Pattern(
            havi_types::fragment_tree::SVGPatternResource {
                units: SVGCoordinateUnits::ObjectBoundingBox,
                content_units: SVGCoordinateUnits::UserSpaceOnUse,
                pattern_transform: SVGTransform::identity(),
                rect: havi_types::fragment_tree::SVGPatternRect::default(),
                view_box: None,
                preserve_aspect_ratio: havi_types::fragment_tree::SVGPreserveAspectRatio::default(),
                source_fragment_roots: Vec::new(),
                source_resource_dependencies: Vec::new(),
            },
        )),
        SVGLayoutNodeKind::Filter => SVGResourceKind::Filter(
            havi_types::fragment_tree::SVGFilterResource {
                rect: SVGRect::zero(),
            },
        ),
        SVGLayoutNodeKind::Marker => SVGResourceKind::Marker(
            havi_types::fragment_tree::SVGMarkerResource {
                view_box: None,
                marker_units: SVGCoordinateUnits::UserSpaceOnUse,
                orient_auto: true,
            },
        ),
        SVGLayoutNodeKind::Defs
        | SVGLayoutNodeKind::Geometry
        | SVGLayoutNodeKind::Group
        | SVGLayoutNodeKind::Image
        | SVGLayoutNodeKind::Stop
        | SVGLayoutNodeKind::Text
        | SVGLayoutNodeKind::TextPath
        | SVGLayoutNodeKind::Viewport
        | SVGLayoutNodeKind::ForeignObject => {
            return None;
        }
        SVGLayoutNodeKind::Use => return None,
    };
    Some(SVGResourceNode { kind: resource })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: usize, kind: SVGLayoutNodeKind) -> SVGResourceGraphNode {
        SVGResourceGraphNode::new(OpaqueNode(id), kind)
    }

    #[test]
    fn resolves_gradient_and_clip_references() {
        let graph = SVGResourceGraph::build(&[
            node(1, SVGLayoutNodeKind::Viewport).with_element_id("root"),
            node(2, SVGLayoutNodeKind::Gradient)
                .with_parent(OpaqueNode(1))
                .with_element_id("grad"),
            node(3, SVGLayoutNodeKind::ClipPath)
                .with_parent(OpaqueNode(1))
                .with_element_id("clip"),
            SVGResourceGraphNode {
                node: OpaqueNode(4),
                parent: Some(OpaqueNode(1)),
                kind: SVGLayoutNodeKind::Geometry,
                element_id: None,
                fill_paint_server: Some("url(#grad)".to_string()),
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

        let resolved = graph.node_resources(OpaqueNode(4)).unwrap();
        assert_eq!(resolved.paint_servers.fill, Some(SVGResourceId(0)));
        assert_eq!(resolved.resources.clip_path, Some(SVGResourceId(1)));
        assert_eq!(
            graph.reverse_dependencies_for_resource(SVGResourceId(0)),
            &[SVGDependency {
                kind: SVGDependencyKind::PaintServer,
                target: SVGDependencyTarget::Node(OpaqueNode(4)),
            }]
        );
        assert_eq!(
            graph.reverse_dependencies_for_resource(SVGResourceId(1)),
            &[SVGDependency {
                kind: SVGDependencyKind::ClipPath,
                target: SVGDependencyTarget::Node(OpaqueNode(4)),
            }]
        );
    }

    #[test]
    fn builds_use_instance_dependencies() {
        let graph = SVGResourceGraph::build(&[
            node(1, SVGLayoutNodeKind::Viewport),
            node(2, SVGLayoutNodeKind::Geometry)
                .with_parent(OpaqueNode(1))
                .with_element_id("shape"),
            node(3, SVGLayoutNodeKind::Use)
                .with_parent(OpaqueNode(1))
                .with_href("#shape"),
        ]);

        let resolved = graph.node_resources(OpaqueNode(3)).unwrap();
        let use_resource = resolved.use_instance_source.expect("use resource");
        assert_eq!(resolved.referenced_node, Some(OpaqueNode(2)));
        assert_eq!(
            graph.reverse_dependencies_for_node(OpaqueNode(2)),
            &[SVGDependency {
                kind: SVGDependencyKind::UseSource,
                target: SVGDependencyTarget::Resource(use_resource),
            }]
        );
        assert_eq!(
            graph.reverse_dependencies_for_resource(use_resource),
            &[SVGDependency {
                kind: SVGDependencyKind::UseSource,
                target: SVGDependencyTarget::Node(OpaqueNode(3)),
            }]
        );
    }

    #[test]
    fn descendant_nodes_feed_resource_owner_dependencies() {
        let graph = SVGResourceGraph::build(&[
            node(1, SVGLayoutNodeKind::Viewport),
            node(2, SVGLayoutNodeKind::ClipPath)
                .with_parent(OpaqueNode(1))
                .with_element_id("clip"),
            node(3, SVGLayoutNodeKind::Geometry).with_parent(OpaqueNode(2)),
        ]);

        assert_eq!(
            graph.reverse_dependencies_for_node(OpaqueNode(3)),
            &[SVGDependency {
                kind: SVGDependencyKind::ClipPath,
                target: SVGDependencyTarget::Resource(SVGResourceId(0)),
            }]
        );
    }

    // Phase 3 tests: pattern, filter, marker, textPath

    #[test]
    fn pattern_node_creates_paint_server_resource() {
        let graph = SVGResourceGraph::build(&[
            node(1, SVGLayoutNodeKind::Viewport),
            node(2, SVGLayoutNodeKind::Pattern)
                .with_parent(OpaqueNode(1))
                .with_element_id("pat"),
        ]);

        let resource_id = graph.resource_for_element_id("pat").expect("pattern resource");
        assert!(matches!(
            graph.resource(resource_id),
            Some(SVGResourceKind::PaintServer(SVGPaintServerResource::Pattern(_)))
        ));
    }

    #[test]
    fn filter_node_creates_filter_resource() {
        let graph = SVGResourceGraph::build(&[
            node(1, SVGLayoutNodeKind::Viewport),
            node(2, SVGLayoutNodeKind::Filter)
                .with_parent(OpaqueNode(1))
                .with_element_id("blur"),
        ]);

        let resource_id = graph.resource_for_element_id("blur").expect("filter resource");
        assert!(matches!(
            graph.resource(resource_id),
            Some(SVGResourceKind::Filter(_))
        ));
    }

    #[test]
    fn marker_node_creates_marker_resource() {
        let graph = SVGResourceGraph::build(&[
            node(1, SVGLayoutNodeKind::Viewport),
            node(2, SVGLayoutNodeKind::Marker)
                .with_parent(OpaqueNode(1))
                .with_element_id("arrow"),
        ]);

        let resource_id = graph.resource_for_element_id("arrow").expect("marker resource");
        assert!(matches!(
            graph.resource(resource_id),
            Some(SVGResourceKind::Marker(_))
        ));
    }

    #[test]
    fn textpath_node_does_not_define_a_resource() {
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

        assert!(graph.resource_id_for_node(OpaqueNode(4)).is_none());
    }

    #[test]
    fn pattern_filter_marker_do_not_participate_in_paint() {
        let graph = SVGResourceGraph::build(&[
            node(1, SVGLayoutNodeKind::Viewport),
            node(2, SVGLayoutNodeKind::Pattern)
                .with_parent(OpaqueNode(1))
                .with_element_id("pat"),
            node(3, SVGLayoutNodeKind::Filter)
                .with_parent(OpaqueNode(1))
                .with_element_id("flt"),
            node(4, SVGLayoutNodeKind::Marker)
                .with_parent(OpaqueNode(1))
                .with_element_id("mrk"),
        ]);

        assert!(!graph.node_info(OpaqueNode(2)).unwrap().participates_in_paint);
        assert!(!graph.node_info(OpaqueNode(3)).unwrap().participates_in_paint);
        assert!(!graph.node_info(OpaqueNode(4)).unwrap().participates_in_paint);
    }

    #[test]
    fn pattern_fill_reference_creates_reverse_dependency() {
        let graph = SVGResourceGraph::build(&[
            node(1, SVGLayoutNodeKind::Viewport),
            node(2, SVGLayoutNodeKind::Pattern)
                .with_parent(OpaqueNode(1))
                .with_element_id("pat"),
            SVGResourceGraphNode {
                node: OpaqueNode(3),
                parent: Some(OpaqueNode(1)),
                kind: SVGLayoutNodeKind::Geometry,
                element_id: None,
                fill_paint_server: Some("#pat".to_string()),
                stroke_paint_server: None,
                resources: SVGResourceReferenceInputs::default(),
                href: None,
                defined_resource: None,
                establishes_viewport: false,
                participates_in_paint: true,
            },
        ]);

        let resource_id = graph.resource_for_element_id("pat").expect("pattern resource");
        let resolved = graph.node_resources(OpaqueNode(3)).unwrap();
        assert_eq!(resolved.paint_servers.fill, Some(resource_id));
        assert!(graph
            .reverse_dependencies_for_resource(resource_id)
            .iter()
            .any(|dep| dep.kind == SVGDependencyKind::PaintServer
                && dep.target == SVGDependencyTarget::Node(OpaqueNode(3))));
    }

    #[test]
    fn filter_reference_creates_reverse_dependency() {
        let graph = SVGResourceGraph::build(&[
            node(1, SVGLayoutNodeKind::Viewport),
            node(2, SVGLayoutNodeKind::Filter)
                .with_parent(OpaqueNode(1))
                .with_element_id("blur"),
            SVGResourceGraphNode {
                node: OpaqueNode(3),
                parent: Some(OpaqueNode(1)),
                kind: SVGLayoutNodeKind::Geometry,
                element_id: None,
                fill_paint_server: None,
                stroke_paint_server: None,
                resources: SVGResourceReferenceInputs {
                    filter: Some("#blur".to_string()),
                    ..Default::default()
                },
                href: None,
                defined_resource: None,
                establishes_viewport: false,
                participates_in_paint: true,
            },
        ]);

        let resource_id = graph.resource_for_element_id("blur").expect("filter resource");
        let resolved = graph.node_resources(OpaqueNode(3)).unwrap();
        assert_eq!(resolved.resources.filter, Some(resource_id));
        assert!(graph
            .reverse_dependencies_for_resource(resource_id)
            .iter()
            .any(|dep| dep.kind == SVGDependencyKind::Filter
                && dep.target == SVGDependencyTarget::Node(OpaqueNode(3))));
    }

    #[test]
    fn marker_reference_creates_reverse_dependency() {
        let graph = SVGResourceGraph::build(&[
            node(1, SVGLayoutNodeKind::Viewport),
            node(2, SVGLayoutNodeKind::Marker)
                .with_parent(OpaqueNode(1))
                .with_element_id("arrow"),
            SVGResourceGraphNode {
                node: OpaqueNode(3),
                parent: Some(OpaqueNode(1)),
                kind: SVGLayoutNodeKind::Geometry,
                element_id: None,
                fill_paint_server: None,
                stroke_paint_server: None,
                resources: SVGResourceReferenceInputs {
                    marker_start: Some("#arrow".to_string()),
                    marker_end: Some("#arrow".to_string()),
                    ..Default::default()
                },
                href: None,
                defined_resource: None,
                establishes_viewport: false,
                participates_in_paint: true,
            },
        ]);

        let resource_id = graph.resource_for_element_id("arrow").expect("marker resource");
        let resolved = graph.node_resources(OpaqueNode(3)).unwrap();
        assert_eq!(resolved.resources.marker_start, Some(resource_id));
        assert_eq!(resolved.resources.marker_end, Some(resource_id));
        let deps = graph.reverse_dependencies_for_resource(resource_id);
        assert_eq!(deps.len(), 1);
        assert!(deps.iter().all(|dep| dep.kind == SVGDependencyKind::Marker
            && dep.target == SVGDependencyTarget::Node(OpaqueNode(3))));
    }

    #[test]
    fn textpath_href_creates_textpath_source_dependency() {
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

        // The path node (2) should have a reverse dependency pointing to the textPath node (4).
        assert_eq!(
            graph.reverse_dependencies_for_node(OpaqueNode(2)),
            &[SVGDependency {
                kind: SVGDependencyKind::TextPathSource,
                target: SVGDependencyTarget::Node(OpaqueNode(4)),
            }]
        );
    }

    #[test]
    fn pattern_template_href_creates_pattern_dependency() {
        let graph = SVGResourceGraph::build(&[
            node(1, SVGLayoutNodeKind::Viewport),
            node(2, SVGLayoutNodeKind::Pattern)
                .with_parent(OpaqueNode(1))
                .with_element_id("base"),
            SVGResourceGraphNode {
                node: OpaqueNode(3),
                parent: Some(OpaqueNode(1)),
                kind: SVGLayoutNodeKind::Pattern,
                element_id: Some("derived".to_string()),
                fill_paint_server: None,
                stroke_paint_server: None,
                resources: SVGResourceReferenceInputs::default(),
                href: Some("base".to_string()),
                defined_resource: None,
                establishes_viewport: false,
                participates_in_paint: false,
            },
        ]);

        let base_id = graph.resource_for_element_id("base").expect("base pattern resource");
        let derived_id = graph.resource_for_element_id("derived").expect("derived pattern resource");

        // base change propagates to derived (the dependent pattern).
        assert!(graph
            .reverse_dependencies_for_resource(base_id)
            .iter()
            .any(|dep| dep.kind == SVGDependencyKind::Pattern
                && dep.target == SVGDependencyTarget::Resource(derived_id)));
    }
}
