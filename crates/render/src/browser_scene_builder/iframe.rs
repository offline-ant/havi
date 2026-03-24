use havi_fragment_semantics::fragment_tree::FragmentFlags;
use havi_fragment_semantics::{Fragment, IFrameFragment};
use makepad_browser_scene::{MpChildDocument, MpEmbed, MpHitTestTag, MpScene};
use makepad_widgets::Cx2d;

use super::document::build_browser_document;
use super::geometry::{box_content_insets, fragment_local_bounds, outset_rect, physical_rect_to_rect};
use super::traversal::push_fragment_primitives;
use super::{BuildContext, BuildState, BrowserDocumentScrollNodes, BuiltBrowserDocument, DirectBuilderIds};
use crate::layout_stacking_context::StackingContextSection;
use crate::paint_items::RenderPaintItem;

pub(super) fn build_iframe_fragment(
    cx: &mut Cx2d,
    fragment: &Fragment,
    iframe: &IFrameFragment,
    scroll_state: &crate::ScrollState,
    scene: &mut MpScene,
    state: &mut BuildState,
    ids: &mut DirectBuilderIds,
    build_cx: BuildContext,
    scroll_nodes: &mut BrowserDocumentScrollNodes,
    previous_document: Option<&makepad_browser_scene::MpDocument>,
) -> Result<(), String> {
    if iframe.base.flags.intersects(FragmentFlags::DO_NOT_PAINT) {
        return Ok(());
    }

    let content_bounds = fragment_local_bounds(fragment, build_cx.containing_block_origin);
    let border_bounds = outset_rect(content_bounds, box_content_insets(&iframe.base.style));
    let iframe_rect = physical_rect_to_rect(iframe.base.rect);
    push_fragment_primitives(
        cx,
        scene,
        state,
        &RenderPaintItem {
            section: StackingContextSection::Foreground,
            local_origin: border_bounds.pos - iframe_rect.pos,
            source: fragment,
        },
        iframe.base.tag.map(|tag| tag.node.0),
        build_cx,
    )?;

    let pipeline_id = ids.alloc_pipeline_id();
    let child_document = build_browser_document(
        cx,
        iframe.child_fragments.as_ref(),
        scroll_state,
        content_bounds.size,
        ids,
        previous_document.and_then(|document| document.child_document(pipeline_id)),
    )?;
    let BuiltBrowserDocument {
        document: child_document,
        scroll_nodes: child_scroll_nodes,
    } = child_document;
    scene.push_embed(MpEmbed {
        scene_id: child_document.scene.id,
        pipeline_id,
        spatial_id: build_cx.spatial_id,
        clip_chain_id: build_cx.clip_chain_id,
        effect_id: build_cx.effect_id,
        bounds: content_bounds,
        hit_test_tag: iframe
            .base
            .tag
            .map(|tag| MpHitTestTag(tag.node.0 as u64)),
    });
    state.child_documents.push(MpChildDocument {
        pipeline_id,
        document: Box::new(child_document),
    });
    scroll_nodes
        .child_documents
        .insert(pipeline_id, child_scroll_nodes);
    Ok(())
}
