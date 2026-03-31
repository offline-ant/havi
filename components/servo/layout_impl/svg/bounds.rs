use havi_types::fragment_tree::SVGRect;

#[derive(Clone, Debug, Default)]
pub struct SVGBounds {
    pub object_bounds: Option<SVGRect>,
    pub decorated_bounds: Option<SVGRect>,
}
