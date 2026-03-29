use havi_types::fragment_tree as published;
use makepad_browser_scene::{
    MpDocument, MpScene, MpScrollFrame, MpSpatialKind, MpSpatialNode, ResourceRegistry,
};
use makepad_widgets::{dvec2, Cx2d, DVec2, Rect};
use webrender_api::{ExternalScrollId, PipelineId};

use super::geometry::physical_rect_to_rect;
use super::traversal::build_paint_list;
use super::{
    BrowserDocumentScrollNodes, BuildContext, BuildState, BuiltBrowserDocument, DirectBuilderIds,
};

pub(crate) fn try_build_browser_document(
    cx: &mut Cx2d,
    generation: &published::FragmentArenaGeneration,
    scroll_state: &crate::ScrollState,
    viewport_size: DVec2,
    root_pipeline_id: PipelineId,
    registry: &mut ResourceRegistry,
    previous_document: Option<&MpDocument>,
) -> Result<BuiltBrowserDocument, String> {
    build_browser_document(
        cx,
        generation,
        scroll_state,
        viewport_size,
        root_pipeline_id,
        registry,
        &mut DirectBuilderIds::default(),
        previous_document,
    )
}

pub(super) fn build_browser_document(
    cx: &mut Cx2d,
    generation: &published::FragmentArenaGeneration,
    scroll_state: &crate::ScrollState,
    viewport_size: DVec2,
    root_pipeline_id: PipelineId,
    registry: &mut ResourceRegistry,
    ids: &mut DirectBuilderIds,
    previous_document: Option<&MpDocument>,
) -> Result<BuiltBrowserDocument, String> {
    let viewport_rect = Rect {
        pos: dvec2(0.0, 0.0),
        size: viewport_size,
    };
    let mut scene = MpScene::new(ids.alloc_scene_id(), viewport_rect);
    let root_spatial_id = scene.root_spatial_id;
    let root_clip_chain_id = scene.root_clip_chain_id;

    // Create root scroll frame — same mechanism as per-element scroll frames.
    // Key 0 matches ExternalScrollId(0, pipeline).0 used by layout for root scroll.
    let root_scroll_node_id = ExternalScrollId(0, root_pipeline_id);
    let content_size = physical_rect_to_rect(generation.scrollable_overflow).size;
    let root_scroll_offset = scroll_state
        .get(&root_scroll_node_id)
        .copied()
        .unwrap_or_else(|| dvec2(0.0, 0.0));
    let root_scroll_spatial_id = scene.push_spatial_node(MpSpatialNode {
        parent: Some(root_spatial_id),
        kind: MpSpatialKind::ScrollFrame(MpScrollFrame {
            viewport_rect,
            content_rect: Rect {
                pos: dvec2(0.0, 0.0),
                size: content_size,
            },
            scroll_offset: root_scroll_offset,
        }),
    });

    let mut state = BuildState {
        glyph_runs: Default::default(),
        child_documents: Vec::new(),
    };
    let mut scroll_nodes = BrowserDocumentScrollNodes::default();
    scroll_nodes
        .spatial_nodes
        .insert(root_scroll_node_id, root_scroll_spatial_id);
    build_paint_list(
        cx,
        generation,
        generation.paint_roots.as_ref(),
        scroll_state,
        &mut scene,
        registry,
        &mut state,
        ids,
        BuildContext {
            pipeline_id: root_pipeline_id,
            spatial_id: root_scroll_spatial_id,
            clip_chain_id: root_clip_chain_id,
            effect_id: None,
            containing_block_origin: dvec2(0.0, 0.0),
        },
        &mut scroll_nodes,
        previous_document,
    )?;
    Ok(BuiltBrowserDocument {
        document: MpDocument {
            id: ids.alloc_document_id(),
            epoch: 0,
            scene,
            glyph_runs: state.glyph_runs,
            child_documents: state.child_documents,
        },
        scroll_nodes,
    })
}
