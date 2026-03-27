use havi_types::fragment_tree::SVGGlyphRun;

#[derive(Clone, Debug, Default)]
pub struct SVGTextLayoutResult {
    pub glyph_runs: Vec<SVGGlyphRun>,
}
