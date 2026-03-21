//! Servo-shaped semantic stacking-context construction over the shared semantic fragment model.

use havi_fragment_semantics::fragment_tree::{BoxFragment, FragmentFlags};
use havi_fragment_semantics::Fragment;
use havi_types::PhysicalRect;
use makepad_widgets::{dvec2, Rect};
use style::computed_values::mix_blend_mode::T as ComputedMixBlendMode;
use style::computed_values::overflow_x::T as ComputedOverflow;
use style::computed_values::position::T as ComputedPosition;
use style::computed_values::transform_style::T as ComputedTransformStyle;
use style::properties::ComputedValues;
use style::values::computed::basic_shape::ClipPath;
use style::values::computed::ClipRectOrAuto;
use style::values::specified::box_::DisplayOutside;
use style::Zero;

use crate::scene::{
    ReferenceFrameData, SceneClipId, SceneClipKind, ScrollNodeData, SpatialNodeId,
    StickyNodeData, StickyOffsetBounds,
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct ContainingBlock {
    pub paint_container_id: usize,
    pub spatial_node_id: SpatialNodeId,
    pub clip_id: SceneClipId,
    pub rect: PhysicalRect<app_units::Au>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ContainingBlockInfo {
    pub for_non_absolute_descendants: ContainingBlock,
    pub for_absolute_descendants: ContainingBlock,
    pub for_absolute_and_fixed_descendants: ContainingBlock,
}

impl ContainingBlockInfo {
    pub(crate) fn containing_block_for_fragment(&self, fragment: &Fragment) -> ContainingBlock {
        match fragment {
            Fragment::Box(bf) | Fragment::Float(bf) => match bf.base.style.get_box().position {
                ComputedPosition::Fixed => self.for_absolute_and_fixed_descendants,
                ComputedPosition::Absolute => self.for_absolute_descendants,
                _ => self.for_non_absolute_descendants,
            },
            _ => self.for_non_absolute_descendants,
        }
    }

    pub(crate) fn new_for_non_absolute_descendants(&self, containing_block: ContainingBlock) -> Self {
        Self {
            for_non_absolute_descendants: containing_block,
            ..*self
        }
    }

    pub(crate) fn new_for_absolute_descendants(&self, containing_block: ContainingBlock) -> Self {
        Self {
            for_non_absolute_descendants: containing_block,
            for_absolute_descendants: containing_block,
            ..*self
        }
    }

    pub(crate) fn new_for_absolute_and_fixed_descendants(&self, containing_block: ContainingBlock) -> Self {
        Self {
            for_non_absolute_descendants: containing_block,
            for_absolute_descendants: containing_block,
            for_absolute_and_fixed_descendants: containing_block,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum StackingContextSection {
    OwnBackgroundsAndBorders,
    DescendantBackgroundsAndBorders,
    Foreground,
    Outline,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StackingContextType {
    RealStackingContext,
    PositionedStackingContainer,
    FloatStackingContainer,
    AtomicInlineStackingContainer,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SpatialAttachment {
    pub paint_container_id: usize,
    pub spatial_node_id: SpatialNodeId,
    pub clip_id: SceneClipId,
    pub scene_origin: makepad_widgets::DVec2,
}

pub(crate) enum LayoutStackingContextContent<'a> {
    Fragment {
        section: StackingContextSection,
        fragment: &'a Fragment,
        attachment: SpatialAttachment,
        containing_block: PhysicalRect<app_units::Au>,
    },
    AtomicInlineStackingContainer { index: usize },
}

impl LayoutStackingContextContent<'_> {
    fn section(&self) -> StackingContextSection {
        match self {
            Self::Fragment { section, .. } => *section,
            Self::AtomicInlineStackingContainer { .. } => StackingContextSection::Foreground,
        }
    }

    pub(crate) fn has_outline(&self) -> bool {
        match self {
            Self::Fragment { fragment, .. } => match fragment {
                Fragment::Box(bf) | Fragment::Float(bf) => {
                    let outline = bf.base.style.get_outline();
                    !outline.outline_style.none_or_hidden() && !outline.outline_width.0.is_zero()
                }
                _ => false,
            },
            Self::AtomicInlineStackingContainer { .. } => false,
        }
    }
}

pub(crate) struct LayoutStackingContext<'a> {
    pub initializing_fragment: Option<&'a BoxFragment>,
    pub context_type: StackingContextType,
    pub contents: Vec<LayoutStackingContextContent<'a>>,
    pub real_stacking_contexts_and_positioned_stacking_containers: Vec<LayoutStackingContext<'a>>,
    pub float_stacking_containers: Vec<LayoutStackingContext<'a>>,
    pub atomic_inline_stacking_containers: Vec<LayoutStackingContext<'a>>,
}

impl<'a> LayoutStackingContext<'a> {
    fn new_root(_attachment: SpatialAttachment) -> Self {
        Self {
            initializing_fragment: None,
            context_type: StackingContextType::RealStackingContext,
            contents: Vec::new(),
            real_stacking_contexts_and_positioned_stacking_containers: Vec::new(),
            float_stacking_containers: Vec::new(),
            atomic_inline_stacking_containers: Vec::new(),
        }
    }

    fn new_child(
        bf: &'a BoxFragment,
        context_type: StackingContextType,
        _attachment: SpatialAttachment,
    ) -> Self {
        Self {
            initializing_fragment: Some(bf),
            context_type,
            contents: Vec::new(),
            real_stacking_contexts_and_positioned_stacking_containers: Vec::new(),
            float_stacking_containers: Vec::new(),
            atomic_inline_stacking_containers: Vec::new(),
        }
    }

    pub fn z_index(&self) -> i32 {
        self.initializing_fragment
            .map(|f| effective_z_index(&f.base.style, f.base.flags))
            .unwrap_or(0)
    }

    fn add_stacking_context(&mut self, child: LayoutStackingContext<'a>) {
        match child.context_type {
            StackingContextType::RealStackingContext | StackingContextType::PositionedStackingContainer => {
                self.real_stacking_contexts_and_positioned_stacking_containers.push(child);
            }
            StackingContextType::FloatStackingContainer => {
                self.float_stacking_containers.push(child);
            }
            StackingContextType::AtomicInlineStackingContainer => {
                self.atomic_inline_stacking_containers.push(child);
            }
        }
    }

    pub fn sort(&mut self) {
        self.contents.sort_by_key(|c| c.section());
        self.real_stacking_contexts_and_positioned_stacking_containers
            .sort_by_key(|c| c.z_index());
    }

    pub fn paint_in_order(&self, visitor: &mut impl FnMut(LayoutPaintItem<'a, '_>)) {
        let mut contents = self.contents.iter().peekable();
        let mut outlines: Vec<&LayoutStackingContextContent<'a>> = Vec::new();

        while contents.peek().is_some_and(|c| c.section() == StackingContextSection::OwnBackgroundsAndBorders) {
            let c = contents.next().unwrap();
            emit_content(c, &self.atomic_inline_stacking_containers, visitor);
            if c.has_outline() {
                outlines.push(c);
            }
        }

        let mut positioned = self
            .real_stacking_contexts_and_positioned_stacking_containers
            .iter()
            .peekable();
        while positioned.peek().is_some_and(|c| c.z_index() < 0) {
            visitor(LayoutPaintItem::ChildStackingContext(positioned.next().unwrap()));
        }

        while contents.peek().is_some_and(|c| c.section() == StackingContextSection::DescendantBackgroundsAndBorders) {
            let c = contents.next().unwrap();
            emit_content(c, &self.atomic_inline_stacking_containers, visitor);
            if c.has_outline() {
                outlines.push(c);
            }
        }

        for child in &self.float_stacking_containers {
            visitor(LayoutPaintItem::ChildStackingContext(child));
        }

        while contents.peek().is_some_and(|c| c.section() == StackingContextSection::Foreground) {
            let c = contents.next().unwrap();
            emit_content(c, &self.atomic_inline_stacking_containers, visitor);
            if c.has_outline() {
                outlines.push(c);
            }
        }

        for child in positioned {
            visitor(LayoutPaintItem::ChildStackingContext(child));
        }

        for content in outlines {
            visitor(LayoutPaintItem::Outline(content));
        }
    }
}

fn emit_content<'a, 'b>(
    content: &'b LayoutStackingContextContent<'a>,
    atomic_inlines: &'b [LayoutStackingContext<'a>],
    visitor: &mut impl FnMut(LayoutPaintItem<'a, 'b>),
) {
    match content {
        LayoutStackingContextContent::Fragment { .. } => {
            visitor(LayoutPaintItem::Content(content));
        }
        LayoutStackingContextContent::AtomicInlineStackingContainer { index } => {
            visitor(LayoutPaintItem::ChildStackingContext(&atomic_inlines[*index]));
        }
    }
}

pub(crate) enum LayoutPaintItem<'a, 'b> {
    Content(&'b LayoutStackingContextContent<'a>),
    Outline(&'b LayoutStackingContextContent<'a>),
    ChildStackingContext(&'b LayoutStackingContext<'a>),
}

pub(crate) fn build_stacking_context_tree<'a>(
    fragments: &'a [Fragment],
    scene_builder: &mut crate::scene_builder::RenderSceneBuilder<'a>,
    root_frame_id: usize,
    root_clip_id: SceneClipId,
    scroll_state: &crate::ScrollState,
    owner_semantics: &std::collections::HashMap<usize, crate::render_plan::NodeRenderSemantics>,
) -> LayoutStackingContext<'a> {
    let root_attachment = SpatialAttachment {
        paint_container_id: root_frame_id,
        spatial_node_id: scene_builder.paint_container_spatial_node_id(root_frame_id),
        clip_id: root_clip_id,
        scene_origin: dvec2(0.0, 0.0),
    };
    let mut root = LayoutStackingContext::new_root(root_attachment);
    let root_cb = ContainingBlock {
        paint_container_id: root_frame_id,
        spatial_node_id: root_attachment.spatial_node_id,
        clip_id: root_clip_id,
        rect: PhysicalRect::zero(),
    };
    let cb_info = ContainingBlockInfo {
        for_non_absolute_descendants: root_cb,
        for_absolute_descendants: root_cb,
        for_absolute_and_fixed_descendants: root_cb,
    };
    let mut builder = StackingContextBuilder {
        scene_builder,
        scroll_state,
        owner_semantics,
    };
    for fragment in fragments {
        builder.fragment_build_stacking_context_tree(
            fragment,
            &cb_info,
            &mut root,
            StackingContextBuildMode::SkipHoisted,
        );
    }
    root.sort();
    root
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StackingContextBuildMode {
    IncludeHoisted,
    SkipHoisted,
}

#[derive(Clone, Copy, Debug)]
enum SpatialDescriptor {
    ReferenceFrame(ReferenceFrameData),
    Sticky(StickyNodeData),
    Scroll(ScrollNodeData),
}

struct StackingContextBuilder<'tree, 'a> {
    scene_builder: &'tree mut crate::scene_builder::RenderSceneBuilder<'a>,
    scroll_state: &'tree crate::ScrollState,
    owner_semantics: &'tree std::collections::HashMap<usize, crate::render_plan::NodeRenderSemantics>,
}

impl<'tree, 'a> StackingContextBuilder<'tree, 'a> {
    fn fragment_build_stacking_context_tree(
        &mut self,
        fragment: &'a Fragment,
        containing_block_info: &ContainingBlockInfo,
        stacking_context: &mut LayoutStackingContext<'a>,
        mode: StackingContextBuildMode,
    ) {
        let containing_block = containing_block_info.containing_block_for_fragment(fragment);
        match fragment {
            Fragment::Box(bf) | Fragment::Float(bf) => {
                if mode == StackingContextBuildMode::SkipHoisted
                    && bf.base.style.get_box().position.is_absolutely_positioned()
                {
                    return;
                }
                self.build_for_box(
                    fragment,
                    bf,
                    matches!(fragment, Fragment::Float(_)),
                    containing_block,
                    containing_block_info,
                    stacking_context,
                );
            }
            Fragment::AbsoluteOrFixedPositioned { resolved } => {
                self.fragment_build_stacking_context_tree(
                    resolved,
                    containing_block_info,
                    stacking_context,
                    StackingContextBuildMode::IncludeHoisted,
                );
            }
            Fragment::Positioning(pf) => {
                for child in &pf.children {
                    self.fragment_build_stacking_context_tree(
                        child,
                        containing_block_info,
                        stacking_context,
                        StackingContextBuildMode::SkipHoisted,
                    );
                }
            }
            Fragment::Text(tf) => {
                if tf.base.flags.intersects(FragmentFlags::DO_NOT_PAINT) {
                    return;
                }
                stacking_context.contents.push(LayoutStackingContextContent::Fragment {
                    section: StackingContextSection::Foreground,
                    fragment,
                    attachment: attachment_from_containing_block(containing_block),
                    containing_block: containing_block.rect,
                });
            }
            Fragment::Image(img) => {
                if img.base.flags.intersects(FragmentFlags::DO_NOT_PAINT) {
                    return;
                }
                stacking_context.contents.push(LayoutStackingContextContent::Fragment {
                    section: StackingContextSection::Foreground,
                    fragment,
                    attachment: attachment_from_containing_block(containing_block),
                    containing_block: containing_block.rect,
                });
            }
            Fragment::IFrame(iframe) => {
                if iframe.base.flags.intersects(FragmentFlags::DO_NOT_PAINT) {
                    return;
                }
                stacking_context.contents.push(LayoutStackingContextContent::Fragment {
                    section: StackingContextSection::Foreground,
                    fragment,
                    attachment: attachment_from_containing_block(containing_block),
                    containing_block: containing_block.rect,
                });
            }
        }
    }

    fn build_for_box(
        &mut self,
        fragment: &'a Fragment,
        bf: &'a BoxFragment,
        is_float: bool,
        containing_block: ContainingBlock,
        containing_block_info: &ContainingBlockInfo,
        parent_sc: &mut LayoutStackingContext<'a>,
    ) {
        let context_type = get_stacking_context_type(bf, is_float);

        match context_type {
            Some(ct) => {
                let child_info = self.create_spatial_context_for_box(bf, containing_block, containing_block_info);
                let attachment = attachment_from_containing_block(child_info.for_non_absolute_descendants);
                if ct == StackingContextType::AtomicInlineStackingContainer {
                    parent_sc.contents.push(LayoutStackingContextContent::AtomicInlineStackingContainer {
                        index: parent_sc.atomic_inline_stacking_containers.len(),
                    });
                }

                let mut child_sc = LayoutStackingContext::new_child(bf, ct, attachment);
                child_sc.contents.push(LayoutStackingContextContent::Fragment {
                    section: StackingContextSection::OwnBackgroundsAndBorders,
                    fragment,
                    attachment,
                    containing_block: child_info.for_non_absolute_descendants.rect,
                });
                self.build_box_children(bf, &child_info, &mut child_sc);

                let mut stolen = Vec::new();
                if ct != StackingContextType::RealStackingContext {
                    stolen = std::mem::take(&mut child_sc.real_stacking_contexts_and_positioned_stacking_containers);
                }

                child_sc.sort();
                parent_sc.add_stacking_context(child_sc);
                parent_sc.real_stacking_contexts_and_positioned_stacking_containers.append(&mut stolen);
            }
            None => {
                let child_info = self.create_spatial_context_for_box(bf, containing_block, containing_block_info);
                let attachment = attachment_from_containing_block(child_info.for_non_absolute_descendants);
                parent_sc.contents.push(LayoutStackingContextContent::Fragment {
                    section: get_section_for_non_sc(bf),
                    fragment,
                    attachment,
                    containing_block: child_info.for_non_absolute_descendants.rect,
                });
                self.build_box_children(bf, &child_info, parent_sc);
            }
        }
    }

    fn build_box_children(
        &mut self,
        bf: &'a BoxFragment,
        containing_block_info: &ContainingBlockInfo,
        stacking_context: &mut LayoutStackingContext<'a>,
    ) {
        for child in &bf.children {
            self.fragment_build_stacking_context_tree(
                child,
                containing_block_info,
                stacking_context,
                StackingContextBuildMode::SkipHoisted,
            );
        }
    }

    fn create_spatial_context_for_box(
        &mut self,
        bf: &'a BoxFragment,
        containing_block: ContainingBlock,
        containing_block_info: &ContainingBlockInfo,
    ) -> ContainingBlockInfo {
        let owner_node_id = bf.base.tag.map(|tag| {
            let pseudo_key = match bf.base.style.pseudo() {
                Some(style::selector_parser::PseudoElement::Before) => 1,
                Some(style::selector_parser::PseudoElement::After) => 2,
                Some(style::selector_parser::PseudoElement::Marker) => 3,
                Some(style::selector_parser::PseudoElement::ServoAnonymousBox) => 4,
                Some(style::selector_parser::PseudoElement::ServoAnonymousTable) => 5,
                Some(style::selector_parser::PseudoElement::ServoAnonymousTableCell) => 6,
                Some(style::selector_parser::PseudoElement::ServoAnonymousTableRow) => 7,
                Some(_) => 15,
                None => 0,
            };
            (tag.node.0 << 8) ^ pseudo_key
        });
        let mut new_containing_block = containing_block;

        for descriptor in self.spatial_descriptors_for_box(bf, owner_node_id) {
            let spatial_node_id = match descriptor {
                SpatialDescriptor::ReferenceFrame(data) => self.scene_builder.child_reference_frame(
                    new_containing_block.spatial_node_id,
                    owner_node_id,
                    data,
                ),
                SpatialDescriptor::Sticky(data) => self.scene_builder.child_sticky_node(
                    new_containing_block.spatial_node_id,
                    owner_node_id,
                    data,
                ),
                SpatialDescriptor::Scroll(data) => self.scene_builder.child_scroll_node(
                    new_containing_block.spatial_node_id,
                    owner_node_id,
                    data,
                ),
            };
            let parent_rect_origin = new_containing_block.rect.origin;
            let paint_container_id = self.scene_builder.child_paint_container(
                new_containing_block.paint_container_id,
                spatial_node_id,
                owner_node_id,
            );
            new_containing_block.paint_container_id = paint_container_id;
            new_containing_block.spatial_node_id = spatial_node_id;
            new_containing_block.rect.origin = new_containing_block.rect.origin - parent_rect_origin.to_vector();
        }

        if let Some(css_clip_rect) = css_clip_rect(bf, new_containing_block.rect) {
            let clip_id = self.scene_builder.rect_clip(
                new_containing_block.paint_container_id,
                new_containing_block.clip_id,
                css_clip_rect,
                SceneClipKind::CssClip,
            );
            self.scene_builder.set_frame_clip(new_containing_block.paint_container_id, clip_id);
            new_containing_block.clip_id = clip_id;
        }

        if let Some(rect) = bf.scrollable_overflow {
            let overflow_kind = overflow_clip_kind(&bf.base.style);
            let clip_id = self.scene_builder.rect_clip(
                new_containing_block.paint_container_id,
                new_containing_block.clip_id,
                Rect {
                    pos: dvec2(
                        new_containing_block.rect.origin.x.to_f32_px() as f64 + rect.origin.x.to_f32_px() as f64,
                        new_containing_block.rect.origin.y.to_f32_px() as f64 + rect.origin.y.to_f32_px() as f64,
                    ),
                    size: dvec2(
                        rect.size.width.to_f32_px() as f64,
                        rect.size.height.to_f32_px() as f64,
                    ),
                },
                overflow_kind,
            );
            self.scene_builder.set_frame_clip(new_containing_block.paint_container_id, clip_id);
            new_containing_block.clip_id = clip_id;
        }

        let border_rect = bf.border_rect().translate(bf.cumulative_containing_block_rect.origin.to_vector());
        let padding_rect = bf.padding_rect().translate(bf.cumulative_containing_block_rect.origin.to_vector());
        let content_rect = bf.content_rect().translate(bf.cumulative_containing_block_rect.origin.to_vector());

        let for_absolute_descendants = ContainingBlock {
            paint_container_id: new_containing_block.paint_container_id,
            spatial_node_id: new_containing_block.spatial_node_id,
            clip_id: new_containing_block.clip_id,
            rect: padding_rect,
        };
        let for_non_absolute_descendants = ContainingBlock {
            paint_container_id: new_containing_block.paint_container_id,
            spatial_node_id: new_containing_block.spatial_node_id,
            clip_id: new_containing_block.clip_id,
            rect: content_rect,
        };
        let for_absolute_and_fixed_descendants = ContainingBlock {
            paint_container_id: new_containing_block.paint_container_id,
            spatial_node_id: new_containing_block.spatial_node_id,
            clip_id: new_containing_block.clip_id,
            rect: border_rect,
        };

        if crate::transform::has_effective_transform_or_perspective(&bf.base.style) {
            ContainingBlockInfo {
                for_non_absolute_descendants,
                for_absolute_descendants,
                for_absolute_and_fixed_descendants,
            }
        } else if bf.base.style.get_box().position != ComputedPosition::Static {
            ContainingBlockInfo {
                for_non_absolute_descendants,
                for_absolute_descendants,
                for_absolute_and_fixed_descendants: containing_block_info.for_absolute_and_fixed_descendants,
            }
        } else {
            containing_block_info.new_for_non_absolute_descendants(for_non_absolute_descendants)
        }
    }

    fn spatial_descriptors_for_box(
        &self,
        bf: &'a BoxFragment,
        owner_node_id: Option<usize>,
    ) -> Vec<SpatialDescriptor> {
        let mut descriptors = Vec::new();

        let flatten_3d = owner_node_id
            .and_then(|node_id| self.owner_semantics.get(&node_id).copied())
            .map(|semantics| !semantics.requires_surface_composition())
            .unwrap_or(true);
        let current_origin = dvec2(
            bf.cumulative_containing_block_rect.origin.x.to_f32_px() as f64,
            bf.cumulative_containing_block_rect.origin.y.to_f32_px() as f64,
        );
        if let Some(reference_frame) = crate::reference_frame::reference_frame_semantics(bf, current_origin, flatten_3d) {
            descriptors.push(SpatialDescriptor::ReferenceFrame(ReferenceFrameData {
                origin: reference_frame.origin,
                placement_origin: reference_frame.placement_origin,
                transform_matrix: reference_frame.transform_matrix,
                perspective_matrix: reference_frame.perspective_matrix,
                has_transform: reference_frame.has_transform,
                has_perspective: reference_frame.has_perspective,
                preserves_3d: !flatten_3d,
                anchors_content: reference_frame.is_invertible,
            }));
        }

        if let Some(insets) = bf.resolved_sticky_insets {
            if has_sticky_offset_constraints(insets) {
                let scroll_frame_rect = physical_rect_to_rect(bf.cumulative_containing_block_rect);
                let containing_block_rect = physical_rect_to_rect(bf.cumulative_containing_block_rect);
                let frame_rect = physical_rect_to_rect(
                    bf.border_rect().translate(bf.cumulative_containing_block_rect.origin.to_vector()),
                );
                let computed_margin = bf.base.style.get_margin();
                let border_rect = bf.border_rect();
                let distance_top = border_rect.min_y();
                let distance_right = bf.cumulative_containing_block_rect.width() - border_rect.max_x();
                let distance_bottom = bf.cumulative_containing_block_rect.height() - border_rect.max_y();
                let distance_left = border_rect.min_x();
                let offset_bound = |distance: app_units::Au,
                                    used_margin: app_units::Au,
                                    computed_margin_auto: bool| {
                    let used_margin = if computed_margin_auto {
                        app_units::Au::zero()
                    } else {
                        used_margin
                    };
                    app_units::Au::zero().max(distance - used_margin).to_f32_px()
                };
                descriptors.push(SpatialDescriptor::Sticky(StickyNodeData {
                    frame_rect,
                    margins: crate::scene::StickyOffsetConstraints {
                        top: insets.top.non_auto().map(|v| v.to_f32_px()),
                        right: insets.right.non_auto().map(|v| v.to_f32_px()),
                        bottom: insets.bottom.non_auto().map(|v| v.to_f32_px()),
                        left: insets.left.non_auto().map(|v| v.to_f32_px()),
                    },
                    vertical_offset_bounds: StickyOffsetBounds {
                        min: -offset_bound(distance_top, bf.margin.top, computed_margin.margin_top.is_auto()),
                        max: offset_bound(distance_bottom, bf.margin.bottom, computed_margin.margin_bottom.is_auto()),
                    },
                    horizontal_offset_bounds: StickyOffsetBounds {
                        min: -offset_bound(distance_left, bf.margin.left, computed_margin.margin_left.is_auto()),
                        max: offset_bound(distance_right, bf.margin.right, computed_margin.margin_right.is_auto()),
                    },
                    containing_block_rect,
                    scroll_frame_rect,
                    scroll_port_rect: scroll_frame_rect,
                    nearest_scroll_node_id: None,
                }));
            }
        }

        if let Some(scrollable_overflow) = bf.scrollable_overflow {
            let scroll_offset = fragment_scroll_offset(bf, self.scroll_state)
                .unwrap_or_else(|| dvec2(0.0, 0.0));
            let overflow = bf.base.style.get_box();
            descriptors.push(SpatialDescriptor::Scroll(ScrollNodeData {
                scroll_offset,
                scroll_frame_rect: physical_rect_to_rect(bf.cumulative_containing_block_rect),
                content_rect: physical_rect_to_rect(scrollable_overflow),
                sensitivity_x: matches!(overflow.overflow_x, ComputedOverflow::Auto | ComputedOverflow::Scroll),
                sensitivity_y: matches!(overflow.overflow_y, ComputedOverflow::Auto | ComputedOverflow::Scroll),
                external_scroll_node_id: bf.base.tag.map(|tag| tag.node.0),
            }));
        }

        descriptors
    }
}

fn attachment_from_containing_block(containing_block: ContainingBlock) -> SpatialAttachment {
    SpatialAttachment {
        paint_container_id: containing_block.paint_container_id,
        spatial_node_id: containing_block.spatial_node_id,
        clip_id: containing_block.clip_id,
        scene_origin: dvec2(
            containing_block.rect.origin.x.to_f32_px() as f64,
            containing_block.rect.origin.y.to_f32_px() as f64,
        ),
    }
}

fn fragment_scroll_offset(
    bf: &BoxFragment,
    scroll_state: &crate::ScrollState,
) -> Option<makepad_widgets::DVec2> {
    let node_id = bf.base.tag.map(|tag| tag.node.0)?;
    Some(scroll_state.get(&node_id).copied().unwrap_or(dvec2(0.0, 0.0)))
}

fn has_sticky_offset_constraints(insets: havi_types::PhysicalSides<havi_types::AuOrAuto>) -> bool {
    insets.top.non_auto().is_some()
        || insets.right.non_auto().is_some()
        || insets.bottom.non_auto().is_some()
        || insets.left.non_auto().is_some()
}

fn physical_rect_to_rect(rect: PhysicalRect<app_units::Au>) -> Rect {
    Rect {
        pos: dvec2(rect.origin.x.to_f32_px() as f64, rect.origin.y.to_f32_px() as f64),
        size: dvec2(rect.size.width.to_f32_px() as f64, rect.size.height.to_f32_px() as f64),
    }
}

fn css_clip_rect(
    bf: &BoxFragment,
    containing_block_rect: PhysicalRect<app_units::Au>,
) -> Option<Rect> {
    if !bf.base.style.get_box().position.is_absolutely_positioned() {
        return None;
    }
    let clip_rect = match bf.base.style.get_effects().clip {
        ClipRectOrAuto::Rect(rect) => rect,
        _ => return None,
    };
    let border_rect = bf.border_rect();
    let clip_rect = clip_rect
        .for_border_rect(border_rect)
        .translate(containing_block_rect.origin.to_vector());
    Some(physical_rect_to_rect(clip_rect))
}

fn overflow_clip_kind(style: &ComputedValues) -> SceneClipKind {
    let overflow = style.get_box();
    if matches!(overflow.overflow_x, ComputedOverflow::Clip)
        || matches!(overflow.overflow_y, ComputedOverflow::Clip)
    {
        SceneClipKind::OverflowClip
    } else {
        SceneClipKind::Overflow
    }
}

fn get_stacking_context_type(bf: &BoxFragment, is_float: bool) -> Option<StackingContextType> {
    let style = &bf.base.style;
    let flags = bf.base.flags;

    if flags.intersects(FragmentFlags::DO_NOT_PAINT) {
        return None;
    }
    if establishes_stacking_context(style, flags) {
        return Some(StackingContextType::RealStackingContext);
    }
    if style.get_box().position != ComputedPosition::Static {
        return Some(StackingContextType::PositionedStackingContainer);
    }
    if is_float {
        return Some(StackingContextType::FloatStackingContainer);
    }
    if is_atomic_inline_level(style, flags) {
        return Some(StackingContextType::AtomicInlineStackingContainer);
    }
    None
}

fn is_atomic_inline_level(style: &ComputedValues, flags: FragmentFlags) -> bool {
    style.get_box().display.outside() == DisplayOutside::Inline && !is_inline_box(style, flags)
}

fn is_inline_box(style: &ComputedValues, flags: FragmentFlags) -> bool {
    style.get_box().display.is_inline_flow()
        && !flags.intersects(FragmentFlags::IS_REPLACED | FragmentFlags::IS_WIDGET)
}

fn get_section_for_non_sc(bf: &BoxFragment) -> StackingContextSection {
    if bf.base.style.get_box().display.outside() == DisplayOutside::Inline {
        StackingContextSection::Foreground
    } else {
        StackingContextSection::DescendantBackgroundsAndBorders
    }
}

fn effective_z_index(style: &ComputedValues, _flags: FragmentFlags) -> i32 {
    if style.get_box().position != ComputedPosition::Static {
        style.get_position().z_index.integer_or(0)
    } else {
        0
    }
}

fn establishes_stacking_context(style: &ComputedValues, flags: FragmentFlags) -> bool {
    if style.get_box().position != ComputedPosition::Static && !style.get_position().z_index.is_auto() {
        return true;
    }
    if matches!(style.get_box().position, ComputedPosition::Fixed | ComputedPosition::Sticky) {
        return true;
    }
    if crate::transform::has_effective_transform_or_perspective(style)
        || style.get_box().transform_style == ComputedTransformStyle::Preserve3d
    {
        return true;
    }
    if style.get_effects().opacity != 1.0 {
        return true;
    }
    if !style.get_effects().filter.0.is_empty() {
        return true;
    }
    if style.get_effects().mix_blend_mode != ComputedMixBlendMode::Normal {
        return true;
    }
    if style.get_svg().clip_path != ClipPath::None {
        return true;
    }
    if flags.intersects(FragmentFlags::IS_ROOT_ELEMENT) {
        return true;
    }
    let overflow = style.get_box();
    if !matches!(overflow.overflow_x, ComputedOverflow::Visible)
        || !matches!(overflow.overflow_y, ComputedOverflow::Visible)
    {
        return true;
    }
    false
}
