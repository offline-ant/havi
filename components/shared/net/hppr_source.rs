/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::sync::LazyLock;

use base::id::PipelineId;
use parking_lot::Mutex;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::{HpprSigner, HpprViaSpec};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum HpprDocumentSource {
    Repo,
    Remote {
        endpoint: HpprViaSpec,
        signer: HpprSigner,
        content_root: String,
        content_authority: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HpprDocumentSourceSnapshot {
    pub group: String,
    pub app: String,
    pub source: HpprDocumentSource,
}

static HPPR_SOURCE_REGISTRY: LazyLock<Mutex<FxHashMap<PipelineId, HpprDocumentSourceSnapshot>>> =
    LazyLock::new(|| Mutex::new(FxHashMap::default()));

pub fn set_hppr_document_source(
    pipeline_id: PipelineId,
    snapshot: HpprDocumentSourceSnapshot,
) {
    HPPR_SOURCE_REGISTRY.lock().insert(pipeline_id, snapshot);
}

pub fn get_hppr_document_source(
    pipeline_id: PipelineId,
) -> Option<HpprDocumentSourceSnapshot> {
    HPPR_SOURCE_REGISTRY.lock().get(&pipeline_id).cloned()
}

pub fn clear_hppr_document_source(pipeline_id: PipelineId) {
    HPPR_SOURCE_REGISTRY.lock().remove(&pipeline_id);
}
