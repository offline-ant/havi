#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGLayoutNodeKind {
    Viewport,
    Group,
    Geometry,
    Text,
    Defs,
    Use,
    Gradient,
    Stop,
    ClipPath,
    Mask,
    ForeignObject,
    Image,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SVGDOMNodeSummary {
    pub kind: SVGLayoutNodeKind,
    pub establishes_viewport: bool,
    pub participates_in_paint: bool,
}
