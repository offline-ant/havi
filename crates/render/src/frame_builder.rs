use crate::frame_tree::FrameKey;
use crate::layout_stacking_context::{
    build_stacking_context_tree, LayoutPaintItem, LayoutStackingContext,
    LayoutStackingContextContent, SpatialAttachment, StackingContextSection,
};
use crate::paint_items::PaintSource;
use crate::render_plan::collect_owner_render_semantics;
use crate::scene::{ReferenceFrameData, RenderScene, SceneClipId, SpatialNodeSemantics};
use crate::scene_builder::RenderSceneBuilder;
use havi_fragment_semantics::{Fragment, IFrameFragment};
use makepad_widgets::*;

#[derive(Clone, Copy)]
pub(crate) struct BuildContext {
    pub attachment: SpatialAttachment,
    pub local_origin: DVec2,
}

pub(crate) type BuiltScene<'a> = RenderScene<'a>;

struct PaintListBuilder<'tree, 'a> {
    scene_builder: &'tree mut RenderSceneBuilder<'a>,
}

impl<'tree, 'a> PaintListBuilder<'tree, 'a> {
    fn build_stacking_context_into_scene(&mut self, sc: &LayoutStackingContext<'a>, cx: BuildContext) {
        sc.paint_in_order(&mut |item| self.build_paint_item_into_scene(item, cx));
    }

    fn build_paint_item_into_scene(&mut self, item: LayoutPaintItem<'a, '_>, cx: BuildContext) {
        match item {
            LayoutPaintItem::Content(content) => self.build_content_into_scene(content, cx, None),
            LayoutPaintItem::Outline(content) => self.build_content_into_scene(
                content,
                cx,
                Some(StackingContextSection::Outline),
            ),
            LayoutPaintItem::ChildStackingContext(child) => self.build_stacking_context_into_scene(child, cx),
        }
    }

    fn build_content_into_scene(
        &mut self,
        content: &LayoutStackingContextContent<'a>,
        cx: BuildContext,
        section_override: Option<StackingContextSection>,
    ) {
        match content {
            LayoutStackingContextContent::Fragment {
                section,
                fragment,
                attachment,
                containing_block,
            } => {
                let item_cx = BuildContext {
                    attachment: *attachment,
                    local_origin: cx.local_origin
                        + dvec2(
                            containing_block.origin.x.to_f32_px() as f64,
                            containing_block.origin.y.to_f32_px() as f64,
                        ),
                };
                self.build_fragment_into_scene(fragment, section_override.unwrap_or(*section), item_cx);
            }
            LayoutStackingContextContent::AtomicInlineStackingContainer { .. } => {}
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
                self.scene_builder.push_item(
                    cx.attachment.paint_container_id,
                    source,
                    section,
                    cx.local_origin,
                    cx.attachment.clip_id,
                );
            }
            Fragment::IFrame(iframe) => {
                self.scene_builder.push_item(
                    cx.attachment.paint_container_id,
                    source,
                    section,
                    cx.local_origin,
                    cx.attachment.clip_id,
                );
                self.build_iframe_into_scene(iframe, cx);
            }
            Fragment::Positioning(_) | Fragment::AbsoluteOrFixedPositioned { .. } => {}
        }
    }

    fn build_iframe_into_scene(&mut self, iframe: &'a IFrameFragment, cx: BuildContext) {
        let key_id = frame_key_id_for_iframe(iframe);
        let iframe_origin = iframe_content_origin(iframe, cx.local_origin);
        let spatial_node_id = self.scene_builder.child_iframe_root_node(
            cx.attachment.spatial_node_id,
            iframe.base.tag.map(|tag| tag.node.0),
        );
        let spatial_node_id = self.scene_builder.child_spatial_node(
            spatial_node_id,
            SpatialNodeSemantics::ReferenceFrame(ReferenceFrameData {
                origin: iframe_origin,
                transform_matrix: Some(translation_matrix(iframe_origin.x as f32, iframe_origin.y as f32)),
                perspective_matrix: None,
                has_transform: true,
                has_perspective: false,
                preserves_3d: false,
                anchors_content: true,
            }),
            iframe.base.tag.map(|tag| tag.node.0),
        );
        let paint_container_id = self.scene_builder.child_paint_container(
            cx.attachment.paint_container_id,
            spatial_node_id,
            FrameKey::NodeIFrameRoot(key_id),
            iframe.base.tag.map(|tag| tag.node.0),
        );
        let clip_id = self.scene_builder.rect_clip(
            paint_container_id,
            cx.attachment.clip_id,
            Rect {
                pos: dvec2(0.0, 0.0),
                size: dvec2(
                    iframe.base.rect.size.width.to_f32_px() as f64,
                    iframe.base.rect.size.height.to_f32_px() as f64,
                ),
            },
        );
        self.scene_builder.set_frame_clip(paint_container_id, clip_id);
        let child_owner_semantics = collect_owner_render_semantics(&iframe.child_fragments);
        let child_sc = build_stacking_context_tree(
            &iframe.child_fragments,
            self.scene_builder,
            paint_container_id,
            clip_id,
            &crate::ScrollState::default(),
            &child_owner_semantics,
        );
        self.build_stacking_context_into_scene(
            &child_sc,
            BuildContext {
                attachment: SpatialAttachment {
                    paint_container_id,
                    spatial_node_id,
                    clip_id,
                },
                local_origin: dvec2(0.0, 0.0),
            },
        );
    }
}

pub(crate) fn build_scene<'a>(
    fragments: &'a [Fragment],
    scroll_state: &crate::ScrollState,
    scroll_origin: DVec2,
    _viewport_size: DVec2,
) -> BuiltScene<'a> {
    let owner_semantics = collect_owner_render_semantics(fragments);
    let mut scene_builder = RenderSceneBuilder::new();
    let root_id = scene_builder.root_frame_id();
    let root_spatial_node_id = scene_builder.paint_container_spatial_node_id(root_id);
    let semantic_tree = build_stacking_context_tree(
        fragments,
        &mut scene_builder,
        root_id,
        SceneClipId::INVALID,
        scroll_state,
        &owner_semantics,
    );
    PaintListBuilder {
        scene_builder: &mut scene_builder,
    }
    .build_stacking_context_into_scene(
        &semantic_tree,
        BuildContext {
            attachment: SpatialAttachment {
                paint_container_id: root_id,
                spatial_node_id: root_spatial_node_id,
                clip_id: SceneClipId::INVALID,
            },
            local_origin: scroll_origin,
        },
    );
    scene_builder.build(owner_semantics)
}

fn frame_key_id_for_iframe(iframe: &IFrameFragment) -> usize {
    iframe
        .base
        .tag
        .map(|tag| tag.node.0)
        .unwrap_or(std::ptr::from_ref(iframe) as usize)
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
