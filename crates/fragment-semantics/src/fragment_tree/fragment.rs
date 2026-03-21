use std::ops::Range;
use std::sync::Arc;

use app_units::Au;
use servo_arc::Arc as ServoArc;
use style::properties::ComputedValues;

use super::{BaseFragment, BaseFragmentInfo, BoxFragment};
use havi_types::geom::PhysicalRect;

#[derive(Clone, Debug)]
pub enum Fragment {
    Box(BoxFragment),
    Float(BoxFragment),
    Text(TextFragment),
    Image(ImageFragment),
    Positioning(PositioningFragment),
    AbsoluteOrFixedPositioned { resolved: Box<Fragment> },
    IFrame(IFrameFragment),
}

impl Fragment {
    pub fn base(&self) -> &BaseFragment {
        match self {
            Fragment::Box(f) | Fragment::Float(f) => &f.base,
            Fragment::Text(f) => &f.base,
            Fragment::Image(f) => &f.base,
            Fragment::Positioning(f) => &f.base,
            Fragment::AbsoluteOrFixedPositioned { .. } => {
                panic!("AbsoluteOrFixedPositioned placeholders do not own a BaseFragment")
            }
            Fragment::IFrame(f) => &f.base,
        }
    }

    pub fn base_mut(&mut self) -> &mut BaseFragment {
        match self {
            Fragment::Box(f) | Fragment::Float(f) => &mut f.base,
            Fragment::Text(f) => &mut f.base,
            Fragment::Image(f) => &mut f.base,
            Fragment::Positioning(f) => &mut f.base,
            Fragment::AbsoluteOrFixedPositioned { .. } => {
                panic!("AbsoluteOrFixedPositioned placeholders do not own a BaseFragment")
            }
            Fragment::IFrame(f) => &mut f.base,
        }
    }

    pub fn tag(&self) -> Option<super::Tag> {
        match self {
            Fragment::AbsoluteOrFixedPositioned { .. } => None,
            _ => self.base().tag,
        }
    }

    pub fn opaque_node(&self) -> Option<super::OpaqueNode> {
        self.tag().map(|tag| tag.node)
    }

    pub fn content_rect(&self) -> PhysicalRect<Au> {
        match self {
            Fragment::AbsoluteOrFixedPositioned { .. } => PhysicalRect::zero(),
            _ => self.base().rect,
        }
    }

    pub fn children(&self) -> Option<&[Fragment]> {
        match self {
            Fragment::Box(f) | Fragment::Float(f) => Some(&f.children),
            Fragment::Positioning(f) => Some(&f.children),
            Fragment::AbsoluteOrFixedPositioned { .. } => None,
            Fragment::Text(_) | Fragment::Image(_) | Fragment::IFrame(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ShapedGlyph {
    pub glyph_id: u32,
    pub advance: Au,
    pub x_offset: Au,
    pub y_offset: Au,
    pub char_count: u32,
}

#[derive(Clone, Debug)]
pub struct TextFragment {
    pub base: BaseFragment,
    pub text: String,
    pub font_size_px: f32,
    pub glyphs: Vec<ShapedGlyph>,
    pub font_handle: Option<havi_fonts::FontHandle>,
    pub font_data: Option<havi_fonts::FontData>,
    pub baseline_ascent: Au,
    pub underline_offset: Au,
    pub underline_size: Au,
    pub strikeout_offset: Au,
    pub strikeout_size: Au,
}

#[derive(Clone, Debug)]
pub struct IFrameFragment {
    pub base: BaseFragment,
    pub child_fragments: Arc<Vec<Fragment>>,
    pub child_content_height: f32,
}

#[derive(Clone, Debug)]
pub struct ImageFragment {
    pub base: BaseFragment,
    pub image_key: Option<(u32, u32)>,
    pub frame_width: u32,
    pub frame_height: u32,
    pub image_data: Arc<Vec<u8>>,
    pub frame_byte_range: Range<usize>,
}

#[derive(Clone, Debug)]
pub struct ImageOverride {
    pub data: Arc<Vec<u8>>,
    pub offset: usize,
    pub width: u32,
    pub height: u32,
}

pub type ImageOverrides = std::collections::HashMap<(u32, u32), ImageOverride>;

#[derive(Clone, Debug)]
pub struct PositioningFragment {
    pub base: BaseFragment,
    pub children: Vec<Fragment>,
}

impl PositioningFragment {
    pub fn new_anonymous(
        style: ServoArc<ComputedValues>,
        rect: PhysicalRect<Au>,
        children: Vec<Fragment>,
    ) -> Self {
        Self {
            base: BaseFragment::new(BaseFragmentInfo::anonymous(), style, rect),
            children,
        }
    }
}
