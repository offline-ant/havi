use std::collections::HashMap;

use makepad_browser_scene::{
    MpBlendMode as BrowserBlendMode, MpChildDocument, MpClipChain, MpClipChainId, MpClipKind,
    MpClipNode, MpDocument, MpDocumentId, MpEffectNode, MpFilter, MpHitTestTag, MpIsolation,
    MpPipelineId, MpScene, MpSceneId, MpSpatialKind, MpSpatialNode, MpStickyFrame,
    MpStickyOffsets,
};
use makepad_widgets::{dvec2, Cx2d, Rect};

use crate::browser_scene_primitives::{paint_run_to_primitives, AdapterState};
use crate::scene::{
    RenderBlendMode, RenderClipGeometry, RenderNode, RenderNodeId, RenderReferenceFrameKind,
    RenderScene, RenderStickyInfo,
};

#[derive(Clone, Copy)]
struct NodeContext {
    spatial_id: makepad_browser_scene::MpSpatialId,
    clip_chain_id: MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
}

#[derive(Default)]
struct AdapterIds {
    next_document_id: u64,
    next_scene_id: u64,
    next_pipeline_id: u64,
}

impl AdapterIds {
    fn alloc_document_id(&mut self) -> MpDocumentId {
        self.next_document_id += 1;
        MpDocumentId(self.next_document_id)
    }

    fn alloc_scene_id(&mut self) -> MpSceneId {
        self.next_scene_id += 1;
        MpSceneId(self.next_scene_id)
    }

    fn alloc_pipeline_id(&mut self) -> MpPipelineId {
        self.next_pipeline_id += 1;
        MpPipelineId(self.next_pipeline_id)
    }
}

pub(crate) fn try_build_browser_document(
    cx: &mut Cx2d,
    render_scene: &RenderScene<'_>,
) -> Result<MpDocument, String> {
    build_browser_document(cx, render_scene, &mut AdapterIds::default())
}

fn build_browser_document(
    cx: &mut Cx2d,
    render_scene: &RenderScene<'_>,
    ids: &mut AdapterIds,
) -> Result<MpDocument, String> {
    let document_id = ids.alloc_document_id();
    let scene_id = ids.alloc_scene_id();
    let mut scene = MpScene::new(scene_id, render_scene.root_reference_frame().local_rect);
    let mut state = AdapterState {
        resources: makepad_browser_scene::MpResourceStore::default(),
        child_documents: Vec::new(),
    };

    let root_id = render_scene.root_reference_frame_id();
    let mut node_contexts = HashMap::new();
    node_contexts.insert(
        root_id,
        NodeContext {
            spatial_id: scene.root_spatial_id,
            clip_chain_id: scene.root_clip_chain_id,
            effect_id: None,
        },
    );
    let mut clip_chains = HashMap::new();
    let root_clip_chain_id = render_scene
        .root
        .clip
        .map(|clip_id| ensure_clip_chain(render_scene, &mut scene, clip_id, &node_contexts, &mut clip_chains))
        .transpose()?
        .unwrap_or(scene.root_clip_chain_id);
    scene.set_root_clip_chain(root_clip_chain_id);
    node_contexts.insert(
        root_id,
        NodeContext {
            spatial_id: scene.root_spatial_id,
            clip_chain_id: root_clip_chain_id,
            effect_id: None,
        },
    );

    for (index, node) in render_scene.nodes.iter().enumerate().skip(1) {
        let render_id = RenderNodeId(index);
        match node {
            RenderNode::ReferenceFrame(frame) => {
                let parent_ctx = node_contexts
                    .get(&frame.parent.ok_or_else(|| "missing reference-frame parent".to_string())?)
                    .copied()
                    .ok_or_else(|| "reference-frame parent context missing".to_string())?;
                let clip_chain_id = frame
                    .clip
                    .map(|clip_id| {
                        ensure_clip_chain(
                            render_scene,
                            &mut scene,
                            clip_id,
                            &node_contexts,
                            &mut clip_chains,
                        )
                    })
                    .transpose()?
                    .unwrap_or(parent_ctx.clip_chain_id);
                let spatial_id = scene.push_spatial_node(MpSpatialNode {
                    parent: Some(parent_ctx.spatial_id),
                    kind: match &frame.kind {
                        RenderReferenceFrameKind::Root | RenderReferenceFrameKind::Transform => {
                            MpSpatialKind::ReferenceFrame(makepad_browser_scene::MpReferenceFrame {
                                viewport_rect: frame.local_rect,
                                placement_origin: frame.placement_origin,
                                transform: frame.transform,
                                perspective: frame.perspective,
                                transform_style: frame.transform_style,
                                backface_visibility: frame.backface_visibility,
                                flattens_descendants: frame.flattens_descendants,
                            })
                        }
                        RenderReferenceFrameKind::Scroll(info) => {
                            MpSpatialKind::ScrollFrame(makepad_browser_scene::MpScrollFrame {
                                viewport_rect: info.scroll_frame_rect,
                                content_rect: frame.local_rect,
                                scroll_offset: info.scroll_offset,
                            })
                        }
                        RenderReferenceFrameKind::Sticky(info) => {
                            MpSpatialKind::StickyFrame(sticky_frame(info))
                        }
                        RenderReferenceFrameKind::IFrameRoot { size } => {
                            MpSpatialKind::EmbedRoot(makepad_browser_scene::MpEmbedRoot {
                                rect: Rect {
                                    pos: dvec2(0.0, 0.0),
                                    size: *size,
                                },
                            })
                        }
                    },
                });
                node_contexts.insert(
                    render_id,
                    NodeContext {
                        spatial_id,
                        clip_chain_id,
                        effect_id: parent_ctx.effect_id,
                    },
                );
            }
            RenderNode::Clip(_) => {
                ensure_clip_chain(
                    render_scene,
                    &mut scene,
                    crate::scene::RenderClipId(index),
                    &node_contexts,
                    &mut clip_chains,
                )?;
            }
            RenderNode::Effect(effect) => {
                let filters = lower_effect_filters(&effect.filter.entries)?;
                if effect.mask.is_some() {
                    return Err("masks not supported by browser-scene adapter yet".to_string());
                }
                let parent_ctx = node_contexts
                    .get(&effect.parent)
                    .copied()
                    .ok_or_else(|| "effect parent context missing".to_string())?;
                let clip_chain_id = effect
                    .clip
                    .map(|clip_id| {
                        ensure_clip_chain(
                            render_scene,
                            &mut scene,
                            clip_id,
                            &node_contexts,
                            &mut clip_chains,
                        )
                    })
                    .transpose()?
                    .unwrap_or(parent_ctx.clip_chain_id);
                let effect_id = scene.push_effect(MpEffectNode {
                    spatial_id: parent_ctx.spatial_id,
                    clip_chain_id,
                    opacity: effect.opacity,
                    filters,
                    blend_mode: match &effect.blend_mode {
                        RenderBlendMode::Normal => BrowserBlendMode::Normal,
                        RenderBlendMode::Named(name) => BrowserBlendMode::Named(name.clone()),
                    },
                    isolation: if effect.is_isolated {
                        MpIsolation::Isolate
                    } else {
                        MpIsolation::Auto
                    },
                    mask: None,
                });
                node_contexts.insert(
                    render_id,
                    NodeContext {
                        spatial_id: parent_ctx.spatial_id,
                        clip_chain_id,
                        effect_id: Some(effect_id),
                    },
                );
            }
            RenderNode::PaintRun(run) => {
                let parent_ctx = node_contexts
                    .get(&run.parent)
                    .copied()
                    .ok_or_else(|| "paint-run parent context missing".to_string())?;
                let clip_chain_id = run
                    .clip
                    .map(|clip_id| {
                        ensure_clip_chain(
                            render_scene,
                            &mut scene,
                            clip_id,
                            &node_contexts,
                            &mut clip_chains,
                        )
                    })
                    .transpose()?
                    .unwrap_or(parent_ctx.clip_chain_id);
                let primitives = paint_run_to_primitives(
                    cx,
                    &mut scene,
                    &mut state,
                    run,
                    parent_ctx.spatial_id,
                    clip_chain_id,
                    parent_ctx.effect_id,
                )?;
                for primitive in primitives {
                    scene.push_primitive(primitive);
                }
            }
            RenderNode::Embed(embed) => {
                let parent_ctx = node_contexts
                    .get(&embed.parent)
                    .copied()
                    .ok_or_else(|| "embed parent context missing".to_string())?;
                let clip_chain_id = embed
                    .clip
                    .map(|clip_id| {
                        ensure_clip_chain(
                            render_scene,
                            &mut scene,
                            clip_id,
                            &node_contexts,
                            &mut clip_chains,
                        )
                    })
                    .transpose()?
                    .unwrap_or(parent_ctx.clip_chain_id);
                let child_document = build_browser_document(cx, &embed.child_scene, ids)?;
                let pipeline_id = ids.alloc_pipeline_id();
                scene.push_embed(makepad_browser_scene::MpEmbed {
                    scene_id: child_document.scene.id,
                    pipeline_id,
                    spatial_id: parent_ctx.spatial_id,
                    clip_chain_id,
                    effect_id: parent_ctx.effect_id,
                    bounds: embed.local_rect,
                    hit_test_tag: embed.owner_node_id.map(|id| MpHitTestTag(id as u64)),
                });
                state.child_documents.push(MpChildDocument {
                    pipeline_id,
                    document: Box::new(child_document),
                });
            }
        }
    }

    Ok(MpDocument {
        id: document_id,
        epoch: 0,
        scene,
        resources: state.resources,
        child_documents: state.child_documents,
    })
}

fn ensure_clip_chain(
    render_scene: &RenderScene<'_>,
    scene: &mut MpScene,
    clip_id: crate::scene::RenderClipId,
    node_contexts: &HashMap<RenderNodeId, NodeContext>,
    clip_chains: &mut HashMap<crate::scene::RenderClipId, MpClipChainId>,
) -> Result<MpClipChainId, String> {
    if let Some(chain_id) = clip_chains.get(&clip_id).copied() {
        return Ok(chain_id);
    }
    let clip = render_scene
        .clip(clip_id)
        .ok_or_else(|| "clip missing".to_string())?;
    let owner = clip.parent.ok_or_else(|| "clip owner missing".to_string())?;
    let owner_ctx = node_contexts
        .get(&owner)
        .copied()
        .ok_or_else(|| "clip owner context missing".to_string())?;
    let parent_chain_id = clip
        .prev
        .map(|prev| ensure_clip_chain(render_scene, scene, prev, node_contexts, clip_chains))
        .transpose()?
        .unwrap_or(owner_ctx.clip_chain_id);
    let clip_id_out = scene.push_clip(MpClipNode {
        spatial_id: owner_ctx.spatial_id,
        kind: match &clip.geometry {
            RenderClipGeometry::Rect { rect } => MpClipKind::Rect { rect: *rect },
            RenderClipGeometry::RoundedRect { rect, radius } => MpClipKind::RoundedRect {
                rect: *rect,
                radius: makepad_browser_scene::MpPerCornerRadius::uniform(*radius),
            },
            RenderClipGeometry::PlaneSet { .. } => {
                return Err("plane-set clip not supported by browser-scene adapter yet".to_string())
            }
        },
    });
    let chain_id = scene.push_clip_chain(MpClipChain {
        parent: Some(parent_chain_id),
        clips: vec![clip_id_out],
    });
    clip_chains.insert(clip_id, chain_id);
    Ok(chain_id)
}

fn lower_effect_filters(entries: &[String]) -> Result<Vec<MpFilter>, String> {
    let mut filters = Vec::new();
    for entry in entries {
        if let Some(value) = entry
            .strip_prefix("blur(")
            .and_then(|value| value.strip_suffix(')'))
        {
            let radius = value
                .parse::<f32>()
                .map_err(|_| format!("invalid blur filter entry: {entry}"))?;
            filters.push(MpFilter::Blur(radius.max(0.0)));
            continue;
        }
        if let Some(value) = entry
            .strip_prefix("opacity(")
            .and_then(|value| value.strip_suffix(')'))
        {
            let opacity = value
                .parse::<f32>()
                .map_err(|_| format!("invalid opacity filter entry: {entry}"))?;
            filters.push(MpFilter::Opacity(opacity.clamp(0.0, 1.0)));
            continue;
        }
        return Err(format!("filters not supported by browser-scene adapter yet: {entry}"));
    }
    Ok(filters)
}

fn sticky_frame(info: &RenderStickyInfo) -> MpStickyFrame {
    MpStickyFrame {
        frame_rect: info.frame_rect,
        containing_block_rect: info.containing_block_rect,
        margins: MpStickyOffsets {
            top: info.margins.top,
            right: info.margins.right,
            bottom: info.margins.bottom,
            left: info.margins.left,
        },
    }
}
