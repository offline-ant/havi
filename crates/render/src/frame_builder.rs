use crate::clip_tree::{ClipId, ClipTree};
use crate::frame_tree::{FrameId, FrameKey, FrameKind, FrameTree};
use crate::stacking_context::{PaintItem, StackingContext, StackingContextContent, StackingContextSection};
use crate::transform::compute_css_reference_frame_matrix;
use havi_types::fragment_tree::BoxFragment;
use havi_types::{Fragment, IFrameFragment};
use makepad_widgets::*;
use style::computed_values::position::T as ComputedPosition;
use style::values::generics::position::Inset;

#[derive(Clone, Copy)]
pub(crate) struct BuildContext {
    pub frame_id: FrameId,
    pub clip_id: ClipId,
    pub local_origin: DVec2,
}

pub(crate) struct BuiltScene<'a> {
    pub frame_tree: FrameTree<'a>,
    pub clip_tree: ClipTree,
}

struct SceneBuilder<'tree, 'a> {
    frame_tree: &'tree mut FrameTree<'a>,
    clip_tree: &'tree mut ClipTree,
    scroll_state: &'tree crate::ScrollState,
    viewport_size: DVec2,
}

impl<'tree, 'a> SceneBuilder<'tree, 'a> {
    fn build_stacking_context_into_scene(&mut self, sc: &StackingContext<'a>, cx: BuildContext) {
        let (visual_cx, descendant_cx, owner_fragment) = self.contexts_for_stacking_context(sc, cx);
        sc.paint_in_order(&mut |item| self.build_paint_item_into_scene(item, visual_cx, descendant_cx, owner_fragment));
    }

    fn build_paint_item_into_scene(
        &mut self,
        item: PaintItem<'a, '_>,
        visual_cx: BuildContext,
        descendant_cx: BuildContext,
        owner_fragment: Option<&'a BoxFragment>,
    ) {
        match item {
            PaintItem::Content(content) => {
                let build_cx = if uses_visual_context(content, owner_fragment) {
                    visual_cx
                } else {
                    descendant_cx
                };
                self.build_content_into_scene(content, build_cx);
            }
            PaintItem::ChildStackingContext(child) => {
                self.build_stacking_context_into_scene(child, descendant_cx);
            }
            PaintItem::Outline(_) => {}
        }
    }

    fn build_content_into_scene(&mut self, content: &StackingContextContent<'a>, cx: BuildContext) {
        match content {
            StackingContextContent::Fragment {
                section,
                fragment,
                containing_block_origin,
            } => {
                let item_cx = BuildContext {
                    frame_id: cx.frame_id,
                    clip_id: cx.clip_id,
                    local_origin: dvec2(
                        cx.local_origin.x + containing_block_origin.0,
                        cx.local_origin.y + containing_block_origin.1,
                    ),
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
                self.frame_tree.push_item(cx.frame_id, fragment, section, cx.local_origin, cx.clip_id);
            }
            Fragment::IFrame(iframe) => {
                self.frame_tree.push_item(cx.frame_id, fragment, section, cx.local_origin, cx.clip_id);
                self.build_iframe_into_scene(iframe, cx);
            }
            Fragment::Positioning(_) => {}
        }
    }

    fn build_iframe_into_scene(&mut self, iframe: &'a IFrameFragment, cx: BuildContext) {
        let node_id = iframe.base.tag.map(|tag| tag.node.0);
        let clip_id = self.clip_tree.push_rect(
            cx.frame_id,
            cx.clip_id,
            fragment_iframe_clip_rect(iframe, cx.local_origin),
        );
        let frame_id = self.frame_tree.push_child_frame(
            cx.frame_id,
            FrameKey::NodeIFrameRoot(node_id.unwrap_or(0)),
            FrameKind::IFrameRoot,
            node_id,
            Mat4f::identity(),
        );
        let child_sc = crate::stacking_context::build_stacking_context_tree(&iframe.child_fragments);
        self.build_stacking_context_into_scene(
            &child_sc,
            BuildContext {
                frame_id,
                clip_id,
                local_origin: iframe_content_origin(iframe, cx.local_origin),
            },
        );
    }

    fn contexts_for_stacking_context(
        &mut self,
        sc: &StackingContext<'a>,
        cx: BuildContext,
    ) -> (BuildContext, BuildContext, Option<&'a BoxFragment>) {
        let Some(owner_fragment) = sc.initializing_fragment else {
            return (cx, cx, None);
        };

        let node_id = owner_fragment.base.tag.map(|tag| tag.node.0);
        let mut visual = cx;

        if let Some(mat) = fragment_reference_frame_matrix(owner_fragment) {
            visual.frame_id = self.frame_tree.push_child_frame(
                visual.frame_id,
                FrameKey::NodeReferenceFrame(node_id.unwrap_or(0)),
                FrameKind::ReferenceFrame,
                node_id,
                mat,
            );
        }

        if let Some(mat) = fragment_sticky_translation(owner_fragment, None, self.viewport_size) {
            visual.frame_id = self.frame_tree.push_child_frame(
                visual.frame_id,
                FrameKey::NodeStickyFrame(node_id.unwrap_or(0)),
                FrameKind::StickyFrame,
                node_id,
                mat,
            );
        }

        let mut descendant = visual;
        if crate::is_scroll_container(owner_fragment) {
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
            descendant.frame_id = self.frame_tree.push_child_frame(
                visual.frame_id,
                FrameKey::NodeScrollFrame(node_id.unwrap_or(0)),
                FrameKind::ScrollFrame,
                node_id,
                fragment_scroll_translation(owner_fragment, self.scroll_state).unwrap_or_else(Mat4f::identity),
            );
        }

        (visual, descendant, Some(owner_fragment))
    }
}

pub(crate) fn build_scene<'a>(
    sc: &StackingContext<'a>,
    scroll_state: &crate::ScrollState,
    root_origin: DVec2,
    viewport_size: DVec2,
) -> BuiltScene<'a> {
    let mut frame_tree = FrameTree::new();
    let mut clip_tree = ClipTree::new();
    let root_id = frame_tree.root_id();
    SceneBuilder {
        frame_tree: &mut frame_tree,
        clip_tree: &mut clip_tree,
        scroll_state,
        viewport_size,
    }
    .build_stacking_context_into_scene(
        sc,
        BuildContext {
            frame_id: root_id,
            clip_id: ClipId::INVALID,
            local_origin: root_origin,
        },
    );
    BuiltScene { frame_tree, clip_tree }
}

fn uses_visual_context(
    content: &StackingContextContent<'_>,
    owner_fragment: Option<&BoxFragment>,
) -> bool {
    let Some(owner_fragment) = owner_fragment else {
        return false;
    };
    match content {
        StackingContextContent::Fragment { fragment, section, .. } => match fragment {
            Fragment::Box(bf) | Fragment::Float(bf) => {
                std::ptr::eq(bf, owner_fragment)
                    && *section == StackingContextSection::OwnBackgroundsAndBorders
            }
            _ => false,
        },
        StackingContextContent::AtomicInlineStackingContainer { .. } => false,
    }
}

fn fragment_reference_frame_matrix(bf: &BoxFragment) -> Option<Mat4f> {
    let border_rect = bf.border_rect();
    let bw = border_rect.size.width.to_f32_px();
    let bh = border_rect.size.height.to_f32_px();
    compute_css_reference_frame_matrix(&bf.base.style, bw, bh)
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

    #[test]
    fn build_scene_creates_scroll_frame_for_overflow_container() {
        let fragment = scroll_box(7);
        let fragments = [fragment];
        let sc = crate::stacking_context::build_stacking_context_tree(&fragments);
        let scene = build_scene(
            &sc,
            &[(7usize, dvec2(12.0, 13.0))].into_iter().collect(),
            dvec2(50.0, 60.0),
            dvec2(800.0, 600.0),
        );
        assert!(scene.frame_tree.frames.iter().any(|frame| {
            frame.kind == FrameKind::ScrollFrame && frame.owner_node_id == Some(7)
        }));
        assert_eq!(scene.frame_tree.frame(scene.frame_tree.root).items[0].local_origin, dvec2(50.0, 60.0));
    }
}
