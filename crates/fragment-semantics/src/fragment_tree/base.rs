use app_units::Au;
use bitflags::bitflags;
use servo_arc::Arc as ServoArc;
use style::properties::ComputedValues;

use havi_types::geom::PhysicalRect;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OpaqueNode(pub usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Tag {
    pub node: OpaqueNode,
}

#[derive(Clone, Copy, Debug)]
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

    pub fn new(node: OpaqueNode) -> Self {
        Self {
            tag: Some(Tag { node }),
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
    #[derive(Clone, Copy, Debug)]
    pub struct FragmentFlags: u16 {
        const IS_BODY_ELEMENT_OF_HTML_ELEMENT_ROOT = 1 << 0;
        const IS_BR_ELEMENT = 1 << 1;
        const IS_WIDGET = 1 << 2;
        const IS_REPLACED = 1 << 4;
        const DO_NOT_PAINT = 1 << 7;
        const IS_ROOT_ELEMENT = 1 << 9;
    }
}
