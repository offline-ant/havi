// Fragment enum — the nodes of the fragment tree, adapted from Servo's fragment.rs.

use std::ops::Range;
use std::sync::Arc;

use app_units::Au;
use servo_arc::Arc as ServoArc;
use style::properties::ComputedValues;

use super::{BaseFragment, BaseFragmentInfo, BoxFragment};
use crate::geom::PhysicalRect;

/// A single fragment in the fragment tree.
#[derive(Clone, Debug)]
pub enum Fragment {
    /// A laid-out element box with children.
    Box(BoxFragment),
    /// A floating box (same structure, different positioning rules).
    Float(BoxFragment),
    /// A text run.
    Text(TextFragment),
    /// An image (replaced element).
    Image(ImageFragment),
    /// A positioning wrapper (anonymous, carries children with relative offsets).
    Positioning(PositioningFragment),
    /// An iframe (nested browsing context). Carries a reference to the child
    /// document's fragment tree, rendered as a nested draw call.
    IFrame(IFrameFragment),
}

impl Fragment {
    pub fn base(&self) -> &BaseFragment {
        match self {
            Fragment::Box(f) | Fragment::Float(f) => &f.base,
            Fragment::Text(f) => &f.base,
            Fragment::Image(f) => &f.base,
            Fragment::Positioning(f) => &f.base,
            Fragment::IFrame(f) => &f.base,
        }
    }

    pub fn base_mut(&mut self) -> &mut BaseFragment {
        match self {
            Fragment::Box(f) | Fragment::Float(f) => &mut f.base,
            Fragment::Text(f) => &mut f.base,
            Fragment::Image(f) => &mut f.base,
            Fragment::Positioning(f) => &mut f.base,
            Fragment::IFrame(f) => &mut f.base,
        }
    }

    pub fn tag(&self) -> Option<super::Tag> {
        self.base().tag
    }

    pub fn opaque_node(&self) -> Option<super::OpaqueNode> {
        self.base().tag.map(|tag| tag.node)
    }

    pub fn content_rect(&self) -> PhysicalRect<Au> {
        self.base().rect
    }

    pub fn children(&self) -> Option<&[Fragment]> {
        match self {
            Fragment::Box(f) | Fragment::Float(f) => Some(&f.children),
            Fragment::Positioning(f) => Some(&f.children),
            Fragment::Text(_) | Fragment::Image(_) | Fragment::IFrame(_) => None,
        }
    }
}

/// A single shaped glyph with its ID and positioning offsets.
///
/// Conversion from havi's `GlyphInfo`:
/// ```ignore
/// glyph_store.glyphs().map(|g| ShapedGlyph {
///     glyph_id: g.id(),
///     advance: g.advance(),
///     x_offset: g.offset().map_or(Au(0), |o| Au::from_f32_px(o.x)),
///     y_offset: g.offset().map_or(Au(0), |o| Au::from_f32_px(o.y)),
/// })
/// ```
#[derive(Clone, Copy, Debug)]
pub struct ShapedGlyph {
    /// Font-intrinsic glyph ID. OpenType glyph IDs are u16; stored as u32
    /// to match havi's GlyphId type without truncation.
    pub glyph_id: u32,
    /// Horizontal advance for this glyph.
    pub advance: Au,
    /// Shaping x-offset (kerning, etc.).
    pub x_offset: Au,
    /// Shaping y-offset.
    pub y_offset: Au,
    /// Number of source text characters this glyph covers (for fallback lookup).
    pub char_count: u32,
}

/// A text run fragment.
#[derive(Clone, Debug)]
pub struct TextFragment {
    pub base: BaseFragment,
    /// The shaped text content (kept for Ahem fallback and debugging).
    pub text: String,
    /// Font size in px, for rendering.
    pub font_size_px: f32,
    /// Shaped glyphs from rustybuzz. Empty for Ahem font fragments.
    pub glyphs: Vec<ShapedGlyph>,
    /// Font handle for rendering (None for Ahem or when using built-in fallback).
    pub font_handle: Option<havi_fonts::FontHandle>,
    /// Pre-loaded font data bytes. When present, the render crate uses this
    /// instead of reading from disk, avoiding I/O during draw.
    pub font_data: Option<havi_fonts::FontData>,
    /// Distance from content-rect top to the baseline.
    pub baseline_ascent: Au,
    /// Distance from baseline to underline center (positive = below).
    pub underline_offset: Au,
    /// Thickness of underline/overline lines.
    pub underline_size: Au,
    /// Distance from baseline to strikeout center (positive = above).
    pub strikeout_offset: Au,
    /// Thickness of strikeout line.
    pub strikeout_size: Au,
}

/// An iframe fragment (nested browsing context).
#[derive(Clone, Debug)]
pub struct IFrameFragment {
    pub base: BaseFragment,
    /// The child document's fragment tree.
    pub child_fragments: Arc<Vec<Fragment>>,
    /// Content height of the child document (for scroll).
    pub child_content_height: f32,
}

/// An image fragment (replaced element).
///
/// Image data is stored as a shared byte buffer containing all animation frames.
/// `frame_byte_range` selects the active frame within that buffer.
/// For non-animated images, the range spans the entire buffer.
#[derive(Clone, Debug)]
pub struct ImageFragment {
    pub base: BaseFragment,
    /// Opaque image key `(namespace, index)` for matching Paint-layer updates.
    /// `None` for broken/missing images.
    pub image_key: Option<(u32, u32)>,
    /// Width of the active frame in pixels.
    pub frame_width: u32,
    /// Height of the active frame in pixels.
    pub frame_height: u32,
    /// All image bytes (may contain multiple animation frames).
    pub image_data: Arc<Vec<u8>>,
    /// Byte range of the active frame within `image_data`.
    pub frame_byte_range: Range<usize>,
}

/// An image data override from the Paint layer, used when the image store
/// has newer data than the fragment tree (e.g. canvas updates, animation
/// frames that arrived after the last layout).
#[derive(Clone, Debug)]
pub struct ImageOverride {
    /// RGBA pixel data.
    pub data: Arc<Vec<u8>>,
    /// Byte offset of the active frame within `data`.
    pub offset: usize,
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
}

/// Map of image key → override data. Passed to the render layer so it can
/// pick up image updates that arrived after the last fragment tree build.
pub type ImageOverrides = std::collections::HashMap<(u32, u32), ImageOverride>;

/// A positioning fragment (anonymous or not) that contains children
/// with coordinates relative to its own content rect.
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
