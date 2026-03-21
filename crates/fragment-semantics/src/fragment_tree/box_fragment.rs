use app_units::Au;
use servo_arc::Arc as ServoArc;
use style::properties::ComputedValues;

use super::{BaseFragment, BaseFragmentInfo, CollapsedBlockMargins, Fragment, FragmentFlags};
use havi_types::geom::{AuOrAuto, PhysicalRect, PhysicalSides};

#[derive(Clone, Copy, Debug, Default)]
pub struct Baselines {
    pub first: Option<Au>,
    pub last: Option<Au>,
}

#[derive(Clone, Debug)]
pub struct BackgroundImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct BoxFragment {
    pub base: BaseFragment,
    pub children: Vec<Fragment>,
    pub cumulative_containing_block_rect: PhysicalRect<Au>,
    pub scrollable_overflow: Option<PhysicalRect<Au>>,
    pub resolved_sticky_insets: Option<PhysicalSides<AuOrAuto>>,
    pub padding: PhysicalSides<Au>,
    pub border: PhysicalSides<Au>,
    pub margin: PhysicalSides<Au>,
    pub baselines: Baselines,
    pub block_level_info: Option<Box<BlockLevelLayoutInfo>>,
    pub background_images: Vec<BackgroundImage>,
}

#[derive(Clone, Debug)]
pub struct BlockLevelLayoutInfo {
    pub clearance: Option<Au>,
    pub block_margins_collapsed_with_children: CollapsedBlockMargins,
}

impl BoxFragment {
    pub fn new(
        info: BaseFragmentInfo,
        style: ServoArc<ComputedValues>,
        children: Vec<Fragment>,
        content_rect: PhysicalRect<Au>,
        padding: PhysicalSides<Au>,
        border: PhysicalSides<Au>,
        margin: PhysicalSides<Au>,
    ) -> Self {
        Self {
            base: BaseFragment::new(info, style, content_rect),
            children,
            cumulative_containing_block_rect: PhysicalRect::zero(),
            scrollable_overflow: None,
            resolved_sticky_insets: None,
            padding,
            border,
            margin,
            baselines: Baselines::default(),
            block_level_info: None,
            background_images: Vec::new(),
        }
    }

    pub fn with_baselines(mut self, baselines: Baselines) -> Self {
        self.baselines = baselines;
        self
    }

    pub fn with_block_level_info(
        mut self,
        collapsed_margins: CollapsedBlockMargins,
        clearance: Option<Au>,
    ) -> Self {
        self.block_level_info = Some(Box::new(BlockLevelLayoutInfo {
            clearance,
            block_margins_collapsed_with_children: collapsed_margins,
        }));
        self
    }

    pub fn style(&self) -> &ServoArc<ComputedValues> {
        &self.base.style
    }

    pub fn content_rect(&self) -> PhysicalRect<Au> {
        self.base.rect
    }

    pub fn padding_rect(&self) -> PhysicalRect<Au> {
        self.content_rect().outer_rect(self.padding)
    }

    pub fn border_rect(&self) -> PhysicalRect<Au> {
        self.padding_rect().outer_rect(self.border)
    }

    pub fn margin_rect(&self) -> PhysicalRect<Au> {
        self.border_rect().outer_rect(self.margin)
    }

    pub fn padding_border_margin(&self) -> PhysicalSides<Au> {
        self.padding + self.border + self.margin
    }

    pub fn is_root_element(&self) -> bool {
        self.base.flags.intersects(FragmentFlags::IS_ROOT_ELEMENT)
    }
}
