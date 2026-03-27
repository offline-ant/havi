use havi_types::fragment_tree::SVGTransform;

#[derive(Clone, Debug)]
pub struct SVGCoordinateMapper {
    pub local_to_parent: SVGTransform,
}

impl SVGCoordinateMapper {
    pub fn identity() -> Self {
        Self {
            local_to_parent: SVGTransform::identity(),
        }
    }
}
