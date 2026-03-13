use std::collections::HashMap;

use crate::clip_tree::{ClipId, ClipTree};
use crate::frame_tree::{FrameId, FrameKey, FrameKind, FrameTree};
use crate::stacking_context::{PaintItem, StackingContext, StackingContextContent, StackingContextSection};
use crate::transform::compute_css_reference_frame_matrix;
use havi_types::fragment_tree::BoxFragment;
use havi_types::{Fragment, IFrameFragment};
use makepad_widgets::*;
use style::computed_values::overflow_x::T as ComputedOverflow;
use style::computed_values::position::T as ComputedPosition;
use style::values::generics::box_::Perspective;
use style::values::generics::position::Inset;

#[derive(Clone, Copy)]
pub(crate) struct BuildContext {
    pub frame_id: FrameId,
    pub clip_id: ClipId,
    pub local_origin: DVec2,
    pub origin_basis: DVec2,
}

pub(crate) struct BuiltScene<'a> {
    pub frame_tree: FrameTree<'a>,
    pub clip_tree: ClipTree,
}

struct StackingContextBuildState<'a> {
    visual_cx: BuildContext,
    descendant_cx: BuildContext,
    owner_fragment: Option<&'a BoxFragment>,
    entry_frame_id: Option<FrameId>,
    descendant_frame_entry_id: Option<FrameId>,
    descendant_frame_entry_inserted: bool,
}

impl<'a> StackingContextBuildState<'a> {
    fn ensure_descendant_frame_entry(&mut self, frame_tree: &mut FrameTree<'a>) {
        let Some(frame_id) = self.descendant_frame_entry_id else {
            return;
        };
        if self.descendant_frame_entry_inserted {
            return;
        }
        frame_tree.append_child_frame(self.visual_cx.frame_id, frame_id);
        self.descendant_frame_entry_inserted = true;
    }
}

struct SceneBuilder<'tree, 'a> {
    frame_tree: &'tree mut FrameTree<'a>,
    clip_tree: &'tree mut ClipTree,
    scroll_state: &'tree crate::ScrollState,
    viewport_size: DVec2,
    fragment_origins: HashMap<usize, DVec2>,
    box_origins: HashMap<usize, DVec2>,
}

impl<'tree, 'a> SceneBuilder<'tree, 'a> {
    fn build_stacking_context_into_scene(&mut self, sc: &StackingContext<'a>, cx: BuildContext) {
        let mut scx = self.contexts_for_stacking_context(sc, cx);
        if let Some(frame_id) = scx.entry_frame_id {
            self.frame_tree.append_child_frame(cx.frame_id, frame_id);
        }
        sc.paint_in_order(&mut |item| self.build_paint_item_into_scene(item, &mut scx));
    }

    fn build_paint_item_into_scene(
        &mut self,
        item: PaintItem<'a, '_>,
        scx: &mut StackingContextBuildState<'a>,
    ) {
        match item {
            PaintItem::Content(content) => {
                let build_cx = if uses_visual_context(content, scx.owner_fragment) {
                    scx.visual_cx
                } else {
                    scx.ensure_descendant_frame_entry(self.frame_tree);
                    scx.descendant_cx
                };
                self.build_content_into_scene(content, build_cx);
            }
            PaintItem::ChildStackingContext(child) => {
                scx.ensure_descendant_frame_entry(self.frame_tree);
                self.build_stacking_context_into_scene(child, scx.descendant_cx);
            }
            PaintItem::Outline(_) => {}
        }
    }

    fn build_content_into_scene(&mut self, content: &StackingContextContent<'a>, cx: BuildContext) {
        match content {
            StackingContextContent::Fragment { section, fragment } => {
                let containing_block_origin = self
                    .fragment_origins
                    .get(&(std::ptr::from_ref(*fragment) as usize))
                    .copied()
                    .unwrap_or(dvec2(0.0, 0.0));
                let item_cx = BuildContext {
                    frame_id: cx.frame_id,
                    clip_id: cx.clip_id,
                    local_origin: dvec2(
                        cx.local_origin.x + containing_block_origin.x - cx.origin_basis.x,
                        cx.local_origin.y + containing_block_origin.y - cx.origin_basis.y,
                    ),
                    origin_basis: cx.origin_basis,
                };
                self.build_fragment_into_scene(fragment, *section, item_cx);
            }
            StackingContextContent::AtomicInlineStackingContainer { .. } => {}
        }
    }

    fn build_fragment_into_scene(
        &mut self,
        fragment: &'a Fragment,
        section: StackingContextSection,
        cx: BuildContext,
    ) {
        match fragment {
            Fragment::Box(_) | Fragment::Float(_) | Fragment::Text(_) | Fragment::Image(_) => {
                self.frame_tree
                    .push_item(cx.frame_id, fragment, section, cx.local_origin, cx.clip_id);
            }
            Fragment::IFrame(iframe) => {
                self.frame_tree
                    .push_item(cx.frame_id, fragment, section, cx.local_origin, cx.clip_id);
                self.build_iframe_into_scene(iframe, cx);
            }
            Fragment::Positioning(_) => {}
        }
    }

    fn build_iframe_into_scene(&mut self, iframe: &'a IFrameFragment, cx: BuildContext) {
        let key_id = frame_key_id_for_iframe(iframe);
        let iframe_origin = iframe_content_origin(iframe, cx.local_origin);
        let frame_id = self.frame_tree.push_child_frame(
            cx.frame_id,
            FrameKey::NodeIFrameRoot(key_id),
            FrameKind::IFrameRoot,
            iframe.base.tag.map(|tag| tag.node.0),
            translation_matrix(iframe_origin.x as f32, iframe_origin.y as f32),
        );
        let clip_id = self.clip_tree.push_rect(
            frame_id,
            cx.clip_id,
            Rect {
                pos: dvec2(0.0, 0.0),
                size: dvec2(
                    iframe.base.rect.size.width.to_f32_px() as f64,
                    iframe.base.rect.size.height.to_f32_px() as f64,
                ),
            },
        );
        self.frame_tree.set_clip(frame_id, clip_id);
        self.frame_tree.append_child_frame(cx.frame_id, frame_id);
        let child_sc = crate::stacking_context::build_stacking_context_tree(&iframe.child_fragments);
        self.build_stacking_context_into_scene(
            &child_sc,
            BuildContext {
                frame_id,
                clip_id,
                local_origin: dvec2(0.0, 0.0),
                origin_basis: dvec2(0.0, 0.0),
            },
        );
    }

    fn contexts_for_stacking_context(
        &mut self,
        sc: &StackingContext<'a>,
        cx: BuildContext,
    ) -> StackingContextBuildState<'a> {
        let Some(owner_fragment) = sc.initializing_fragment else {
            return StackingContextBuildState {
                visual_cx: cx,
                descendant_cx: cx,
                owner_fragment: None,
                entry_frame_id: None,
                descendant_frame_entry_id: None,
                descendant_frame_entry_inserted: false,
            };
        };

        let frame_key_id = frame_key_id_for_box(owner_fragment);
        let owner_node_id = owner_fragment.base.tag.map(|tag| tag.node.0);
        let mut visual = cx;
        let mut entry_frame_id = None;

        if let Some(owner_origin) = self.box_origins.get(&(std::ptr::from_ref(owner_fragment) as usize)).copied() {
            if let Some(spec) = fragment_reference_frame_spec(owner_fragment, owner_origin) {
                let frame_id = self.frame_tree.push_child_frame(
                    visual.frame_id,
                    FrameKey::NodeReferenceFrame(frame_key_id),
                    FrameKind::ReferenceFrame,
                    owner_node_id,
                    spec.matrix,
                );
                entry_frame_id = Some(frame_id);
                visual.frame_id = frame_id;
                if let Some(origin_basis) = spec.origin_basis {
                    visual.origin_basis = origin_basis;
                }
            }
        }

        if let Some(mat) = fragment_sticky_translation(owner_fragment, None, self.viewport_size) {
            let frame_id = self.frame_tree.push_child_frame(
                visual.frame_id,
                FrameKey::NodeStickyFrame(frame_key_id),
                FrameKind::StickyFrame,
                owner_node_id,
                mat,
            );
            entry_frame_id = entry_frame_id.or(Some(frame_id));
            visual.frame_id = frame_id;
        }

        let mut descendant = visual;
        if let Some(rect) = fragment_overflow_clip_rect(owner_fragment) {
            descendant.clip_id = self.clip_tree.push_rect(
                visual.frame_id,
                visual.clip_id,
                Rect {
                    pos: dvec2(visual.local_origin.x + rect.pos.x, visual.local_origin.y + rect.pos.y),
                    size: rect.size,
                },
            );
        }

        let mut descendant_frame_entry_id = None;
        if crate::is_scroll_container(owner_fragment) {
            let frame_id = self.frame_tree.push_child_frame(
                visual.frame_id,
                FrameKey::NodeScrollFrame(frame_key_id),
                FrameKind::ScrollFrame,
                owner_node_id,
                fragment_scroll_translation(owner_fragment, self.scroll_state)
                    .unwrap_or_else(Mat4f::identity),
            );
            self.frame_tree.set_clip(frame_id, descendant.clip_id);
            descendant.frame_id = frame_id;
            descendant_frame_entry_id = Some(frame_id);
        }

        StackingContextBuildState {
            visual_cx: visual,
            descendant_cx: descendant,
            owner_fragment: Some(owner_fragment),
            entry_frame_id,
            descendant_frame_entry_id,
            descendant_frame_entry_inserted: false,
        }
    }
}

pub(crate) fn build_scene<'a>(
    sc: &StackingContext<'a>,
    fragments: &'a [Fragment],
    scroll_state: &crate::ScrollState,
    root_origin: DVec2,
    viewport_size: DVec2,
) -> BuiltScene<'a> {
    let mut frame_tree = FrameTree::new();
    let mut clip_tree = ClipTree::new();
    let root_id = frame_tree.root_id();
    frame_tree.set_root_transform(translation_matrix(root_origin.x as f32, root_origin.y as f32));
    SceneBuilder {
        frame_tree: &mut frame_tree,
        clip_tree: &mut clip_tree,
        scroll_state,
        viewport_size,
        fragment_origins: build_fragment_origin_map(fragments),
        box_origins: build_box_origin_map(fragments),
    }
    .build_stacking_context_into_scene(
        sc,
        BuildContext {
            frame_id: root_id,
            clip_id: ClipId::INVALID,
            local_origin: dvec2(0.0, 0.0),
            origin_basis: dvec2(0.0, 0.0),
        },
    );
    BuiltScene { frame_tree, clip_tree }
}

fn build_fragment_origin_map(fragments: &[Fragment]) -> HashMap<usize, DVec2> {
    let mut origins = HashMap::new();
    for fragment in fragments {
        collect_fragment_origins(fragment, dvec2(0.0, 0.0), &mut origins, &mut HashMap::new());
    }
    origins
}

fn build_box_origin_map(fragments: &[Fragment]) -> HashMap<usize, DVec2> {
    let mut box_origins = HashMap::new();
    for fragment in fragments {
        collect_fragment_origins(fragment, dvec2(0.0, 0.0), &mut HashMap::new(), &mut box_origins);
    }
    box_origins
}

fn collect_fragment_origins(
    fragment: &Fragment,
    containing_block_origin: DVec2,
    origins: &mut HashMap<usize, DVec2>,
    box_origins: &mut HashMap<usize, DVec2>,
) {
    origins.insert(std::ptr::from_ref(fragment) as usize, containing_block_origin);
    match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => {
            box_origins.insert(std::ptr::from_ref(bf) as usize, containing_block_origin);
            let rect = bf.content_rect();
            let child_origin = dvec2(
                containing_block_origin.x + rect.origin.x.to_f32_px() as f64,
                containing_block_origin.y + rect.origin.y.to_f32_px() as f64,
            );
            for child in &bf.children {
                collect_fragment_origins(child, child_origin, origins, box_origins);
            }
        }
        Fragment::Positioning(pf) => {
            let rect = pf.base.rect;
            let child_origin = dvec2(
                containing_block_origin.x + rect.origin.x.to_f32_px() as f64,
                containing_block_origin.y + rect.origin.y.to_f32_px() as f64,
            );
            for child in &pf.children {
                collect_fragment_origins(child, child_origin, origins, box_origins);
            }
        }
        Fragment::IFrame(iframe) => {
            for child in iframe.child_fragments.iter() {
                collect_fragment_origins(child, dvec2(0.0, 0.0), origins, box_origins);
            }
        }
        Fragment::Text(_) | Fragment::Image(_) => {}
    }
}

fn uses_visual_context(
    content: &StackingContextContent<'_>,
    owner_fragment: Option<&BoxFragment>,
) -> bool {
    let Some(owner_fragment) = owner_fragment else {
        return false;
    };
    match content {
        StackingContextContent::Fragment { fragment, section } => match fragment {
            Fragment::Box(bf) | Fragment::Float(bf) => {
                std::ptr::eq(bf, owner_fragment)
                    && *section == StackingContextSection::OwnBackgroundsAndBorders
            }
            _ => false,
        },
        StackingContextContent::AtomicInlineStackingContainer { .. } => false,
    }
}

fn frame_key_id_for_box(bf: &BoxFragment) -> usize {
    bf.base
        .tag
        .map(|tag| tag.node.0)
        .unwrap_or(std::ptr::from_ref(bf) as usize)
}

fn frame_key_id_for_iframe(iframe: &IFrameFragment) -> usize {
    iframe
        .base
        .tag
        .map(|tag| tag.node.0)
        .unwrap_or(std::ptr::from_ref(iframe) as usize)
}

struct ReferenceFrameSpec {
    matrix: Mat4f,
    origin_basis: Option<DVec2>,
}

fn fragment_reference_frame_spec(bf: &BoxFragment, current_origin: DVec2) -> Option<ReferenceFrameSpec> {
    let border_rect = bf.border_rect();
    let bw = border_rect.size.width.to_f32_px();
    let bh = border_rect.size.height.to_f32_px();
    let style = &bf.base.style;
    let box_style = style.get_box();
    let has_transform = !box_style.transform.0.is_empty()
        || box_style.scale != style::values::generics::transform::GenericScale::None
        || box_style.rotate != style::values::generics::transform::GenericRotate::None
        || box_style.translate != style::values::generics::transform::GenericTranslate::None;
    let has_perspective = !matches!(box_style.perspective, Perspective::None);

    if !has_transform && has_perspective {
        return Some(ReferenceFrameSpec {
            matrix: Mat4f::identity(),
            origin_basis: None,
        });
    }

    let css_matrix = compute_css_reference_frame_matrix(style, bw, bh)?;
    let anchor = fragment_border_origin_absolute(bf, current_origin);
    Some(ReferenceFrameSpec {
        matrix: compose_reference_frame_transform(anchor, css_matrix),
        origin_basis: Some(anchor),
    })
}

fn fragment_border_origin_absolute(bf: &BoxFragment, current_origin: DVec2) -> DVec2 {
    let border_rect = bf.border_rect();
    dvec2(
        current_origin.x + border_rect.origin.x.to_f32_px() as f64,
        current_origin.y + border_rect.origin.y.to_f32_px() as f64,
    )
}

fn compose_reference_frame_transform(anchor: DVec2, transform: Mat4f) -> Mat4f {
    Mat4f::mul(&translation_matrix(anchor.x as f32, anchor.y as f32), &transform)
}

fn fragment_sticky_translation(
    bf: &BoxFragment,
    scroll_frame_size: Option<DVec2>,
    viewport_size: DVec2,
) -> Option<Mat4f> {
    if bf.base.style.get_box().position != ComputedPosition::Sticky {
        return None;
    }

    let basis_y = scroll_frame_size.unwrap_or(viewport_size).y as f32;
    let position = bf.base.style.get_position();
    let dy = match (&position.top, &position.bottom) {
        (Inset::LengthPercentage(lp), _) => {
            lp.to_used_value(app_units::Au::from_f32_px(basis_y)).to_f32_px()
        }
        (_, Inset::LengthPercentage(lp)) => {
            -lp.to_used_value(app_units::Au::from_f32_px(basis_y)).to_f32_px()
        }
        _ => 0.0,
    };

    if dy.abs() < 0.001 {
        None
    } else {
        Some(translation_matrix(0.0, dy))
    }
}

fn fragment_scroll_translation(
    bf: &BoxFragment,
    scroll_state: &crate::ScrollState,
) -> Option<Mat4f> {
    let node_id = bf.base.tag.map(|tag| tag.node.0)?;
    let offset = scroll_state.get(&node_id).copied().unwrap_or(dvec2(0.0, 0.0));
    Some(translation_matrix(-(offset.x as f32), -(offset.y as f32)))
}

fn fragment_overflow_clip_rect(bf: &BoxFragment) -> Option<Rect> {
    let overflow = bf.base.style.get_box();
    if matches!(overflow.overflow_x, ComputedOverflow::Visible)
        && matches!(overflow.overflow_y, ComputedOverflow::Visible)
    {
        return None;
    }

    let padding_rect = bf.padding_rect();
    Some(Rect {
        pos: dvec2(
            padding_rect.origin.x.to_f32_px() as f64,
            padding_rect.origin.y.to_f32_px() as f64,
        ),
        size: dvec2(
            padding_rect.size.width.to_f32_px() as f64,
            padding_rect.size.height.to_f32_px() as f64,
        ),
    })
}

fn iframe_content_origin(iframe: &IFrameFragment, current_origin: DVec2) -> DVec2 {
    let rect = iframe.base.rect;
    dvec2(
        current_origin.x + rect.origin.x.to_f32_px() as f64,
        current_origin.y + rect.origin.y.to_f32_px() as f64,
    )
}

fn fragment_iframe_clip_rect(iframe: &IFrameFragment, current_origin: DVec2) -> Rect {
    let rect = iframe.base.rect;
    Rect {
        pos: dvec2(
            current_origin.x + rect.origin.x.to_f32_px() as f64,
            current_origin.y + rect.origin.y.to_f32_px() as f64,
        ),
        size: dvec2(
            rect.size.width.to_f32_px() as f64,
            rect.size.height.to_f32_px() as f64,
        ),
    }
}

fn translation_matrix(tx: f32, ty: f32) -> Mat4f {
    Mat4f {
        v: [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            tx, ty, 0.0, 1.0,
        ],
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use havi_types::fragment_tree::{BaseFragment, BaseFragmentInfo, Baselines, BoxFragment};
    use havi_types::geom::{PhysicalRect, PhysicalSides};
    use havi_types::OpaqueNode;
    use style::properties::ComputedValues;
    use style::properties::generated::style_structs::Font;

    fn make_rect(x: f32, y: f32, w: f32, h: f32) -> PhysicalRect<app_units::Au> {
        use app_units::Au;
        use style_traits::CSSPixel;
        PhysicalRect::new(
            euclid::Point2D::<Au, CSSPixel>::new(Au::from_f32_px(x), Au::from_f32_px(y)),
            euclid::Size2D::<Au, CSSPixel>::new(Au::from_f32_px(w), Au::from_f32_px(h)),
        )
    }

    fn initial_style() -> servo_arc::Arc<ComputedValues> {
        ComputedValues::initial_values_with_font_override(Font::initial_values()).to_arc()
    }

    fn scroll_box(node_id: usize) -> Fragment {
        use app_units::Au;
        use style::values::specified::Overflow;

        let mut style = ComputedValues::initial_values_with_font_override(Font::initial_values());
        servo_arc::Arc::make_mut(&mut style).mutate_box().set_overflow_x(Overflow::Auto);
        servo_arc::Arc::make_mut(&mut style).mutate_box().set_overflow_y(Overflow::Auto);

        let sides = PhysicalSides::new(Au(0), Au(0), Au(0), Au(0));
        Fragment::Box(BoxFragment {
            base: BaseFragment::new(
                BaseFragmentInfo::new(OpaqueNode(node_id)),
                style.to_arc(),
                make_rect(0.0, 0.0, 100.0, 100.0),
            ),
            children: Vec::new(),
            padding: sides,
            border: sides,
            margin: sides,
            baselines: Baselines::default(),
            block_level_info: None,
            background_images: Vec::new(),
        })
    }

    fn plain_box(node_id: usize, x: f32, y: f32, children: Vec<Fragment>) -> Fragment {
        use app_units::Au;
        let sides = PhysicalSides::new(Au(0), Au(0), Au(0), Au(0));
        Fragment::Box(BoxFragment {
            base: BaseFragment::new(
                BaseFragmentInfo::new(OpaqueNode(node_id)),
                initial_style(),
                make_rect(x, y, 100.0, 100.0),
            ),
            children,
            padding: sides,
            border: sides,
            margin: sides,
            baselines: Baselines::default(),
            block_level_info: None,
            background_images: Vec::new(),
        })
    }

    #[test]
    fn build_scene_creates_scroll_frame_for_overflow_container() {
        let fragment = scroll_box(7);
        let fragments = [fragment];
        let sc = crate::stacking_context::build_stacking_context_tree(&fragments);
        let scene = build_scene(
            &sc,
            &fragments,
            &[(7usize, dvec2(12.0, 13.0))].into_iter().collect(),
            dvec2(50.0, 60.0),
            dvec2(800.0, 600.0),
        );
        let scroll_frame = scene
            .frame_tree
            .frames
            .iter()
            .find(|frame| frame.kind == FrameKind::ScrollFrame && frame.owner_node_id == Some(7))
            .unwrap();
        assert_eq!(scene.frame_tree.frame(scene.frame_tree.root).items[0].local_origin, dvec2(0.0, 0.0));
        assert_eq!(scroll_frame.clip_id, ClipId(0));
    }

    #[test]
    fn iframe_child_fragments_get_nested_origins() {
        let child_fragments = Arc::new(vec![plain_box(2, 5.0, 6.0, vec![plain_box(3, 7.0, 8.0, Vec::new())])]);
        let iframe = Fragment::IFrame(IFrameFragment {
            base: BaseFragment::new(
                BaseFragmentInfo::new(OpaqueNode(1)),
                initial_style(),
                make_rect(30.0, 40.0, 200.0, 150.0),
            ),
            child_fragments,
            child_content_height: 150.0,
        });
        let fragments = [iframe];
        let sc = crate::stacking_context::build_stacking_context_tree(&fragments);
        let scene = build_scene(
            &sc,
            &fragments,
            &crate::ScrollState::default(),
            dvec2(0.0, 0.0),
            dvec2(800.0, 600.0),
        );
        let iframe_frame = scene
            .frame_tree
            .frames
            .iter()
            .find(|frame| frame.kind == FrameKind::IFrameRoot)
            .unwrap();
        assert!(iframe_frame.items.iter().any(|item| item.local_origin == dvec2(0.0, 0.0)));
        assert!(iframe_frame.items.iter().any(|item| item.local_origin == dvec2(5.0, 6.0)));
    }

    #[test]
    fn reference_frame_transform_translates_local_space_to_anchor() {
        let mat = compose_reference_frame_transform(dvec2(100.0, 0.0), translation_matrix(10.0, 0.0));
        let mapped = mat.transform_vec4(vec4f(0.0, 0.0, 0.0, 1.0));
        assert_eq!(mapped.x, 110.0);
    }
}
