use app_units::Au;
use havi_types::fragment_tree::{SVGRect, SVGTransform};
use layout_api::SVGNodeKind;
use script::layout_dom::ServoThreadSafeLayoutNode;
use servo_arc::Arc as ServoArc;
use style::properties::ComputedValues;

use super::path::resolve_length;
use super::transform::{parse_svg_transform, translate_svg_transform};
use super::tree::SVGResolvedNode;
use crate::context::LayoutContext;
use crate::dom_traversal::{NodeAndStyleInfo, NonReplacedContents};
use crate::flow::BlockFormattingContext;
use crate::fragment_tree::Fragment;
use crate::geom::{LogicalVec2, PhysicalSides};
use crate::positioned::PositioningContext;
use crate::{
    ContainingBlock, ContainingBlockSize, DefiniteContainingBlock, PropagatedBoxTreeData,
};
use crate::sizing::SizeConstraint;

#[derive(Clone, Debug, Default)]
pub struct SVGForeignObjectLayoutResult {
    pub viewport_rect: Option<SVGRect>,
    pub local_transform: SVGTransform,
    pub local_to_html_containing_block: SVGTransform,
}

pub fn layout_foreign_object(node: &SVGResolvedNode) -> SVGForeignObjectLayoutResult {
    let viewport_rect = match &node.svg_data().node_kind {
        SVGNodeKind::ForeignObject(data) => Some(SVGRect::new(
            euclid::point2(
                resolve_length(data.x).unwrap_or(0.0),
                resolve_length(data.y).unwrap_or(0.0),
            ),
            euclid::size2(
                resolve_length(data.width).unwrap_or(0.0),
                resolve_length(data.height).unwrap_or(0.0),
            ),
        )),
        _ => None,
    };
    let local_transform = parse_svg_transform(&node.svg_data().common.transform);
    let local_to_html_containing_block = viewport_rect.map_or(SVGTransform::identity(), |rect| {
        translate_svg_transform(-rect.origin.x, -rect.origin.y)
    });
    SVGForeignObjectLayoutResult {
        viewport_rect,
        local_transform,
        local_to_html_containing_block,
    }
}

pub(crate) fn layout_foreign_object_children(
    node: ServoThreadSafeLayoutNode<'_>,
    computed_style: ServoArc<ComputedValues>,
    layout_context: &LayoutContext,
    positioning_context: &mut PositioningContext,
    viewport_rect: SVGRect,
) -> Vec<Fragment> {
    let info = NodeAndStyleInfo::new(node, computed_style.clone());
    let formatting_context = BlockFormattingContext::construct(
        layout_context,
        &info,
        NonReplacedContents::OfElement,
        PropagatedBoxTreeData::default(),
        false,
    );

    let definite_containing_block = DefiniteContainingBlock {
        size: LogicalVec2 {
            inline: Au::from_f32_px(viewport_rect.size.width.max(0.0)),
            block: Au::from_f32_px(viewport_rect.size.height.max(0.0)),
        },
        style: &computed_style,
    };
    let containing_block = ContainingBlock {
        size: ContainingBlockSize {
            inline: definite_containing_block.size.inline,
            block: SizeConstraint::Definite(definite_containing_block.size.block),
        },
        style: definite_containing_block.style,
    };

    let mut foreign_object_positioning_context = PositioningContext::default();
    let mut layout = formatting_context.layout(
        layout_context,
        &mut foreign_object_positioning_context,
        &containing_block,
    );
    foreign_object_positioning_context.layout_collected_children_for_non_box_containing_block(
        layout_context,
        &mut layout.fragments,
        &definite_containing_block,
        PhysicalSides::zero(),
        positioning_context,
    );
    layout.fragments
}
