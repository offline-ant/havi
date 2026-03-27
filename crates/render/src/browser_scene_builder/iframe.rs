use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use base::id::PipelineId;
use havi_types::fragment_tree as published;
use makepad_browser_scene::{MpChildDocument, MpEmbed, MpHitTestTag, MpPipelineId, MpScene, ResourceRegistry};
use makepad_widgets::Cx2d;

use super::document::build_browser_document;
use super::geometry::{box_content_insets, fragment_local_bounds, outset_rect, physical_rect_to_rect};
use super::traversal::push_fragment_primitives;
use super::{BuildContext, BuildState, BrowserDocumentScrollNodes, BuiltBrowserDocument, DirectBuilderIds};
use crate::layout_stacking_context::StackingContextSection;
use crate::paint_items::RenderPaintItem;

pub(super) fn build_iframe_fragment(
    cx: &mut Cx2d,
    generation: &published::FragmentArenaGeneration,
    fragment_id: published::FragmentId,
    iframe: &published::IFrameFragment,
    scroll_state: &crate::ScrollState,
    scene: &mut MpScene,
    registry: &mut ResourceRegistry,
    state: &mut BuildState,
    ids: &mut DirectBuilderIds,
    build_cx: BuildContext,
    scroll_nodes: &mut BrowserDocumentScrollNodes,
    previous_document: Option<&makepad_browser_scene::MpDocument>,
) -> Result<(), String> {
    if iframe.base.flags.intersects(published::FragmentFlags::DO_NOT_PAINT) {
        return Ok(());
    }

    let content_bounds = fragment_local_bounds(generation, fragment_id, build_cx.containing_block_origin);
    let border_bounds = outset_rect(content_bounds, box_content_insets(&iframe.base.style));
    let iframe_rect = physical_rect_to_rect(iframe.base.rect);
    push_fragment_primitives(
        cx,
        generation,
        scene,
        registry,
        state,
        &RenderPaintItem {
            section: StackingContextSection::Foreground,
            local_origin: border_bounds.pos - iframe_rect.pos,
            fragment_id,
        },
        iframe.base.tag.map(|tag| tag.node.0),
        build_cx,
    )?;

    let pipeline_id = mp_pipeline_id(iframe.pipeline_id);
    let child_generation = layout_api::shared_layout_fragment_tree_for_pipeline(iframe.pipeline_id)
        .get::<published::FragmentArenaGeneration>()
        .unwrap_or_else(|| {
            Arc::new(published::FragmentArenaGeneration {
                geometry_roots: Arc::from(Vec::<published::FragmentId>::new()),
                paint_roots: Arc::from(Vec::<published::PaintChild>::new()),
                nodes: Arc::from(Vec::<published::FragmentNode>::new()),
                placements: Arc::from(Vec::<published::OutOfFlowPlacement>::new()),
                derived: published::FragmentDerivedData {
                    containing_blocks: Vec::new(),
                    scrollable_overflow: Vec::new(),
                    sticky_insets: Vec::new(),
                    background_images: Vec::new(),
                },
                node_fragments: std::collections::HashMap::new(),
                svg_resources: Arc::from(Vec::<published::SVGResourceNode>::new()),
                initial_containing_block: havi_types::PhysicalRect::zero(),
                scrollable_overflow: havi_types::PhysicalRect::zero(),
            })
        });
    let child_document = build_browser_document(
        cx,
        child_generation.as_ref(),
        scroll_state,
        content_bounds.size,
        registry,
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

fn mp_pipeline_id(pipeline_id: PipelineId) -> MpPipelineId {
    let mut hasher = DefaultHasher::new();
    pipeline_id.hash(&mut hasher);
    MpPipelineId(hasher.finish())
}
