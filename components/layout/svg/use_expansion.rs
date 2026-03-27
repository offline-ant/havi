use havi_types::fragment_tree::SVGResourceId;

#[derive(Clone, Debug, Default)]
pub struct SVGUseExpansionResult {
    pub referenced_resource: Option<SVGResourceId>,
}
