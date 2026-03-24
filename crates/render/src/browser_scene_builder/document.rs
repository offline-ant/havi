use havi_fragment_semantics::Fragment;
use makepad_browser_scene::{MpDocument, MpScene};
use makepad_widgets::{dvec2, Cx2d, DVec2, Rect};

use super::{BuildContext, BuildState, BrowserDocumentScrollNodes, BuiltBrowserDocument, DirectBuilderIds};
use super::traversal::build_fragment_list;

pub(crate) fn try_build_browser_document(
    cx: &mut Cx2d,
    fragments: &[Fragment],
    scroll_state: &crate::ScrollState,
    viewport_size: DVec2,
    previous_document: Option<&MpDocument>,
) -> Result<BuiltBrowserDocument, String> {
    build_browser_document(
        cx,
        fragments,
        scroll_state,
        viewport_size,
        &mut DirectBuilderIds::default(),
        previous_document,
    )
}

pub(super) fn build_browser_document(
    cx: &mut Cx2d,
    fragments: &[Fragment],
    scroll_state: &crate::ScrollState,
    viewport_size: DVec2,
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
    let mut state = BuildState {
        resources: previous_document
            .map(|document| document.resources.clone())
            .unwrap_or_default(),
        child_documents: Vec::new(),
    };
    let mut scroll_nodes = BrowserDocumentScrollNodes::default();
    build_fragment_list(
        cx,
        fragments,
        scroll_state,
        &mut scene,
        &mut state,
        ids,
        BuildContext {
            spatial_id: root_spatial_id,
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
            resources: state.resources,
            child_documents: state.child_documents,
        },
        scroll_nodes,
    })
}
