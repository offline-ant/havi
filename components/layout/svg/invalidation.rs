#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGInvalidationKind {
    Paint,
    Geometry,
    Transform,
    ResourceDependency,
    Bounds,
    HitTest,
}

#[derive(Clone, Debug, Default)]
pub struct SVGInvalidationSet {
    pub kinds: Vec<SVGInvalidationKind>,
}
