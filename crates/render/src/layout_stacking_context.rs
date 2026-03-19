//! Semantic stacking-context construction over layout-style fragment semantics.
//!
//! This is the active render paint-order path.
//!
//! The builder preserves placeholder semantics for hoisted absolute/fixed
//! descendants. A placeholder participates at its original tree position and
//! paints the hoisted fragment exactly once through that position.

use havi_types::fragment_tree::{BoxFragment, FragmentFlags};
use havi_types::Fragment;
use style::computed_values::mix_blend_mode::T as ComputedMixBlendMode;
use style::computed_values::overflow_x::T as ComputedOverflow;
use style::computed_values::position::T as ComputedPosition;
use style::computed_values::transform_style::T as ComputedTransformStyle;
use style::properties::ComputedValues;
use style::values::computed::basic_shape::ClipPath;
use style::values::specified::box_::DisplayOutside;
use style::Zero;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum StackingContextSection {
    OwnBackgroundsAndBorders,
    DescendantBackgroundsAndBorders,
    Foreground,
    #[allow(dead_code)]
    Outline,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StackingContextType {
    RealStackingContext,
    PositionedStackingContainer,
    FloatStackingContainer,
    AtomicInlineStackingContainer,
}

pub(crate) enum LayoutStackingContextContent<'a> {
    Fragment {
        section: StackingContextSection,
        fragment: &'a Fragment,
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
    fn new_root() -> Self {
        Self {
            initializing_fragment: None,
            context_type: StackingContextType::RealStackingContext,
            contents: Vec::new(),
            real_stacking_contexts_and_positioned_stacking_containers: Vec::new(),
            float_stacking_containers: Vec::new(),
            atomic_inline_stacking_containers: Vec::new(),
        }
    }

    fn new_child(bf: &'a BoxFragment, context_type: StackingContextType) -> Self {
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

        for _ in outlines {
            visitor(LayoutPaintItem::Outline);
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
    ChildStackingContext(&'b LayoutStackingContext<'a>),
    Outline,
}

pub(crate) fn build_stacking_context_tree<'a>(fragments: &'a [Fragment]) -> LayoutStackingContext<'a> {
    let mut root = LayoutStackingContext::new_root();
    let hoisted = collect_hoisted_fragments(fragments);
    let mut resolving = std::collections::HashSet::new();
    for fragment in fragments {
        build_fragment(fragment, BuildMode::SkipHoisted, &mut root, &hoisted, &mut resolving);
    }
    root.sort();
    root
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BuildMode {
    SkipHoisted,
    IncludeHoisted,
}

fn build_fragment<'a>(
    fragment: &'a Fragment,
    mode: BuildMode,
    stacking_context: &mut LayoutStackingContext<'a>,
    hoisted: &std::collections::HashMap<usize, &'a Fragment>,
    resolving: &mut std::collections::HashSet<usize>,
) {
    match fragment {
        Fragment::Box(bf) => {
            if mode == BuildMode::SkipHoisted
                && bf.base.style.get_box().position.is_absolutely_positioned()
            {
                return;
            }
            build_for_box(fragment, bf, false, stacking_context, hoisted, resolving);
        }
        Fragment::Float(bf) => {
            if mode == BuildMode::SkipHoisted
                && bf.base.style.get_box().position.is_absolutely_positioned()
            {
                return;
            }
            build_for_box(fragment, bf, true, stacking_context, hoisted, resolving);
        }
        Fragment::AbsoluteOrFixedPositioned { resolved } => {
            let key = std::ptr::from_ref(&**resolved) as usize;
            if !resolving.insert(key) {
                return;
            }
            build_fragment(resolved, BuildMode::IncludeHoisted, stacking_context, hoisted, resolving);
            resolving.remove(&key);
        }
        Fragment::Text(tf) => {
            if tf.base.flags.intersects(FragmentFlags::DO_NOT_PAINT) {
                return;
            }
            stacking_context.contents.push(LayoutStackingContextContent::Fragment {
                section: StackingContextSection::Foreground,
                fragment,
            });
        }
        Fragment::Image(img) => {
            if img.base.flags.intersects(FragmentFlags::DO_NOT_PAINT) {
                return;
            }
            stacking_context.contents.push(LayoutStackingContextContent::Fragment {
                section: StackingContextSection::Foreground,
                fragment,
            });
        }
        Fragment::IFrame(iframe) => {
            if iframe.base.flags.intersects(FragmentFlags::DO_NOT_PAINT) {
                return;
            }
            stacking_context.contents.push(LayoutStackingContextContent::Fragment {
                section: StackingContextSection::Foreground,
                fragment,
            });
        }
        Fragment::Positioning(pf) => {
            for child in &pf.children {
                build_fragment(child, BuildMode::SkipHoisted, stacking_context, hoisted, resolving);
            }
        }
    }
}

fn build_for_box<'a>(
    fragment: &'a Fragment,
    bf: &'a BoxFragment,
    is_float: bool,
    parent_sc: &mut LayoutStackingContext<'a>,
    hoisted: &std::collections::HashMap<usize, &'a Fragment>,
    resolving: &mut std::collections::HashSet<usize>,
) {
    let context_type = get_stacking_context_type(bf, is_float);
    match context_type {
        Some(ct) => {
            if ct == StackingContextType::AtomicInlineStackingContainer {
                parent_sc.contents.push(LayoutStackingContextContent::AtomicInlineStackingContainer {
                    index: parent_sc.atomic_inline_stacking_containers.len(),
                });
            }

            let mut child_sc = LayoutStackingContext::new_child(bf, ct);
            child_sc.contents.push(LayoutStackingContextContent::Fragment {
                section: StackingContextSection::OwnBackgroundsAndBorders,
                fragment,
            });
            build_box_children(bf, &mut child_sc, hoisted, resolving);

            let mut stolen = Vec::new();
            if ct != StackingContextType::RealStackingContext {
                stolen = std::mem::take(&mut child_sc.real_stacking_contexts_and_positioned_stacking_containers);
            }

            child_sc.sort();
            parent_sc.add_stacking_context(child_sc);
            parent_sc.real_stacking_contexts_and_positioned_stacking_containers.append(&mut stolen);
        }
        None => {
            parent_sc.contents.push(LayoutStackingContextContent::Fragment {
                section: get_section_for_non_sc(bf),
                fragment,
            });
            build_box_children(bf, parent_sc, hoisted, resolving);
        }
    }
}

fn build_box_children<'a>(
    bf: &'a BoxFragment,
    stacking_context: &mut LayoutStackingContext<'a>,
    hoisted: &std::collections::HashMap<usize, &'a Fragment>,
    resolving: &mut std::collections::HashSet<usize>,
) {
    for child in &bf.children {
        build_fragment(child, BuildMode::SkipHoisted, stacking_context, hoisted, resolving);
    }
}

fn collect_hoisted_fragments<'a>(fragments: &'a [Fragment]) -> std::collections::HashMap<usize, &'a Fragment> {
    let mut hoisted = std::collections::HashMap::new();
    for fragment in fragments {
        collect_hoisted_fragment(fragment, &mut hoisted);
    }
    hoisted
}

fn collect_hoisted_fragment<'a>(
    fragment: &'a Fragment,
    hoisted: &mut std::collections::HashMap<usize, &'a Fragment>,
) {
    match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => {
            if bf.base.style.get_box().position.is_absolutely_positioned() {
                let key = bf.base.tag.map(|tag| tag.node.0).unwrap_or(std::ptr::from_ref(fragment) as usize);
                hoisted.entry(key).or_insert(fragment);
            }
            for child in &bf.children {
                collect_hoisted_fragment(child, hoisted);
            }
        }
        Fragment::Positioning(pf) => {
            for child in &pf.children {
                collect_hoisted_fragment(child, hoisted);
            }
        }
        Fragment::IFrame(iframe) => {
            for child in iframe.child_fragments.iter() {
                collect_hoisted_fragment(child, hoisted);
            }
        }
        Fragment::AbsoluteOrFixedPositioned { .. } | Fragment::Text(_) | Fragment::Image(_) => {}
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
