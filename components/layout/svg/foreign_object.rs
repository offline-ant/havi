use havi_types::fragment_tree::{SVGRect, SVGTransform};

use super::dom::{SVGDOMNode, SVGNodeKindOwned};
use super::path::parse_svg_length;
use super::transform::{parse_svg_transform, translate_svg_transform};

#[derive(Clone, Debug, Default)]
pub struct SVGForeignObjectLayoutResult {
    pub viewport_rect: Option<SVGRect>,
    pub local_transform: SVGTransform,
    pub local_to_html_containing_block: SVGTransform,
}

pub fn layout_foreign_object(node: &SVGDOMNode) -> SVGForeignObjectLayoutResult {
    let viewport_rect = match &node.node_kind {
        SVGNodeKindOwned::ForeignObject(data) => Some(SVGRect::new(
            euclid::point2(
                parse_svg_length(data.x.as_deref()).unwrap_or(0.0),
                parse_svg_length(data.y.as_deref()).unwrap_or(0.0),
            ),
            euclid::size2(
                parse_svg_length(data.width.as_deref()).unwrap_or(0.0),
                parse_svg_length(data.height.as_deref()).unwrap_or(0.0),
            ),
        )),
        _ => None,
    };
    let local_transform = parse_svg_transform(node.common.transform.as_deref());
    let local_to_html_containing_block = viewport_rect.map_or(SVGTransform::identity(), |rect| {
        translate_svg_transform(-rect.origin.x, -rect.origin.y)
    });
    SVGForeignObjectLayoutResult {
        viewport_rect,
        local_transform,
        local_to_html_containing_block,
    }
}
