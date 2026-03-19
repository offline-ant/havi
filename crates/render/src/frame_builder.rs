use crate::clip_tree::{ClipId, ClipTree};
use crate::compositor_scene::CompositorScene;
use crate::frame_tree::{FrameId, FrameKey, FrameKind, FrameTree};
use crate::layout_stacking_context::{
    build_stacking_context_tree, LayoutPaintItem, LayoutStackingContext,
    LayoutStackingContextContent, StackingContextSection,
};
use crate::paint_items::PaintSource;
use crate::reference_frame::reference_frame_matrix;
use crate::render_plan::{collect_owner_render_semantics, NodeRenderSemantics, RenderPlan};
use havi_types::fragment_tree::BoxFragment;
use havi_types::{Fragment, IFrameFragment};
use makepad_widgets::*;

#[derive(Clone, Copy)]
pub(crate) struct BuildContext {
    pub frame_id: FrameId,
    pub clip_id: ClipId,
    pub local_origin: DVec2,
}

pub(crate) struct BuiltScene<'a> {
    pub frame_tree: FrameTree<'a>,
    pub clip_tree: ClipTree,
    pub render_plan: RenderPlan,
    pub compositor_scene: CompositorScene,
}

struct SceneBuilder<'tree, 'a> {
    frame_tree: &'tree mut FrameTree<'a>,
    clip_tree: &'tree mut ClipTree,
    scroll_state: &'tree crate::ScrollState,
    owner_semantics: &'tree std::collections::HashMap<usize, NodeRenderSemantics>,
}

impl<'tree, 'a> SceneBuilder<'tree, 'a> {
    fn build_stacking_context_into_scene(&mut self, sc: &LayoutStackingContext<'a>, cx: BuildContext) {
        sc.paint_in_order(&mut |item| self.build_paint_item_into_scene(item, cx));
    }

    fn build_paint_item_into_scene(&mut self, item: LayoutPaintItem<'a, '_>, cx: BuildContext) {
        match item {
            LayoutPaintItem::Content(content) => self.build_content_into_scene(content, cx),
            LayoutPaintItem::ChildStackingContext(child) => self.build_stacking_context_into_scene(child, cx),
        }
    }

    fn build_content_into_scene(&mut self, content: &LayoutStackingContextContent<'a>, cx: BuildContext) {
        match content {
            LayoutStackingContextContent::Fragment {
                section,
                fragment,
                frame_id,
                clip_id,
                containing_block,
                ..
            } => {
                let item_cx = BuildContext {
                    frame_id: *frame_id,
                    clip_id: *clip_id,
                    local_origin: cx.local_origin
                        + dvec2(
                            containing_block.origin.x.to_f32_px() as f64,
                            containing_block.origin.y.to_f32_px() as f64,
                        ),
                };
                self.ensure_fragment_spatial_context(*fragment, item_cx);
                self.build_fragment_into_scene(fragment, *section, item_cx);
            }
            LayoutStackingContextContent::AtomicInlineStackingContainer { .. } => {}
        }
    }

    fn ensure_fragment_spatial_context(&mut self, fragment: &'a Fragment, cx: BuildContext) {
        let Some(bf) = fragment_box(fragment) else {
            return;
        };

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
        let frame_key_id = frame_key_id_for_box(bf);

        if let Some(flatten_3d) = owner_node_id
            .and_then(|node_id| self.owner_semantics.get(&node_id).copied())
            .map(|semantics| !semantics.requires_compositor())
        {
            if let Some(matrix) = reference_frame_matrix(
                bf,
                dvec2(
                    bf.cumulative_containing_block_rect.origin.x.to_f32_px() as f64,
                    bf.cumulative_containing_block_rect.origin.y.to_f32_px() as f64,
                ),
                flatten_3d,
            ) {
                let frame_id = self.frame_tree.push_child_frame(
                    cx.frame_id,
                    FrameKey::NodeReferenceFrame(frame_key_id),
                    FrameKind::ReferenceFrame,
                    owner_node_id,
                    matrix,
                );
                self.frame_tree.append_child_frame(cx.frame_id, frame_id);
            }
        }

        if let Some(rect) = bf.scrollable_overflow {
            let clip_id = self.clip_tree.push_rect(
                cx.frame_id,
                cx.clip_id,
                Rect {
                    pos: dvec2(
                        cx.local_origin.x + rect.origin.x.to_f32_px() as f64,
                        cx.local_origin.y + rect.origin.y.to_f32_px() as f64,
                    ),
                    size: dvec2(
                        rect.size.width.to_f32_px() as f64,
                        rect.size.height.to_f32_px() as f64,
                    ),
                },
            );
            let frame_id = self.frame_tree.push_child_frame(
                cx.frame_id,
                FrameKey::NodeScrollFrame(frame_key_id),
                FrameKind::ScrollFrame,
                owner_node_id,
                fragment_scroll_translation(bf, self.scroll_state).unwrap_or_else(Mat4f::identity),
            );
            self.frame_tree.set_clip(frame_id, clip_id);
            self.frame_tree.append_child_frame(cx.frame_id, frame_id);
        }

        if let Some(mat) = fragment_sticky_translation(bf) {
            let frame_id = self.frame_tree.push_child_frame(
                cx.frame_id,
                FrameKey::NodeStickyFrame(frame_key_id),
                FrameKind::StickyFrame,
                owner_node_id,
                mat,
            );
            self.frame_tree.append_child_frame(cx.frame_id, frame_id);
        }
    }

    fn build_fragment_into_scene(
        &mut self,
        source: PaintSource<'a>,
        section: StackingContextSection,
        cx: BuildContext,
    ) {
        match source {
            Fragment::Box(_) | Fragment::Float(_) | Fragment::Text(_) | Fragment::Image(_) => {
                self.frame_tree
                    .push_item(cx.frame_id, source, section, cx.local_origin, cx.clip_id);
            }
            Fragment::IFrame(iframe) => {
                self.frame_tree
                    .push_item(cx.frame_id, source, section, cx.local_origin, cx.clip_id);
                self.build_iframe_into_scene(iframe, cx);
            }
            Fragment::Positioning(_) | Fragment::AbsoluteOrFixedPositioned { .. } => {}
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
        let child_sc = build_stacking_context_tree(&iframe.child_fragments, frame_id, clip_id);
        self.build_stacking_context_into_scene(
            &child_sc,
            BuildContext {
                frame_id,
                clip_id,
                local_origin: dvec2(0.0, 0.0),
            },
        );
    }
}

pub(crate) fn build_scene<'a>(
    sc: &LayoutStackingContext<'a>,
    fragments: &'a [Fragment],
    scroll_state: &crate::ScrollState,
    scroll_origin: DVec2,
    _viewport_size: DVec2,
) -> BuiltScene<'a> {
    let mut frame_tree = FrameTree::new();
    let mut clip_tree = ClipTree::new();
    let root_id = frame_tree.root_id();
    let owner_semantics = collect_owner_render_semantics(fragments);
    SceneBuilder {
        frame_tree: &mut frame_tree,
        clip_tree: &mut clip_tree,
        scroll_state,
        owner_semantics: &owner_semantics,
    }
    .build_stacking_context_into_scene(
        sc,
        BuildContext {
            frame_id: root_id,
            clip_id: ClipId::INVALID,
            local_origin: scroll_origin,
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

fn fragment_box(fragment: &Fragment) -> Option<&BoxFragment> {
    match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => Some(bf),
        _ => None,
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

fn fragment_sticky_translation(bf: &BoxFragment) -> Option<Mat4f> {
    let insets = bf.resolved_sticky_insets?;
    let dy = match (insets.top, insets.bottom) {
        (havi_types::AuOrAuto::LengthPercentage(v), _) => v.to_f32_px(),
        (_, havi_types::AuOrAuto::LengthPercentage(v)) => -v.to_f32_px(),
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
