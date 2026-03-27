use havi_types::fragment_tree::SVGRect;

#[derive(Clone, Debug, Default)]
pub struct SVGForeignObjectLayoutResult {
    pub viewport_rect: Option<SVGRect>,
}
