use std::collections::HashMap;
use std::sync::Arc;

use app_units::Au;
use style::dom::OpaqueNode;
use style::selector_parser::PseudoElement;

use super::{
    BaseFragment, FragmentDerivedData, FragmentId, FragmentKind, FragmentNode, OutOfFlowPlacement,
    PaintChild, PlacementId,
};
use crate::geom::PhysicalRect;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct FragmentMapKey {
    pub node: OpaqueNode,
    pub pseudo: Option<PseudoElement>,
}

#[derive(Clone, Debug)]
pub struct FragmentArenaGeneration {
    pub geometry_roots: Arc<[FragmentId]>,
    pub paint_roots: Arc<[PaintChild]>,
    pub nodes: Arc<[FragmentNode]>,
    pub placements: Arc<[OutOfFlowPlacement]>,
    pub derived: FragmentDerivedData,
    pub node_fragments: HashMap<FragmentMapKey, Arc<[FragmentId]>>,
    pub initial_containing_block: PhysicalRect<Au>,
    pub scrollable_overflow: PhysicalRect<Au>,
}

impl FragmentArenaGeneration {
    pub fn node(&self, id: FragmentId) -> &FragmentNode {
        &self.nodes[id.0 as usize]
    }

    pub fn base(&self, id: FragmentId) -> &BaseFragment {
        self.node(id).base()
    }

    pub fn kind(&self, id: FragmentId) -> &FragmentKind {
        &self.node(id).kind
    }

    pub fn placement(&self, id: PlacementId) -> &OutOfFlowPlacement {
        &self.placements[id.0 as usize]
    }

    pub fn geometry_children(&self, id: FragmentId) -> &[FragmentId] {
        match self.kind(id) {
            FragmentKind::Box(fragment) | FragmentKind::Float(fragment) => {
                fragment.geometry_children.as_slice()
            }
            FragmentKind::Positioning(fragment) => fragment.geometry_children.as_slice(),
            FragmentKind::Text(_) | FragmentKind::Image(_) | FragmentKind::IFrame(_) => &[],
        }
    }

    pub fn paint_children(&self, id: FragmentId) -> &[PaintChild] {
        match self.kind(id) {
            FragmentKind::Box(fragment) | FragmentKind::Float(fragment) => {
                fragment.paint_children.as_slice()
            }
            FragmentKind::Positioning(fragment) => fragment.paint_children.as_slice(),
            FragmentKind::Text(_) | FragmentKind::Image(_) | FragmentKind::IFrame(_) => &[],
        }
    }

    pub fn fragments_for_node(
        &self,
        node: OpaqueNode,
        pseudo: Option<PseudoElement>,
    ) -> &[FragmentId] {
        self.node_fragments
            .get(&FragmentMapKey { node, pseudo })
            .map(|fragments| fragments.as_ref())
            .unwrap_or(&[])
    }

    pub fn containing_block(&self, id: FragmentId) -> PhysicalRect<Au> {
        self.derived.containing_blocks[id.0 as usize]
    }

    pub fn scrollable_overflow_for(&self, id: FragmentId) -> PhysicalRect<Au> {
        self.derived.scrollable_overflow[id.0 as usize]
    }

    pub fn sticky_insets_for(
        &self,
        id: FragmentId,
    ) -> Option<&crate::geom::PhysicalSides<crate::geom::AuOrAuto>> {
        self.derived.sticky_insets[id.0 as usize].as_ref()
    }

    pub fn background_images_for(
        &self,
        id: FragmentId,
    ) -> &[Option<super::BackgroundImage>] {
        self.derived.background_images[id.0 as usize].as_slice()
    }
}
