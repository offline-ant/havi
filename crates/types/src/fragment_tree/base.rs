use app_units::Au;
use bitflags::bitflags;
use servo_arc::Arc as ServoArc;
use style::dom::OpaqueNode;
use style::properties::ComputedValues;
use style::selector_parser::PseudoElement;

use crate::geom::PhysicalRect;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Tag {
    pub node: OpaqueNode,
    pub pseudo: Option<PseudoElement>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BaseFragmentInfo {
    pub tag: Option<Tag>,
    pub flags: FragmentFlags,
}

impl BaseFragmentInfo {
    pub fn anonymous() -> Self {
        Self {
            tag: None,
            flags: FragmentFlags::empty(),
        }
    }

    pub fn new(node: OpaqueNode, pseudo: Option<PseudoElement>) -> Self {
        Self {
            tag: Some(Tag { node, pseudo }),
            flags: FragmentFlags::empty(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct BaseFragment {
    pub tag: Option<Tag>,
    pub flags: FragmentFlags,
    pub style: ServoArc<ComputedValues>,
    pub rect: PhysicalRect<Au>,
}

impl BaseFragment {
    pub fn new(
        info: BaseFragmentInfo,
        style: ServoArc<ComputedValues>,
        rect: PhysicalRect<Au>,
    ) -> Self {
        Self {
            tag: info.tag,
            flags: info.flags,
            style,
            rect,
        }
    }

    pub fn is_anonymous(&self) -> bool {
        self.tag.is_none()
    }
}

bitflags! {
    #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
    pub struct FragmentFlags: u16 {
        const IS_BODY_ELEMENT_OF_HTML_ELEMENT_ROOT = 1 << 0;
        const IS_BR_ELEMENT = 1 << 1;
        const IS_WIDGET = 1 << 2;
        const IS_FLEX_OR_GRID_ITEM = 1 << 3;
        const IS_REPLACED = 1 << 4;
        const IS_TABLE_TH_OR_TD_ELEMENT = 1 << 5;
        const IS_OUTSIDE_LIST_ITEM_MARKER = 1 << 6;
        const DO_NOT_PAINT = 1 << 7;
        const SIZE_DEPENDS_ON_BLOCK_CONSTRAINTS_AND_CAN_BE_CHILD_OF_FLEX_ITEM = 1 << 8;
        const IS_ROOT_ELEMENT = 1 << 9;
        const PROPAGATED_OVERFLOW_TO_VIEWPORT = 1 << 10;
        const IS_COLLAPSED = 1 << 11;
    }
}
