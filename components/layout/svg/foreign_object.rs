use havi_types::fragment_tree::{SVGRect, SVGTransform};
use layout_api::SVGNodeKind;

use super::dom::SVGResolvedNode;
use super::path::parse_svg_length;
use super::transform::{parse_svg_transform, translate_svg_transform};

#[derive(Clone, Debug, Default)]
pub struct SVGForeignObjectLayoutResult {
    pub viewport_rect: Option<SVGRect>,
    pub local_transform: SVGTransform,
    pub local_to_html_containing_block: SVGTransform,
}

pub fn layout_foreign_object(node: &SVGResolvedNode<'_>) -> SVGForeignObjectLayoutResult {
    let viewport_rect = match &node.svg_data.node_kind {
        SVGNodeKind::ForeignObject(data) => Some(SVGRect::new(
            euclid::point2(
                parse_svg_length(data.x).unwrap_or(0.0),
                parse_svg_length(data.y).unwrap_or(0.0),
            ),
            euclid::size2(
                parse_svg_length(data.width).unwrap_or(0.0),
                parse_svg_length(data.height).unwrap_or(0.0),
            ),
        )),
        _ => None,
    };
    let local_transform = parse_svg_transform(node.svg_data.common.transform);
    let local_to_html_containing_block = viewport_rect.map_or(SVGTransform::identity(), |rect| {
        translate_svg_transform(-rect.origin.x, -rect.origin.y)
    });
    SVGForeignObjectLayoutResult {
        viewport_rect,
        local_transform,
        local_to_html_containing_block,
    }
}
