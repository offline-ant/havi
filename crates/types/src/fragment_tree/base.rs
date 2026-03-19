// Base fragment info and flags, adapted from Servo's base_fragment.rs.

use app_units::Au;
use bitflags::bitflags;
use servo_arc::Arc as ServoArc;
use style::properties::ComputedValues;

use crate::geom::PhysicalRect;

/// Opaque DOM node identifier, used to associate fragments with their
/// source DOM nodes without layout depending on concrete DOM types.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OpaqueNode(pub usize);

/// Tag identifying the DOM node of a fragment. If the fragment is
/// anonymous (e.g. anonymous block box), the tag is None.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Tag {
    pub node: OpaqueNode,
}

/// Information needed to construct a BaseFragment.
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

/// Common fields for all concrete fragment types.
#[derive(Clone, Debug)]
pub struct BaseFragment {
    pub tag: Option<Tag>,
    pub flags: FragmentFlags,
    pub style: ServoArc<ComputedValues>,
    /// Content rect relative to the parent fragment's content rectangle.
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
        /// `<body>` element on an HTML document.
        const IS_BODY_ELEMENT_OF_HTML_ELEMENT_ROOT = 1 << 0;
        /// `<br>` element.
        const IS_BR_ELEMENT = 1 << 1;
        /// Widget element. Widgets are atomic when inline-level.
        const IS_WIDGET = 1 << 2;
        /// Replaced element or wrapper created for one.
        const IS_REPLACED = 1 << 4;
        /// Skip painting backgrounds/borders/shadow (table wrappers, hidden empty cells).
        const DO_NOT_PAINT = 1 << 7;
        /// Root element.
        const IS_ROOT_ELEMENT = 1 << 9;
    }
}
