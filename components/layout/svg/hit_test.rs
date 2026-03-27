#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGHitTargetKind {
    Fill,
    Stroke,
    BoundingBox,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SVGHitTestResult {
    pub hit: bool,
    pub target: SVGHitTargetKind,
}
