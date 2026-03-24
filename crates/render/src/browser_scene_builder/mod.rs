mod box_fragment;
mod document;
mod effects;
mod geometry;
mod iframe;
mod traversal;

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use makepad_browser_scene::{
    MpChildDocument, MpDocument, MpDocumentId, MpGlyphRunKey, MpGlyphRunResource, MpPipelineId,
    MpSceneId,
};
use makepad_widgets::DVec2;

pub(crate) use document::try_build_browser_document;

#[derive(Clone, Copy)]
pub(super) struct BuildContext {
    pub spatial_id: makepad_browser_scene::MpSpatialId,
    pub clip_chain_id: makepad_browser_scene::MpClipChainId,
    pub effect_id: Option<makepad_browser_scene::MpEffectId>,
    pub containing_block_origin: DVec2,
}

#[derive(Default)]
pub(super) struct DirectBuilderIds {
    next_document_id: u64,
    next_scene_id: u64,
}

impl DirectBuilderIds {
    pub fn alloc_document_id(&mut self) -> MpDocumentId {
        self.next_document_id += 1;
        MpDocumentId(self.next_document_id)
    }

    pub fn alloc_scene_id(&mut self) -> MpSceneId {
        self.next_scene_id += 1;
        MpSceneId(self.next_scene_id)
    }

}

#[derive(Default)]
pub(super) struct BuildState {
    pub glyph_runs: HashMap<MpGlyphRunKey, MpGlyphRunResource>,
    pub child_documents: Vec<MpChildDocument>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct BrowserDocumentScrollNodes {
    pub spatial_nodes: HashMap<usize, makepad_browser_scene::MpSpatialId>,
    pub child_documents: HashMap<MpPipelineId, BrowserDocumentScrollNodes>,
}

#[derive(Clone)]
pub(crate) struct BuiltBrowserDocument {
    pub document: MpDocument,
    pub scroll_nodes: BrowserDocumentScrollNodes,
}

pub(super) fn log_builder_skip_once(reason: impl Into<String>) {
    static LOGGED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let reason = reason.into();
    let logged = LOGGED.get_or_init(|| Mutex::new(HashSet::new()));
    let mut logged = logged.lock().unwrap();
    if logged.insert(reason.clone()) {
        eprintln!("[havi][render] browser_scene builder skipped unsupported content: {reason}");
    }
}
