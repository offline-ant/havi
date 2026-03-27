use havi_types::fragment_tree::{SVGResourceId, SVGResourceKind, SVGResourceNode};

#[derive(Clone, Debug, Default)]
pub struct SVGResourceGraph {
    pub resources: Vec<SVGResourceNode>,
}

impl SVGResourceGraph {
    pub fn resource(&self, id: SVGResourceId) -> Option<&SVGResourceKind> {
        self.resources.get(id.0 as usize).map(|node| &node.kind)
    }
}
