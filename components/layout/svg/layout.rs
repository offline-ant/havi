use havi_types::fragment_tree::SVGRect;

#[derive(Clone, Debug, Default)]
pub struct SVGViewportLayoutResult {
    pub viewport_rect: Option<SVGRect>,
    pub view_box_rect: Option<SVGRect>,
}
