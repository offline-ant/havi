use crate::layout_stacking_context::{
    build_stacking_context_tree, LayoutPaintItem, LayoutStackingContext,
    LayoutStackingContextContent, SpatialAttachment, StackingContextSection,
};
use crate::paint_items::PaintSource;
use crate::scene::{
    PaintContainerKind, ReferenceFrameData, RenderScene, SceneClipId, SceneClipKind,
    SpatialNodeSemantics,
};
use makepad_compositor::{MpBackfaceVisibility, MpTransformStyle};
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
            LayoutPaintItem::ChildStackingContext(child) => {
                if child.entry_paint_container_id != child.insertion_attachment.paint_container_id {
                    self.scene_builder.append_child_paint_container(
                        child.insertion_attachment.paint_container_id,
                        child.entry_paint_container_id,
                    );
                }
                let mut child_attachment = child.attachment;
                child_attachment.paint_container_id = child.entry_paint_container_id;
                self.build_stacking_context_into_scene(
                    child,
                    BuildContext {
                        attachment: child_attachment,
                        local_origin: child.attachment.scene_origin,
                    },
                );
            }
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
            } => {
                let differs_from_current_container =
                    attachment.paint_container_id != cx.attachment.paint_container_id;
                let item_cx = BuildContext {
                    attachment: *attachment,
                    local_origin: if differs_from_current_container {
                        dvec2(0.0, 0.0)
                    } else {
                        attachment.scene_origin
                    },
                };
                if differs_from_current_container {
                    self.scene_builder.append_child_paint_container(
                        cx.attachment.paint_container_id,
                        item_cx.attachment.paint_container_id,
                    );
                }
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
                    cx.attachment.paint_container_id,
                    cx.attachment.clip_id,
                );
            }
            Fragment::IFrame(iframe) => {
                self.scene_builder.push_item(
                    cx.attachment.paint_container_id,
                    source,
                    section,
                    cx.local_origin,
                    cx.attachment.paint_container_id,
                    cx.attachment.clip_id,
                );
                self.build_iframe_into_scene(iframe, cx);
            }
            Fragment::Positioning(_) | Fragment::AbsoluteOrFixedPositioned { .. } => {}
        }
    }

    fn build_iframe_into_scene(&mut self, iframe: &'a IFrameFragment, cx: BuildContext) {
        let iframe_origin = iframe_content_origin(iframe, cx.local_origin);
        let spatial_node_id = self.scene_builder.child_iframe_root_node(
            cx.attachment.spatial_node_id,
            iframe.base.tag.map(|tag| tag.node.0),
        );
        let spatial_node_id = self.scene_builder.child_spatial_node(
            spatial_node_id,
            SpatialNodeSemantics::ReferenceFrame(ReferenceFrameData {
                placement_origin: dvec2(0.0, 0.0),
                transform_matrix: Some(translation_matrix(iframe_origin.x as f32, iframe_origin.y as f32)),
                perspective_matrix: None,
                transform_style: MpTransformStyle::Flat,
                flattens_descendants: true,
                backface_visibility: MpBackfaceVisibility::Visible,
            }),
            iframe.base.tag.map(|tag| tag.node.0),
        );
        let paint_container_id = self.scene_builder.child_paint_container_with_kind(
            cx.attachment.paint_container_id,
            spatial_node_id,
            iframe.base.tag.map(|tag| tag.node.0),
            PaintContainerKind::IFrameRoot {
                size: dvec2(
                    iframe.base.rect.size.width.to_f32_px() as f64,
                    iframe.base.rect.size.height.to_f32_px() as f64,
                ),
            },
        );
        self.scene_builder.append_child_paint_container(
            cx.attachment.paint_container_id,
            paint_container_id,
        );
        let clip_id = self.scene_builder.rect_clip(
            paint_container_id,
            SceneClipId::INVALID,
            Rect {
                pos: dvec2(0.0, 0.0),
                size: dvec2(
                    iframe.base.rect.size.width.to_f32_px() as f64,
                    iframe.base.rect.size.height.to_f32_px() as f64,
                ),
            },
            SceneClipKind::Overflow,
        );
        self.scene_builder.set_frame_clip(paint_container_id, clip_id);
        let child_sc = build_stacking_context_tree(
            &iframe.child_fragments,
            self.scene_builder,
            paint_container_id,
            clip_id,
            &crate::ScrollState::default(),
        );
        self.build_stacking_context_into_scene(
            &child_sc,
            BuildContext {
                attachment: SpatialAttachment {
                    paint_container_id,
                    spatial_node_id,
                    clip_id,
                    scene_origin: dvec2(0.0, 0.0),
                },
                local_origin: dvec2(0.0, 0.0),
            },
        );
    }
}

pub(crate) fn build_scene<'a>(
    fragments: &'a [Fragment],
    scroll_state: &crate::ScrollState,
    _viewport_size: DVec2,
) -> BuiltScene<'a> {
    let owner_semantics = crate::render_plan::collect_owner_render_semantics(fragments);
    let mut scene_builder = RenderSceneBuilder::new();
    let root_id = scene_builder.root_paint_container_id();
    let root_spatial_node_id = scene_builder.paint_container_spatial_node_id(root_id);
    let semantic_tree = build_stacking_context_tree(
        fragments,
        &mut scene_builder,
        root_id,
        SceneClipId::INVALID,
        scroll_state,
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
                scene_origin: dvec2(0.0, 0.0),
            },
            local_origin: dvec2(0.0, 0.0),
        },
    );
    scene_builder.build(owner_semantics)
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
