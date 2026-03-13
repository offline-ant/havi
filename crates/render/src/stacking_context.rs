//! Stacking context tree for CSS 2.1 Appendix E paint ordering.
//!
//! Builds a tree of stacking contexts from a fragment slice, then provides
//! iteration in correct CSS paint order. Adapted from servo-mainline's
//! `components/layout/display_list/stacking_context.rs`.

use std::mem::ManuallyDrop;
use std::sync::Arc;

use havi_types::fragment_tree::{BoxFragment, FragmentFlags};
use havi_types::Fragment;
use style::computed_values::mix_blend_mode::T as ComputedMixBlendMode;
use style::computed_values::overflow_x::T as ComputedOverflow;
use style::computed_values::position::T as ComputedPosition;
use style::properties::ComputedValues;
use style::values::computed::basic_shape::ClipPath;
use style::values::specified::box_::DisplayOutside;
use style::Zero;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Section within a stacking context, controlling paint order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum StackingContextSection {
    OwnBackgroundsAndBorders,
    DescendantBackgroundsAndBorders,
    Foreground,
    #[allow(dead_code)]
    Outline,
}

/// Type of stacking context or stacking container.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StackingContextType {
    RealStackingContext,
    PositionedStackingContainer,
    FloatStackingContainer,
    AtomicInlineStackingContainer,
}

/// A content item inside a stacking context.
pub(crate) enum StackingContextContent<'a> {
    /// A fragment reference with its paint section.
    Fragment {
        section: StackingContextSection,
        fragment: &'a Fragment,
    },
    /// Index into `StackingContext::atomic_inline_stacking_containers`.
    AtomicInlineStackingContainer { index: usize },
}

impl StackingContextContent<'_> {
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
                    !outline.outline_style.none_or_hidden()
                        && !outline.outline_width.0.is_zero()
                }
                _ => false,
            },
            Self::AtomicInlineStackingContainer { .. } => false,
        }
    }
}

/// A stacking context node in the tree.
pub(crate) struct StackingContext<'a> {
    pub initializing_fragment: Option<&'a BoxFragment>,
    #[allow(dead_code)]
    pub is_float_fragment: bool,
    pub context_type: StackingContextType,
    pub contents: Vec<StackingContextContent<'a>>,
    pub real_stacking_contexts_and_positioned_stacking_containers: Vec<StackingContext<'a>>,
    pub float_stacking_containers: Vec<StackingContext<'a>>,
    pub atomic_inline_stacking_containers: Vec<StackingContext<'a>>,
}

impl<'a> StackingContext<'a> {
    fn new_root() -> Self {
        Self {
            initializing_fragment: None,
            is_float_fragment: false,
            context_type: StackingContextType::RealStackingContext,
            contents: Vec::new(),
            real_stacking_contexts_and_positioned_stacking_containers: Vec::new(),
            float_stacking_containers: Vec::new(),
            atomic_inline_stacking_containers: Vec::new(),
        }
    }

    fn new_child(
        bf: &'a BoxFragment,
        is_float: bool,
        context_type: StackingContextType,
    ) -> Self {
        Self {
            initializing_fragment: Some(bf),
            is_float_fragment: is_float,
            context_type,
            contents: Vec::new(),
            real_stacking_contexts_and_positioned_stacking_containers: Vec::new(),
            float_stacking_containers: Vec::new(),
            atomic_inline_stacking_containers: Vec::new(),
        }
    }

    /// True when this stacking context contains no child stacking contexts.
    /// Opacity isolation is unnecessary for leaf contexts because there are no
    /// overlapping child layers that could double-blend.
    pub fn is_leaf(&self) -> bool {
        self.real_stacking_contexts_and_positioned_stacking_containers.is_empty()
            && self.float_stacking_containers.is_empty()
            && self.atomic_inline_stacking_containers.is_empty()
    }

    pub fn z_index(&self) -> i32 {
        self.initializing_fragment
            .map(|f| effective_z_index(&f.base.style, f.base.flags))
            .unwrap_or(0)
    }

    fn add_stacking_context(&mut self, child: StackingContext<'a>) {
        match child.context_type {
            StackingContextType::RealStackingContext
            | StackingContextType::PositionedStackingContainer => {
                self.real_stacking_contexts_and_positioned_stacking_containers
                    .push(child);
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

    /// Walk this stacking context in CSS 2.1 Appendix E paint order,
    /// calling the visitor for each fragment and recursing into child contexts.
    pub fn paint_in_order(&self, visitor: &mut impl FnMut(PaintItem<'a, '_>)) {
        // Steps 1-2: Own backgrounds and borders.
        let mut contents = self.contents.iter().peekable();
        let mut outlines: Vec<&StackingContextContent<'a>> = Vec::new();

        while contents
            .peek()
            .is_some_and(|c| c.section() == StackingContextSection::OwnBackgroundsAndBorders)
        {
            let c = contents.next().unwrap();
            emit_content(c, &self.atomic_inline_stacking_containers, visitor);
            if c.has_outline() {
                outlines.push(c);
            }
        }

        // Step 3: Child stacking contexts with negative z-index.
        let mut positioned = self
            .real_stacking_contexts_and_positioned_stacking_containers
            .iter()
            .peekable();
        while positioned.peek().is_some_and(|c| c.z_index() < 0) {
            let child = positioned.next().unwrap();
            visitor(PaintItem::ChildStackingContext(child));
        }

        // Step 4: Block-level descendants' backgrounds and borders.
        while contents
            .peek()
            .is_some_and(|c| c.section() == StackingContextSection::DescendantBackgroundsAndBorders)
        {
            let c = contents.next().unwrap();
            emit_content(c, &self.atomic_inline_stacking_containers, visitor);
            if c.has_outline() {
                outlines.push(c);
            }
        }

        // Step 5: Float stacking containers.
        for child in &self.float_stacking_containers {
            visitor(PaintItem::ChildStackingContext(child));
        }

        // Steps 6-7: Foreground (inline content, text, images, atomic inlines).
        while contents
            .peek()
            .is_some_and(|c| c.section() == StackingContextSection::Foreground)
        {
            let c = contents.next().unwrap();
            emit_content(c, &self.atomic_inline_stacking_containers, visitor);
            if c.has_outline() {
                outlines.push(c);
            }
        }

        // Steps 8-9: Positioned with z-index >= 0.
        for child in positioned {
            visitor(PaintItem::ChildStackingContext(child));
        }

        // Step 10: Outlines.
        for c in outlines {
            visitor(PaintItem::Outline(c));
        }
    }
}

fn emit_content<'a, 'b>(
    content: &'b StackingContextContent<'a>,
    atomic_inlines: &'b [StackingContext<'a>],
    visitor: &mut impl FnMut(PaintItem<'a, 'b>),
) {
    match content {
        StackingContextContent::Fragment { .. } => {
            visitor(PaintItem::Content(content));
        }
        StackingContextContent::AtomicInlineStackingContainer { index } => {
            visitor(PaintItem::ChildStackingContext(&atomic_inlines[*index]));
        }
    }
}

/// Items yielded during paint-order traversal.
pub(crate) enum PaintItem<'a, 'b> {
    /// A fragment to paint.
    Content(&'b StackingContextContent<'a>),
    /// A child stacking context to recurse into.
    ChildStackingContext(&'b StackingContext<'a>),
    /// An outline to paint (step 10).
    Outline(&'b StackingContextContent<'a>),
}

// ---------------------------------------------------------------------------
// Tree construction
// ---------------------------------------------------------------------------

/// Build a stacking context tree from root fragments.
pub(crate) fn build_stacking_context_tree<'a>(
    fragments: &'a [Fragment],
) -> StackingContext<'a> {
    let mut root = StackingContext::new_root();
    for fragment in fragments {
        build_for_fragment(fragment, &mut root);
    }
    root.sort();
    root
}

fn build_for_fragment<'a>(
    fragment: &'a Fragment,
    stacking_context: &mut StackingContext<'a>,
) {
    match fragment {
        Fragment::Box(bf) => {
            build_for_box(fragment, bf, false, stacking_context);
        }
        Fragment::Float(bf) => {
            build_for_box(fragment, bf, true, stacking_context);
        }
        Fragment::Text(_) | Fragment::Image(_) | Fragment::IFrame(_) => {
            stacking_context
                .contents
                .push(StackingContextContent::Fragment {
                    section: StackingContextSection::Foreground,
                    fragment,
                });
        }
        Fragment::Positioning(pf) => {
            for child in &pf.children {
                build_for_fragment(child, stacking_context);
            }
        }
    }
}

fn build_for_box<'a>(
    fragment: &'a Fragment,
    bf: &'a BoxFragment,
    is_float: bool,
    parent_sc: &mut StackingContext<'a>,
) {
    let context_type = get_stacking_context_type(bf, is_float);

    match context_type {
        Some(ct) => {
            // Atomic inline: push placeholder into parent contents.
            if ct == StackingContextType::AtomicInlineStackingContainer {
                parent_sc
                    .contents
                    .push(StackingContextContent::AtomicInlineStackingContainer {
                        index: parent_sc.atomic_inline_stacking_containers.len(),
                    });
            }

            let mut child_sc = StackingContext::new_child(bf, is_float, ct);

            // Own backgrounds/borders.
            child_sc
                .contents
                .push(StackingContextContent::Fragment {
                    section: StackingContextSection::OwnBackgroundsAndBorders,
                    fragment,
                });

            // Build children.
            build_box_children(bf, &mut child_sc);

            // Steal real stacking contexts from non-real containers.
            let mut stolen = Vec::new();
            if ct != StackingContextType::RealStackingContext {
                stolen = std::mem::take(
                    &mut child_sc.real_stacking_contexts_and_positioned_stacking_containers,
                );
            }

            child_sc.sort();
            parent_sc.add_stacking_context(child_sc);
            parent_sc
                .real_stacking_contexts_and_positioned_stacking_containers
                .append(&mut stolen);
        }
        None => {
            // No stacking context — add directly to parent.
            let section = get_section_for_non_sc(bf);
            parent_sc
                .contents
                .push(StackingContextContent::Fragment {
                    section,
                    fragment,
                });
            build_box_children(bf, parent_sc);
        }
    }
}

fn build_box_children<'a>(
    bf: &'a BoxFragment,
    stacking_context: &mut StackingContext<'a>,
) {
    for child in &bf.children {
        build_for_fragment(child, stacking_context);
    }
}

// ---------------------------------------------------------------------------
// Style classification helpers
// ---------------------------------------------------------------------------

fn get_stacking_context_type(bf: &BoxFragment, is_float: bool) -> Option<StackingContextType> {
    let style = &bf.base.style;
    let flags = bf.base.flags;

    // Table wrappers have DO_NOT_PAINT — they delegate painting (and stacking
    // context creation) to the inner table grid box which shares the same
    // OpaqueNode. Creating a stacking context here would produce two nested
    // contexts with the same node_id, causing DrawPass self-cycles.
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

    // Atomic inline-level: inline-block, inline-table, replaced elements.
    if style.get_box().display.outside() == DisplayOutside::Inline {
        return Some(StackingContextType::AtomicInlineStackingContainer);
    }

    None
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
    // Positioned with explicit z-index.
    if style.get_box().position != ComputedPosition::Static
        && !style.get_position().z_index.is_auto()
    {
        return true;
    }

    // Fixed and sticky always create stacking contexts.
    if matches!(
        style.get_box().position,
        ComputedPosition::Fixed | ComputedPosition::Sticky
    ) {
        return true;
    }

    // Transform.
    if !style.get_box().transform.0.is_empty() {
        return true;
    }

    // Opacity < 1.
    if style.get_effects().opacity != 1.0 {
        return true;
    }

    // Filters.
    if !style.get_effects().filter.0.is_empty() {
        return true;
    }

    // Mix blend mode.
    if style.get_effects().mix_blend_mode != ComputedMixBlendMode::Normal {
        return true;
    }

    // Clip-path.
    if style.get_svg().clip_path != ClipPath::None {
        return true;
    }

    // Root element.
    if flags.intersects(FragmentFlags::IS_ROOT_ELEMENT) {
        return true;
    }

    // Overflow containers (scroll/auto/hidden) need their own stacking context
    // so that scroll offsets and clip rects can be applied during rendering.
    let overflow = style.get_box();
    if !matches!(overflow.overflow_x, ComputedOverflow::Visible)
        || !matches!(overflow.overflow_y, ComputedOverflow::Visible)
    {
        return true;
    }

    false
}

// ---------------------------------------------------------------------------
// Cached stacking context tree
// ---------------------------------------------------------------------------

/// A stacking context tree cached alongside the `Arc<Vec<Fragment>>` it borrows from.
///
/// The tree references data inside the `Arc`. As long as the `Arc` is held, the
/// references are valid. The tree is rebuilt only when the fragment `Arc` changes
/// (detected by data pointer comparison).
///
/// # Safety
///
/// `tree` holds references into `_fragments`'s heap allocation. The `'static`
/// lifetime is a transmuted lie. `ManuallyDrop` + an explicit `Drop` impl
/// ensures `tree` is always dropped before `_fragments`, so no dangling
/// references exist during destruction. The `Arc` guarantees the heap
/// allocation doesn't move or deallocate while this struct is alive.
///
/// Alternatives considered:
/// - **Indices instead of references**: Would require threading `&[Fragment]`
///   through `paint_in_order`, `PaintItem`, and ~20 consumer sites in
///   `makepad_builder.rs`. Significantly more complex API for the same result.
/// - **`self_cell` crate**: Clean but adds a dependency for one use site.
/// - **Rebuild every frame**: Correct but defeats the purpose of caching.
pub struct CachedStackingContextTree {
    /// The built stacking context tree. Dropped first via `ManuallyDrop` +
    /// explicit `Drop` impl. Lifetime is tied to `_fragments`.
    tree: ManuallyDrop<StackingContext<'static>>,
    /// Kept alive to guarantee that `tree` references remain valid.
    /// Dropped after `tree`.
    _fragments: Arc<Vec<Fragment>>,
    /// Data pointer of the `Arc<Vec<Fragment>>` used to build this tree.
    frag_ptr: usize,
}

impl Drop for CachedStackingContextTree {
    fn drop(&mut self) {
        // SAFETY: Drop the tree first while `_fragments` is still alive.
        // After this, `_fragments` drops normally via its own Drop.
        unsafe {
            ManuallyDrop::drop(&mut self.tree);
        }
    }
}

impl CachedStackingContextTree {
    /// Build a new cached tree from the given fragments.
    pub fn new(fragments: Arc<Vec<Fragment>>) -> Self {
        let frag_ptr = Arc::as_ptr(&fragments) as usize;
        let tree = build_stacking_context_tree(&fragments);
        // SAFETY: `_fragments` holds an Arc to the data `tree` borrows.
        // The explicit `Drop` impl drops `tree` before `_fragments`.
        // The Arc heap allocation is stable (won't move or deallocate)
        // for the struct's entire lifetime.
        let tree: StackingContext<'static> = unsafe { std::mem::transmute(tree) };
        Self {
            tree: ManuallyDrop::new(tree),
            _fragments: fragments,
            frag_ptr,
        }
    }

    /// Check if this cache is still valid for the given fragment Arc.
    pub fn is_valid_for(&self, fragments: &Arc<Vec<Fragment>>) -> bool {
        Arc::as_ptr(fragments) as usize == self.frag_ptr
    }

    /// Get a reference to the cached tree.
    pub(crate) fn tree(&self) -> &StackingContext<'_> {
        &self.tree
    }

    pub(crate) fn fragments(&self) -> &[Fragment] {
        self._fragments.as_slice()
    }
}
