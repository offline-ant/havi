use havi_types::fragment_tree::{
    SVGFillRule, SVGLineCap, SVGLineJoin, SVGPaint, SVGPathCommand, SVGPathData,
};

#[derive(Clone, Debug)]
pub struct SVGStrokeStyleSpec {
    pub paint: SVGPaint,
    pub width: f32,
    pub line_cap: SVGLineCap,
    pub line_join: SVGLineJoin,
    pub miter_limit: f32,
    pub non_scaling: bool,
}

impl Default for SVGStrokeStyleSpec {
    fn default() -> Self {
        Self {
            paint: SVGPaint::None,
            width: 1.0,
            line_cap: SVGLineCap::Butt,
            line_join: SVGLineJoin::Miter,
            miter_limit: 4.0,
            non_scaling: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SVGNormalizedPath {
    pub fill_rule: SVGFillRule,
    pub commands: Vec<SVGPathCommand>,
}

impl Default for SVGNormalizedPath {
    fn default() -> Self {
        Self {
            fill_rule: SVGFillRule::NonZero,
            commands: Vec::new(),
        }
    }
}

impl From<SVGNormalizedPath> for SVGPathData {
    fn from(path: SVGNormalizedPath) -> Self {
        Self {
            fill_rule: path.fill_rule,
            commands: path.commands,
        }
    }
}
