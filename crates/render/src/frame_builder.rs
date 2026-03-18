use std::collections::HashMap;

use crate::clip_tree::{ClipId, ClipTree};
use crate::frame_tree::{FrameId, FrameKey, FrameKind, FrameTree};
use crate::compositor_scene::CompositorScene;
use crate::reference_frame::reference_frame_spec;
use crate::render_plan::{collect_owner_render_semantics, NodeRenderSemantics, RenderPlan};
use crate::stacking_context::{PaintItem, StackingContext, StackingContextContent, StackingContextSection};
use havi_types::fragment_tree::BoxFragment;
use havi_types::{Fragment, IFrameFragment};
use makepad_widgets::*;
use style::computed_values::overflow_x::T as ComputedOverflow;
use style::computed_values::position::T as ComputedPosition;
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
    pub render_plan: RenderPlan,
    pub compositor_scene: CompositorScene,
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
    owner_semantics: &'tree HashMap<usize, NodeRenderSemantics>,
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
            PaintItem::Outline => {}
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
            let flatten_3d = owner_node_id
                .and_then(|node_id| self.owner_semantics.get(&node_id).copied())
                .map(|semantics| !semantics.requires_compositor())
                .unwrap_or(true);
            if let Some(spec) = reference_frame_spec(owner_fragment, owner_origin, flatten_3d) {
                let frame_id = self.frame_tree.push_child_frame(
                    visual.frame_id,
                    FrameKey::NodeReferenceFrame(frame_key_id),
                    FrameKind::ReferenceFrame,
                    owner_node_id,
                    spec.matrix,
                );
                entry_frame_id = Some(frame_id);
                visual.frame_id = frame_id;
                match spec.mode {
                    crate::reference_frame::ReferenceFrameMode::AnchoredTransform => {}
                    crate::reference_frame::ReferenceFrameMode::PerspectiveOnlyIsolation => {}
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
    scroll_origin: DVec2,
    viewport_size: DVec2,
) -> BuiltScene<'a> {
    let mut frame_tree = FrameTree::new();
    let mut clip_tree = ClipTree::new();
    let root_id = frame_tree.root_id();
    let owner_semantics = collect_owner_render_semantics(fragments);
    // scroll_origin carries the widget's window position plus the scroll
    // offset so that page-relative item positions map to the correct window
    // coordinates.
    SceneBuilder {
        frame_tree: &mut frame_tree,
        clip_tree: &mut clip_tree,
        scroll_state,
        viewport_size,
        fragment_origins: build_fragment_origin_map(fragments),
        box_origins: build_box_origin_map(fragments),
        owner_semantics: &owner_semantics,
    }
    .build_stacking_context_into_scene(
        sc,
        BuildContext {
            frame_id: root_id,
            clip_id: ClipId::INVALID,
            local_origin: scroll_origin,
            origin_basis: dvec2(0.0, 0.0),
        },
    );
    let render_plan = RenderPlan::build(&frame_tree, owner_semantics);
    let compositor_scene = CompositorScene::build(&frame_tree, &render_plan);
    BuiltScene {
        frame_tree,
        clip_tree,
        render_plan,
        compositor_scene,
    }
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
            dvec2(0.0, 0.0),
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

    fn translated_box(node_id: usize, x: f32, y: f32, tx: f32, ty: f32) -> Fragment {
        use app_units::Au;
        use style::values::computed::length::Length;
        use style::values::computed::LengthPercentage;
        use style::values::generics::transform::GenericTranslate;

        let mut style = ComputedValues::initial_values_with_font_override(Font::initial_values());
        servo_arc::Arc::make_mut(&mut style)
            .mutate_box()
            .set_translate(GenericTranslate::Translate(
                LengthPercentage::new_length(Length::new(tx)),
                LengthPercentage::new_length(Length::new(ty)),
                Length::new(0.0),
            ));

        let sides = PhysicalSides::new(Au(0), Au(0), Au(0), Au(0));
        Fragment::Box(BoxFragment {
            base: BaseFragment::new(
                BaseFragmentInfo::new(OpaqueNode(node_id)),
                style.to_arc(),
                make_rect(x, y, 100.0, 100.0),
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

    /// Verify that a CSS translate creates a reference frame whose world
    /// matrix correctly positions items in page space.
    ///
    /// Layout: body box at rect=(8,8 1264x100), child at rect=(0,0 100x100)
    /// with translate: 100px 100px. The child's containing-block origin is
    /// (8,8) from body content_rect. CSS translate(100,100) anchored at (8,8)
    /// composes to T(100,100). Items have local_origin=(8,8) in page space.
    /// Drawing position: world*(local_origin) = T(100,100)*(8,8) = (108,108)
    /// in page space; scroll_origin shifts that to window space.
    #[test]
    fn translate_reference_frame_world_matches_static_offset() {
        use app_units::Au;
        let child = translated_box(2, 0.0, 0.0, 100.0, 100.0);
        // Body-like parent: content_rect at (8,8) via rect origin.
        let sides = PhysicalSides::new(Au(0), Au(0), Au(0), Au(0));
        let parent = Fragment::Box(BoxFragment {
            base: BaseFragment::new(
                BaseFragmentInfo::new(OpaqueNode(1)),
                initial_style(),
                make_rect(8.0, 8.0, 1264.0, 100.0),
            ),
            children: vec![child],
            padding: sides,
            border: sides,
            margin: sides,
            baselines: Baselines::default(),
            block_level_info: None,
            background_images: Vec::new(),
        });

        let fragments = [parent];
        let sc = crate::stacking_context::build_stacking_context_tree(&fragments);
        let scene = build_scene(
            &sc,
            &fragments,
            &crate::ScrollState::default(),
            dvec2(0.0, 0.0),
            dvec2(1280.0, 800.0),
        );

        // Find the reference frame for node 2
        let ref_frame = scene
            .frame_tree
            .frames
            .iter()
            .find(|f| f.kind == FrameKind::ReferenceFrame && f.owner_node_id == Some(2))
            .expect("should have a reference frame for the translated box");

        // scroll_origin=(0,0), so local_origin = containing_block_origin = (8,8).
        // anchor = border_origin_absolute(div, (8,8)) = (8,8) in page space.
        // compose_reference_frame_transform(anchor=(8,8), T(100,100)):
        //   T(8,8) * T(100,100) * T(-8,-8) = T(100,100).
        // ref_frame.matrix.world = identity * T(100,100) = T(100,100).
        // Page-space draw position = T(100,100) * (8,8) = (108,108).
        let item = ref_frame.items.first().expect("frame should have items");
        let draw_pos = ref_frame.matrix.world.transform_vec4(
            vec4f(item.local_origin.x as f32, item.local_origin.y as f32, 0.0, 1.0),
        );

        assert!(
            (draw_pos.x - 108.0).abs() < 0.5 && (draw_pos.y - 108.0).abs() < 0.5,
            "world*(local_origin) = ({:.1},{:.1}) should be (108,108), local_origin=({:.1},{:.1})",
            draw_pos.x, draw_pos.y,
            item.local_origin.x, item.local_origin.y
        );
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

}
