/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::sync::Arc;

use app_units::Au;
use atomic_refcell::{AtomicRef, AtomicRefMut};
use base::id::PipelineId;
use base::print_tree::PrintTree;
use fonts::{FontMetrics, FontRef, GlyphStore};
use havi_types::fragment_tree::{
    SVGBounds, SVGContainerKind, SVGEffectState, SVGFragmentIdentity, SVGLeafKind,
    SVGOverflowClip, SVGRect, SVGResourceId, SVGTransform,
};
use malloc_size_of_derive::MallocSizeOf;
use style::Zero;
use webrender_api::{FontInstanceKey, ImageKey};

use super::{
    BaseFragment, BoxFragment, ContainingBlockManager, OutOfFlowPlacementFragment,
    PositioningFragment, Tag,
};
use crate::SharedStyle;
use crate::cell::ArcRefCell;
use crate::flow::inline::line::TextRunOffsets;
use crate::geom::{LogicalSides, PhysicalRect};
use crate::style_ext::ComputedValuesExt;

#[derive(Clone, MallocSizeOf)]
pub enum Fragment {
    Box(ArcRefCell<BoxFragment>),
    /// Floating content. A floated fragment is very similar to a normal
    /// [BoxFragment] but it isn't positioned using normal in block flow
    /// positioning rules (margin collapse, etc). Instead, they are laid
    /// out by the [crate::flow::float::SequentialLayoutState] of their
    /// float containing block formatting context.
    Float(ArcRefCell<BoxFragment>),
    Positioning(ArcRefCell<PositioningFragment>),
    /// Placeholder for a hoisted absolute/fixed fragment in original tree order.
    ///
    /// The real fragment is owned once by the containing block subtree. This node
    /// only carries immutable placement metadata used to reconstruct paint/query order.
    AbsoluteOrFixedPositioned(OutOfFlowPlacementFragment),
    Text(ArcRefCell<TextFragment>),
    Image(ArcRefCell<ImageFragment>),
    IFrame(ArcRefCell<IFrameFragment>),
    SVGViewport(ArcRefCell<SVGViewportFragment>),
    SVGContainer(ArcRefCell<SVGContainerFragment>),
    SVGLeaf(ArcRefCell<SVGLeafFragment>),
}

#[derive(Clone, MallocSizeOf)]
pub struct CollapsedBlockMargins {
    pub collapsed_through: bool,
    pub start: CollapsedMargin,
    pub end: CollapsedMargin,
}

#[derive(Clone, Copy, Debug, MallocSizeOf)]
pub struct CollapsedMargin {
    max_positive: Au,
    min_negative: Au,
}

#[derive(MallocSizeOf)]
pub struct TextFragment {
    pub base: BaseFragment,
    pub text: String,
    pub selected_style: SharedStyle,
    #[conditional_malloc_size_of]
    pub font_metrics: Arc<FontMetrics>,
    pub font_key: FontInstanceKey,
    pub font: FontRef,
    #[conditional_malloc_size_of]
    pub glyphs: Vec<Arc<GlyphStore>>,
    /// Extra space to add for each justification opportunity.
    pub justification_adjustment: Au,
    /// When necessary, this field store the [`TextRunOffsets`] for a particular
    /// [`TextRunLineItem`]. This is currently only used inside of text inputs.
    pub(crate) offsets: Option<Box<TextRunOffsets>>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, MallocSizeOf, PartialEq)]
pub enum ImageFragmentSourceKind {
    Raster,
    Canvas,
    Video,
}

#[derive(MallocSizeOf)]
pub struct ImageFragment {
    pub base: BaseFragment,
    pub clip: PhysicalRect<Au>,
    pub image_key: Option<ImageKey>,
    pub source_kind: ImageFragmentSourceKind,
    pub image_revision: u64,
    pub source_width: u32,
    pub source_height: u32,
    #[conditional_malloc_size_of]
    pub source_data: Option<std::sync::Arc<Vec<u8>>>,
    pub showing_broken_image_icon: bool,
    /// Raster image pixel data, stored for rendering without WebRender.
    #[conditional_malloc_size_of]
    pub raster_image: Option<std::sync::Arc<pixels::RasterImage>>,
}

#[derive(MallocSizeOf)]
pub struct IFrameFragment {
    pub base: BaseFragment,
    pub pipeline_id: PipelineId,
}

#[derive(Clone, MallocSizeOf)]
pub struct SVGResourceOwnedSubtree {
    #[ignore_malloc_size_of = "Resource ids are copied into published resources"]
    pub owner_resource_id: SVGResourceId,
    pub fragment_roots: Vec<Fragment>,
    #[ignore_malloc_size_of = "Resource ids are copied into published resources"]
    pub resource_dependencies: Vec<SVGResourceId>,
}

#[derive(MallocSizeOf)]
pub struct SVGViewportFragment {
    pub base: BaseFragment,
    #[ignore_malloc_size_of = "SVG identity is copied into published fragments"]
    pub identity: SVGFragmentIdentity,
    pub children: Vec<Fragment>,
    #[ignore_malloc_size_of = "SVG viewport metadata is small and copied into published fragments"]
    pub viewport_rect: SVGRect,
    #[ignore_malloc_size_of = "SVG viewport metadata is small and copied into published fragments"]
    pub view_box_rect: Option<SVGRect>,
    #[ignore_malloc_size_of = "SVG transform storage is accounted for by fragment ownership"]
    pub local_to_parent_transform: SVGTransform,
    #[ignore_malloc_size_of = "SVG overflow clips are copied into published fragments"]
    pub overflow_clip: Option<SVGOverflowClip>,
    #[ignore_malloc_size_of = "SVG resource graphs are published separately into the fragment arena"]
    pub resource_graph: Option<crate::svg::resources::SVGResourceGraph>,
    pub resource_owned_subtrees: Vec<SVGResourceOwnedSubtree>,
}

#[derive(MallocSizeOf)]
pub struct SVGContainerFragment {
    pub base: BaseFragment,
    #[ignore_malloc_size_of = "SVG identity is copied into published fragments"]
    pub identity: SVGFragmentIdentity,
    #[ignore_malloc_size_of = "SVG container metadata is copied into published fragments"]
    pub kind: SVGContainerKind,
    pub children: Vec<Fragment>,
    #[ignore_malloc_size_of = "SVG transform storage is accounted for by fragment ownership"]
    pub local_transform: SVGTransform,
    #[ignore_malloc_size_of = "SVG effect references are copied into published fragments"]
    pub effects: SVGEffectState,
}

#[derive(MallocSizeOf)]
pub struct SVGLeafFragment {
    pub base: BaseFragment,
    #[ignore_malloc_size_of = "SVG identity is copied into published fragments"]
    pub identity: SVGFragmentIdentity,
    #[ignore_malloc_size_of = "SVG leaf payloads are copied into published fragments"]
    pub kind: SVGLeafKind,
    #[ignore_malloc_size_of = "SVG bounds are copied into published fragments"]
    pub bounds: SVGBounds,
    #[ignore_malloc_size_of = "SVG transform storage is accounted for by fragment ownership"]
    pub local_transform: SVGTransform,
    #[ignore_malloc_size_of = "SVG paints are copied into published fragments"]
    pub paint: havi_types::fragment_tree::SVGPaintStyle,
    #[ignore_malloc_size_of = "SVG effect references are copied into published fragments"]
    pub effects: SVGEffectState,
}

impl Fragment {
    pub fn base<'a>(&'a self) -> Option<AtomicRef<'a, BaseFragment>> {
        Some(match self {
            Fragment::Box(fragment) => AtomicRef::map(fragment.borrow(), |fragment| &fragment.base),
            Fragment::Text(fragment) => {
                AtomicRef::map(fragment.borrow(), |fragment| &fragment.base)
            },
            Fragment::AbsoluteOrFixedPositioned(_) => return None,
            Fragment::Positioning(fragment) => {
                AtomicRef::map(fragment.borrow(), |fragment| &fragment.base)
            },
            Fragment::Image(fragment) => {
                AtomicRef::map(fragment.borrow(), |fragment| &fragment.base)
            },
            Fragment::IFrame(fragment) => {
                AtomicRef::map(fragment.borrow(), |fragment| &fragment.base)
            },
            Fragment::SVGViewport(fragment) => {
                AtomicRef::map(fragment.borrow(), |fragment| &fragment.base)
            },
            Fragment::SVGContainer(fragment) => {
                AtomicRef::map(fragment.borrow(), |fragment| &fragment.base)
            },
            Fragment::SVGLeaf(fragment) => {
                AtomicRef::map(fragment.borrow(), |fragment| &fragment.base)
            },
            Fragment::Float(fragment) => {
                AtomicRef::map(fragment.borrow(), |fragment| &fragment.base)
            },
        })
    }

    pub fn base_mut<'a>(&'a self) -> Option<AtomicRefMut<'a, BaseFragment>> {
        Some(match self {
            Fragment::Box(fragment) => {
                AtomicRefMut::map(fragment.borrow_mut(), |fragment| &mut fragment.base)
            },
            Fragment::Text(fragment) => {
                AtomicRefMut::map(fragment.borrow_mut(), |fragment| &mut fragment.base)
            },
            Fragment::AbsoluteOrFixedPositioned(_) => return None,
            Fragment::Positioning(fragment) => {
                AtomicRefMut::map(fragment.borrow_mut(), |fragment| &mut fragment.base)
            },
            Fragment::Image(fragment) => {
                AtomicRefMut::map(fragment.borrow_mut(), |fragment| &mut fragment.base)
            },
            Fragment::IFrame(fragment) => {
                AtomicRefMut::map(fragment.borrow_mut(), |fragment| &mut fragment.base)
            },
            Fragment::SVGViewport(fragment) => {
                AtomicRefMut::map(fragment.borrow_mut(), |fragment| &mut fragment.base)
            },
            Fragment::SVGContainer(fragment) => {
                AtomicRefMut::map(fragment.borrow_mut(), |fragment| &mut fragment.base)
            },
            Fragment::SVGLeaf(fragment) => {
                AtomicRefMut::map(fragment.borrow_mut(), |fragment| &mut fragment.base)
            },
            Fragment::Float(fragment) => {
                AtomicRefMut::map(fragment.borrow_mut(), |fragment| &mut fragment.base)
            },
        })
    }

    pub(crate) fn set_containing_block(&self, containing_block: &PhysicalRect<Au>) {
        match self {
            Fragment::Box(box_fragment) => box_fragment
                .borrow_mut()
                .set_containing_block(containing_block),
            Fragment::Float(float_fragment) => float_fragment
                .borrow_mut()
                .set_containing_block(containing_block),
            Fragment::Positioning(positioning_fragment) => positioning_fragment
                .borrow_mut()
                .set_containing_block(containing_block),
            Fragment::AbsoluteOrFixedPositioned(_) => {},
            Fragment::Text(_) => {},
            Fragment::Image(_) => {},
            Fragment::IFrame(_) => {},
            Fragment::SVGViewport(_) => {},
            Fragment::SVGContainer(_) => {},
            Fragment::SVGLeaf(_) => {},
        }
    }

    pub fn tag(&self) -> Option<Tag> {
        self.base().and_then(|base| base.tag)
    }

    pub fn content_rect(&self) -> PhysicalRect<Au> {
        match self {
            Fragment::AbsoluteOrFixedPositioned(_) => PhysicalRect::zero(),
            _ => self.base().map(|base| base.rect).unwrap_or_default(),
        }
    }

    pub fn print(&self, tree: &mut PrintTree) {
        match self {
            Fragment::Box(fragment) => fragment.borrow().print(tree),
            Fragment::Float(fragment) => {
                tree.new_level("Float".to_string());
                fragment.borrow().print(tree);
                tree.end_level();
            },
            Fragment::AbsoluteOrFixedPositioned(_) => {
                tree.add_item("AbsoluteOrFixedPositioned".to_string());
            },
            Fragment::Positioning(fragment) => fragment.borrow().print(tree),
            Fragment::Text(fragment) => fragment.borrow().print(tree),
            Fragment::Image(fragment) => fragment.borrow().print(tree),
            Fragment::IFrame(fragment) => fragment.borrow().print(tree),
            Fragment::SVGViewport(fragment) => fragment.borrow().print(tree),
            Fragment::SVGContainer(fragment) => fragment.borrow().print(tree),
            Fragment::SVGLeaf(fragment) => fragment.borrow().print(tree),
        }
    }

    pub(crate) fn scrollable_overflow_for_parent(&self) -> PhysicalRect<Au> {
        match self {
            Fragment::Box(fragment) | Fragment::Float(fragment) => {
                return fragment.borrow().scrollable_overflow_for_parent();
            },
            Fragment::Positioning(fragment) => fragment.borrow().scrollable_overflow_for_parent(),
            Fragment::AbsoluteOrFixedPositioned(_) |
            Fragment::Text(..) |
            Fragment::Image(..) |
            Fragment::IFrame(..) |
            Fragment::SVGLeaf(..) => self.base().map(|base| base.rect).unwrap_or_default(),
            Fragment::SVGViewport(fragment) => fragment.borrow().base.rect,
            Fragment::SVGContainer(fragment) => fragment.borrow().base.rect,
        }
    }

    pub(crate) fn calculate_scrollable_overflow_for_parent(&self) -> PhysicalRect<Au> {
        self.calculate_scrollable_overflow();
        self.scrollable_overflow_for_parent()
    }

    pub(crate) fn calculate_scrollable_overflow(&self) {
        match self {
            Fragment::Box(fragment) | Fragment::Float(fragment) => {
                fragment.borrow_mut().calculate_scrollable_overflow()
            },
            Fragment::Positioning(fragment) => {
                fragment.borrow_mut().calculate_scrollable_overflow()
            },
            _ => {},
        }
    }

    pub(crate) fn find<T>(
        &self,
        manager: &ContainingBlockManager<PhysicalRect<Au>>,
        level: usize,
        process_func: &mut impl FnMut(&Fragment, usize, &PhysicalRect<Au>) -> Option<T>,
    ) -> Option<T> {
        let containing_block = manager.get_containing_block_for_fragment(self);
        if let Some(result) = process_func(self, level, containing_block) {
            return Some(result);
        }

        match self {
            Fragment::Box(fragment) | Fragment::Float(fragment) => {
                let fragment = fragment.borrow();
                let style = fragment.style();
                let content_rect = fragment
                    .content_rect()
                    .translate(containing_block.origin.to_vector());
                let padding_rect = fragment
                    .padding_rect()
                    .translate(containing_block.origin.to_vector());
                let new_manager = if style
                    .establishes_containing_block_for_all_descendants(fragment.base.flags)
                {
                    manager.new_for_absolute_and_fixed_descendants(&content_rect, &padding_rect)
                } else if style
                    .establishes_containing_block_for_absolute_descendants(fragment.base.flags)
                {
                    manager.new_for_absolute_descendants(&content_rect, &padding_rect)
                } else {
                    manager.new_for_non_absolute_descendants(&content_rect)
                };

                fragment
                    .children
                    .iter()
                    .find_map(|child| child.find(&new_manager, level + 1, process_func))
            },
            Fragment::Positioning(fragment) => {
                let fragment = fragment.borrow();
                let content_rect = fragment
                    .base
                    .rect
                    .translate(containing_block.origin.to_vector());
                let new_manager = manager.new_for_non_absolute_descendants(&content_rect);
                fragment
                    .children
                    .iter()
                    .find_map(|child| child.find(&new_manager, level + 1, process_func))
            },
            Fragment::SVGViewport(fragment) => {
                let fragment = fragment.borrow();
                let content_rect = fragment.base.rect.translate(containing_block.origin.to_vector());
                let new_manager = manager.new_for_non_absolute_descendants(&content_rect);
                fragment
                    .children
                    .iter()
                    .find_map(|child| child.find(&new_manager, level + 1, process_func))
            },
            Fragment::SVGContainer(fragment) => {
                let fragment = fragment.borrow();
                let content_rect = fragment.base.rect.translate(containing_block.origin.to_vector());
                let new_manager = manager.new_for_non_absolute_descendants(&content_rect);
                fragment
                    .children
                    .iter()
                    .find_map(|child| child.find(&new_manager, level + 1, process_func))
            },
            _ => None,
        }
    }

    pub(crate) fn retrieve_box_fragment(&self) -> Option<&ArcRefCell<BoxFragment>> {
        match self {
            Fragment::Box(box_fragment) | Fragment::Float(box_fragment) => Some(box_fragment),
            _ => None,
        }
    }
}

impl TextFragment {
    pub fn print(&self, tree: &mut PrintTree) {
        tree.add_item(format!(
            "Text num_glyphs={} box={:?}",
            self.glyphs
                .iter()
                .map(|glyph_store| glyph_store.len())
                .sum::<usize>(),
            self.base.rect
        ));
    }

}

impl ImageFragment {
    pub fn print(&self, tree: &mut PrintTree) {
        tree.add_item(format!(
            "Image\
                \nrect={:?}",
            self.base.rect
        ));
    }
}

impl IFrameFragment {
    pub fn print(&self, tree: &mut PrintTree) {
        tree.add_item(format!(
            "IFrame\
                \npipeline={:?} rect={:?}",
            self.pipeline_id, self.base.rect
        ));
    }
}

impl SVGViewportFragment {
    pub fn print(&self, tree: &mut PrintTree) {
        tree.new_level(format!("SVGViewport rect={:?}", self.base.rect));
        for child in &self.children {
            child.print(tree);
        }
        tree.end_level();
    }
}

impl SVGContainerFragment {
    pub fn print(&self, tree: &mut PrintTree) {
        tree.new_level(format!("SVGContainer rect={:?}", self.base.rect));
        for child in &self.children {
            child.print(tree);
        }
        tree.end_level();
    }
}

impl SVGLeafFragment {
    pub fn print(&self, tree: &mut PrintTree) {
        tree.add_item(format!("SVGLeaf rect={:?}", self.base.rect));
    }
}

impl CollapsedBlockMargins {
    pub fn from_margin(margin: &LogicalSides<Au>) -> Self {
        Self {
            collapsed_through: false,
            start: CollapsedMargin::new(margin.block_start),
            end: CollapsedMargin::new(margin.block_end),
        }
    }

    pub fn zero() -> Self {
        Self {
            collapsed_through: false,
            start: CollapsedMargin::zero(),
            end: CollapsedMargin::zero(),
        }
    }
}

impl CollapsedMargin {
    pub fn zero() -> Self {
        Self {
            max_positive: Au::zero(),
            min_negative: Au::zero(),
        }
    }

    pub fn new(margin: Au) -> Self {
        Self {
            max_positive: margin.max(Au::zero()),
            min_negative: margin.min(Au::zero()),
        }
    }

    pub fn adjoin(&self, other: &Self) -> Self {
        Self {
            max_positive: self.max_positive.max(other.max_positive),
            min_negative: self.min_negative.min(other.min_negative),
        }
    }

    pub fn adjoin_assign(&mut self, other: &Self) {
        *self = self.adjoin(other);
    }

    pub fn solve(&self) -> Au {
        self.max_positive + self.min_negative
    }
}
