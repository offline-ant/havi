use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use havi_fragment_semantics::fragment_tree::{BoxFragment, ImageFragment, TextFragment};
use havi_fragment_semantics::Fragment;
use makepad_browser_scene::{
    MpBlendMode as BrowserBlendMode, MpChildDocument, MpClipChain, MpClipChainId, MpClipKind,
    MpClipNode, MpDocument, MpDocumentId, MpEffectNode, MpEmbed, MpFontKey, MpFontResource,
    MpGlyphRunKey, MpGlyphRunMetrics, MpGlyphRunResource, MpHitTestTag, MpIsolation,
    MpPipelineId, MpPositionedGlyph, MpPrimitive, MpResourceStore, MpScene, MpSceneId,
    MpSpatialId, MpSpatialKind, MpSpatialNode, MpScrollFrame, MpStickyFrame,
    MpStickyOffsets, MpTextDecorations, MpTextShadow,
};
use makepad_widgets::{dvec2, vec2, Cx2d, Rect, Vec2f};
use style::color::{AbsoluteColor, ColorSpace};
use style::properties::ComputedValues;
use style::values::specified::TextDecorationLine;
use style::values::specified::border::BorderStyle;

use crate::background::{
    BackgroundLayerGeom, layout_background_layer, resolve_border_radii, resolve_insets,
};
use crate::color::{inherited_color, resolve_color};
use crate::layout_stacking_context::StackingContextSection;
use crate::scene::{
    RenderBlendMode, RenderClipGeometry, RenderNode, RenderNodeId, RenderPaintRun,
    RenderReferenceFrameKind, RenderScene, RenderStickyInfo,
};

#[derive(Clone, Copy)]
struct NodeContext {
    spatial_id: MpSpatialId,
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

struct AdapterState {
    resources: MpResourceStore,
    child_documents: Vec<MpChildDocument>,
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
    if render_scene.root.clip.is_some() {
        return Err("root clip not supported by browser-scene adapter yet".to_string());
    }

    let document_id = ids.alloc_document_id();
    let scene_id = ids.alloc_scene_id();
    let mut scene = MpScene::new(scene_id, render_scene.root_reference_frame().local_rect);
    let mut state = AdapterState {
        resources: MpResourceStore::default(),
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

    for (index, node) in render_scene.nodes.iter().enumerate().skip(1) {
        let render_id = RenderNodeId(index);
        match node {
            RenderNode::ReferenceFrame(frame) => {
                let parent_ctx = node_contexts
                    .get(&frame.parent.ok_or_else(|| "missing reference-frame parent".to_string())?)
                    .copied()
                    .ok_or_else(|| "reference-frame parent context missing".to_string())?;
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
                            MpSpatialKind::ScrollFrame(MpScrollFrame {
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
                        clip_chain_id: frame
                            .clip
                            .and_then(|clip_id| clip_chains.get(&clip_id).copied())
                            .unwrap_or(parent_ctx.clip_chain_id),
                        effect_id: parent_ctx.effect_id,
                    },
                );
            }
            RenderNode::Clip(clip) => {
                let owner = clip
                    .parent
                    .ok_or_else(|| "clip owner missing".to_string())?;
                let owner_ctx = node_contexts
                    .get(&owner)
                    .copied()
                    .ok_or_else(|| "clip owner context missing".to_string())?;
                let clip_id = scene.push_clip(MpClipNode {
                    spatial_id: owner_ctx.spatial_id,
                    kind: match &clip.geometry {
                        RenderClipGeometry::Rect { rect } => MpClipKind::Rect { rect: *rect },
                        RenderClipGeometry::RoundedRect { rect, radius } => MpClipKind::RoundedRect {
                            rect: *rect,
                            radius: *radius,
                        },
                        RenderClipGeometry::PlaneSet { .. } => {
                            return Err("plane-set clip not supported by browser-scene adapter yet".to_string())
                        }
                    },
                });
                let chain_id = scene.push_clip_chain(MpClipChain {
                    parent: clip.prev.and_then(|prev| clip_chains.get(&prev).copied()),
                    clips: vec![clip_id],
                });
                clip_chains.insert(crate::scene::RenderClipId(index), chain_id);
            }
            RenderNode::Effect(effect) => {
                if !effect.filter.entries.is_empty() {
                    return Err("filters not supported by browser-scene adapter yet".to_string());
                }
                if effect.mask.is_some() {
                    return Err("masks not supported by browser-scene adapter yet".to_string());
                }
                let parent_ctx = node_contexts
                    .get(&effect.parent)
                    .copied()
                    .ok_or_else(|| "effect parent context missing".to_string())?;
                let effect_id = scene.push_effect(MpEffectNode {
                    spatial_id: parent_ctx.spatial_id,
                    clip_chain_id: effect
                        .clip
                        .and_then(|clip_id| clip_chains.get(&clip_id).copied())
                        .unwrap_or(parent_ctx.clip_chain_id),
                    opacity: effect.opacity,
                    filters: Vec::new(),
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
                        clip_chain_id: effect
                            .clip
                            .and_then(|clip_id| clip_chains.get(&clip_id).copied())
                            .unwrap_or(parent_ctx.clip_chain_id),
                        effect_id: Some(effect_id),
                    },
                );
            }
            RenderNode::PaintRun(run) => {
                let parent_ctx = node_contexts
                    .get(&run.parent)
                    .copied()
                    .ok_or_else(|| "paint-run parent context missing".to_string())?;
                let primitives = paint_run_to_primitives(
                    cx,
                    &mut scene,
                    &mut state,
                    run,
                    parent_ctx.spatial_id,
                    run.clip
                        .and_then(|clip_id| clip_chains.get(&clip_id).copied())
                        .unwrap_or(parent_ctx.clip_chain_id),
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
                let child_document = build_browser_document(cx, &embed.child_scene, ids)?;
                let pipeline_id = ids.alloc_pipeline_id();
                scene.push_embed(MpEmbed {
                    scene_id: child_document.scene.id,
                    pipeline_id,
                    spatial_id: parent_ctx.spatial_id,
                    clip_chain_id: embed
                        .clip
                        .and_then(|clip_id| clip_chains.get(&clip_id).copied())
                        .unwrap_or(parent_ctx.clip_chain_id),
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

fn paint_run_to_primitives(
    cx: &mut Cx2d,
    scene: &mut MpScene,
    state: &mut AdapterState,
    run: &RenderPaintRun<'_>,
    spatial_id: MpSpatialId,
    clip_chain_id: MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
) -> Result<Vec<MpPrimitive>, String> {
    let mut primitives = Vec::new();
    for item in &run.items {
        primitives.extend(paint_run_item_to_primitives(
            cx,
            scene,
            state,
            item,
            run.owner_node_id,
            spatial_id,
            clip_chain_id,
            effect_id,
        )?);
    }
    Ok(primitives)
}

fn paint_run_item_to_primitives(
    cx: &mut Cx2d,
    scene: &mut MpScene,
    state: &mut AdapterState,
    item: &crate::scene::RenderPaintItem<'_>,
    run_owner_node_id: Option<usize>,
    spatial_id: MpSpatialId,
    clip_chain_id: MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
) -> Result<Vec<MpPrimitive>, String> {
    let bounds = paint_item_bounds(item);
    let owner_node_id = paint_item_owner_node_id(item.source).or(run_owner_node_id);
    match (item.section, item.source) {
        (StackingContextSection::OwnBackgroundsAndBorders, Fragment::Box(bf))
        | (StackingContextSection::OwnBackgroundsAndBorders, Fragment::Float(bf)) => lower_box_primitives(
            scene,
            &mut state.resources,
            bounds,
            bf,
            spatial_id,
            clip_chain_id,
            effect_id,
            owner_node_id,
        ),
        (StackingContextSection::Foreground, Fragment::Text(tf)) => {
            let (glyph_run_key, glyph_run) = make_glyph_run_resource(cx, owner_node_id, bounds, tf)?;
            state.resources.glyph_runs.insert(glyph_run_key, glyph_run);
            let (font_key, font_resource) = font_resource_for_text(cx, tf)?;
            state.resources.fonts.entry(font_key).or_insert(font_resource);

            let mut primitive = MpPrimitive::text_run(
                makepad_browser_scene::MpPrimitiveId(0),
                spatial_id,
                clip_chain_id,
                bounds,
                glyph_run_key,
                inherited_color(&tf.base.style),
            );
            primitive.effect_id = effect_id;
            primitive.hit_test_tag = owner_node_id.map(|id| MpHitTestTag(id as u64));
            Ok(vec![primitive])
        }
        (StackingContextSection::Foreground, Fragment::Image(image)) => {
            let (image_key, image_resource) = image_resource_for_fragment(image);
            state.resources.images.entry(image_key).or_insert(image_resource);
            let mut primitive = MpPrimitive {
                id: makepad_browser_scene::MpPrimitiveId(0),
                spatial_id,
                clip_chain_id,
                effect_id,
                bounds,
                kind: makepad_browser_scene::MpPrimitiveKind::Image(makepad_browser_scene::MpImage {
                    image_key,
                }),
                hit_test_tag: owner_node_id.map(|id| MpHitTestTag(id as u64)),
            };
            primitive.effect_id = effect_id;
            Ok(vec![primitive])
        }
        _ => Err("paint run not supported by browser-scene adapter yet".to_string()),
    }
}

fn paint_item_owner_node_id(fragment: &Fragment) -> Option<usize> {
    match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => bf.base.tag.map(|tag| tag.node.0),
        Fragment::Text(tf) => tf.base.tag.map(|tag| tag.node.0),
        Fragment::Image(image) => image.base.tag.map(|tag| tag.node.0),
        Fragment::IFrame(iframe) => iframe.base.tag.map(|tag| tag.node.0),
        Fragment::Positioning(positioning) => positioning.base.tag.map(|tag| tag.node.0),
        Fragment::AbsoluteOrFixedPositioned { .. } => None,
    }
}

fn paint_item_bounds(item: &crate::scene::RenderPaintItem<'_>) -> Rect {
    let rect = match item.source {
        Fragment::Box(bf) | Fragment::Float(bf) => physical_rect_to_rect(bf.border_rect()),
        Fragment::Text(tf) => physical_rect_to_rect(tf.base.rect),
        Fragment::Image(image) => physical_rect_to_rect(image.base.rect),
        Fragment::IFrame(iframe) => physical_rect_to_rect(iframe.base.rect),
        Fragment::Positioning(_) | Fragment::AbsoluteOrFixedPositioned { .. } => Rect {
            pos: dvec2(0.0, 0.0),
            size: dvec2(0.0, 0.0),
        },
    };
    Rect {
        pos: item.local_origin + rect.pos,
        size: rect.size,
    }
}

fn physical_rect_to_rect(rect: havi_types::PhysicalRect<app_units::Au>) -> Rect {
    Rect {
        pos: dvec2(rect.origin.x.to_f32_px() as f64, rect.origin.y.to_f32_px() as f64),
        size: dvec2(rect.size.width.to_f32_px() as f64, rect.size.height.to_f32_px() as f64),
    }
}

#[derive(Clone, Copy)]
struct BorderSidePaint {
    width: f64,
    color: makepad_widgets::Vec4f,
    style: BorderStyle,
}

#[derive(Default)]
struct BorderPaint {
    top: Option<BorderSidePaint>,
    right: Option<BorderSidePaint>,
    bottom: Option<BorderSidePaint>,
    left: Option<BorderSidePaint>,
}

#[derive(Clone, Copy)]
struct OutlinePaint {
    width: f64,
    offset: f64,
    color: makepad_widgets::Vec4f,
    style: BorderStyle,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum BorderSide {
    Top,
    Right,
    Bottom,
    Left,
}

fn lower_box_primitives(
    scene: &mut MpScene,
    resources: &mut MpResourceStore,
    bounds: Rect,
    bf: &BoxFragment,
    spatial_id: MpSpatialId,
    clip_chain_id: MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
    owner_node_id: Option<usize>,
) -> Result<Vec<MpPrimitive>, String> {
    let computed = &bf.base.style;
    if has_unsupported_background_layers(computed) {
        return Err("background images not supported by browser-scene adapter yet".to_string());
    }
    let current = inherited_color(computed);
    let current_abs = AbsoluteColor::new(ColorSpace::Srgb, current.x, current.y, current.z, current.w);
    let border = border_paint(computed, &current_abs);
    let outline = outline_paint(computed, &current_abs);
    let radius = uniform_border_radius(computed)?;

    let mut primitives = Vec::new();
    append_box_shadow_primitives(
        &mut primitives,
        computed,
        bounds,
        radius,
        spatial_id,
        clip_chain_id,
        effect_id,
        owner_node_id,
        &current_abs,
    );
    let background_color = resolve_color(&computed.get_background().background_color, &current_abs);
    if background_color.w > 0.001 {
        let mut primitive = if radius > 0.0 {
            MpPrimitive::rounded_rect(
                makepad_browser_scene::MpPrimitiveId(0),
                spatial_id,
                clip_chain_id,
                bounds,
                background_color,
                radius,
            )
        } else {
            MpPrimitive::solid_rect(
                makepad_browser_scene::MpPrimitiveId(0),
                spatial_id,
                clip_chain_id,
                bounds,
                background_color,
            )
        };
        primitive.effect_id = effect_id;
        primitive.hit_test_tag = owner_node_id.map(|id| MpHitTestTag(id as u64));
        primitives.push(primitive);
    }
    append_background_layer_primitives(
        scene,
        &mut primitives,
        computed,
        &bf.background_images,
        bounds,
        radius,
        spatial_id,
        clip_chain_id,
        effect_id,
        owner_node_id,
        &current_abs,
        resources,
    )?;

    if radius > 0.0 {
        if let Some((width, color)) = uniform_rounded_border(&border) {
            let mut primitive = MpPrimitive::border(
                makepad_browser_scene::MpPrimitiveId(0),
                spatial_id,
                clip_chain_id,
                bounds,
                color,
                width as f32,
                radius,
            );
            primitive.effect_id = effect_id;
            primitive.hit_test_tag = owner_node_id.map(|id| MpHitTestTag(id as u64));
            primitives.push(primitive);
        } else if has_border_paint(&border) {
            return Err("rounded borders with non-uniform edges are not supported by browser-scene adapter yet".to_string());
        }

        if let Some(outline) = outline {
            if matches!(outline.style, BorderStyle::Solid) {
                let expanded = Rect {
                    pos: dvec2(
                        bounds.pos.x - outline.offset - outline.width,
                        bounds.pos.y - outline.offset - outline.width,
                    ),
                    size: dvec2(
                        bounds.size.x + 2.0 * (outline.offset + outline.width),
                        bounds.size.y + 2.0 * (outline.offset + outline.width),
                    ),
                };
                let mut primitive = MpPrimitive::border(
                    makepad_browser_scene::MpPrimitiveId(0),
                    spatial_id,
                    clip_chain_id,
                    expanded,
                    outline.color,
                    outline.width as f32,
                    radius + outline.offset as f32 + outline.width as f32,
                );
                primitive.effect_id = effect_id;
                primitive.hit_test_tag = owner_node_id.map(|id| MpHitTestTag(id as u64));
                primitives.push(primitive);
            } else {
                return Err("rounded outlines with non-solid styles are not supported by browser-scene adapter yet".to_string());
            }
        }
        return Ok(primitives);
    }

    append_border_primitives(
        &mut primitives,
        bounds,
        &border,
        spatial_id,
        clip_chain_id,
        effect_id,
        owner_node_id,
    );
    append_outline_primitives(
        &mut primitives,
        bounds,
        outline,
        spatial_id,
        clip_chain_id,
        effect_id,
        owner_node_id,
    );
    Ok(primitives)
}

fn uniform_border_radius(computed: &ComputedValues) -> Result<f32, String> {
    let radii = resolve_border_radii(computed);
    if radii.max() <= 0.0 {
        return Ok(0.0);
    }
    if radii.tl == radii.tr && radii.tl == radii.br && radii.tl == radii.bl {
        return Ok(radii.tl);
    }
    Err("non-uniform rounded boxes are not supported by browser-scene adapter yet".to_string())
}

fn append_box_shadow_primitives(
    primitives: &mut Vec<MpPrimitive>,
    computed: &ComputedValues,
    bounds: Rect,
    corner_radius_px: f32,
    spatial_id: MpSpatialId,
    clip_chain_id: MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
    owner_node_id: Option<usize>,
    current_abs: &AbsoluteColor,
) {
    for shadow in computed.get_effects().box_shadow.0.iter().rev() {
        let horizontal = shadow.base.horizontal.px();
        let vertical = shadow.base.vertical.px();
        let blur = shadow.base.blur.px();
        let spread = shadow.spread.px();
        let color = resolve_color(&shadow.base.color, current_abs);
        if color.w <= 0.001 {
            continue;
        }
        let sigma = blur * 0.5;
        let extent = (sigma * 3.0).max(0.0);
        let (primitive_bounds, box_offset, box_size) = if shadow.inset {
            (
                bounds,
                vec2(spread + horizontal, spread + vertical),
                vec2(
                    (bounds.size.x as f32 - 2.0 * spread).max(0.0),
                    (bounds.size.y as f32 - 2.0 * spread).max(0.0),
                ),
            )
        } else {
            let shadow_width = bounds.size.x as f32 + 2.0 * spread;
            let shadow_height = bounds.size.y as f32 + 2.0 * spread;
            (
                Rect {
                    pos: dvec2(
                        bounds.pos.x + (horizontal - spread - extent) as f64,
                        bounds.pos.y + (vertical - spread - extent) as f64,
                    ),
                    size: dvec2(
                        (shadow_width + 2.0 * extent) as f64,
                        (shadow_height + 2.0 * extent) as f64,
                    ),
                },
                vec2(extent, extent),
                vec2(shadow_width.max(0.0), shadow_height.max(0.0)),
            )
        };
        let mut primitive = MpPrimitive {
            id: makepad_browser_scene::MpPrimitiveId(0),
            spatial_id,
            clip_chain_id,
            effect_id,
            bounds: primitive_bounds,
            kind: makepad_browser_scene::MpPrimitiveKind::BoxShadow(makepad_browser_scene::MpBoxShadow {
                color,
                box_offset,
                box_size,
                sigma,
                corner_radius_px,
                inset: shadow.inset,
            }),
            hit_test_tag: owner_node_id.map(|id| MpHitTestTag(id as u64)),
        };
        primitive.effect_id = effect_id;
        primitives.push(primitive);
    }
}

fn has_unsupported_background_layers(computed: &ComputedValues) -> bool {
    use style::values::computed::image::Image;

    computed
        .get_background()
        .background_image
        .0
        .iter()
        .any(|image| !matches!(image, Image::Gradient(_) | Image::Url(_)))
}

fn background_layer_bounds(layer: &BackgroundLayerGeom) -> Rect {
    Rect {
        pos: dvec2(layer.bounds_x, layer.bounds_y),
        size: dvec2(layer.bounds_w as f64, layer.bounds_h as f64),
    }
}

fn background_layer_tile_rects(layer: &BackgroundLayerGeom) -> Vec<Rect> {
    let tile_w = layer.tile_w.max(0.001) as f64;
    let tile_h = layer.tile_h.max(0.001) as f64;
    let end_x = layer.bounds_x + layer.bounds_w as f64;
    let end_y = layer.bounds_y + layer.bounds_h as f64;
    let mut rects = Vec::new();
    let mut y = layer.bounds_y;
    while y < end_y - 0.001 {
        let mut x = layer.bounds_x;
        while x < end_x - 0.001 {
            rects.push(Rect {
                pos: dvec2(x, y),
                size: dvec2(tile_w, tile_h),
            });
            x += tile_w;
        }
        y += tile_h;
    }
    if rects.is_empty() {
        rects.push(Rect {
            pos: dvec2(layer.bounds_x, layer.bounds_y),
            size: dvec2(tile_w, tile_h),
        });
    }
    rects
}

fn background_layer_clip_chain(
    scene: &mut MpScene,
    spatial_id: MpSpatialId,
    clip_chain_id: MpClipChainId,
    layer: &BackgroundLayerGeom,
    radius: f32,
) -> MpClipChainId {
    let rect = background_layer_bounds(layer);
    let clip_id = scene.push_clip(MpClipNode {
        spatial_id,
        kind: if radius > 0.0 {
            MpClipKind::RoundedRect { rect, radius }
        } else {
            MpClipKind::Rect { rect }
        },
    });
    scene.push_clip_chain(MpClipChain {
        parent: Some(clip_chain_id),
        clips: vec![clip_id],
    })
}

fn append_background_layer_primitives(
    scene: &mut MpScene,
    primitives: &mut Vec<MpPrimitive>,
    computed: &ComputedValues,
    background_images: &[havi_fragment_semantics::fragment_tree::BackgroundImage],
    bounds: Rect,
    clip_radius: f32,
    spatial_id: MpSpatialId,
    clip_chain_id: MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
    owner_node_id: Option<usize>,
    current_abs: &AbsoluteColor,
    resources: &mut MpResourceStore,
) -> Result<(), String> {
    use style::values::computed::image::LineDirection;
    use style::values::generics::image::GradientFlags;

    let bg = computed.get_background();
    if bg.background_image.0.is_empty() {
        return Ok(());
    }
    let url_layer_indices: Vec<usize> = bg
        .background_image
        .0
        .iter()
        .enumerate()
        .filter_map(|(index, image)| matches!(image, style::values::computed::image::Image::Url(_)).then_some(index))
        .collect();
    if url_layer_indices.len() != background_images.len() {
        return Err("background image layer count mismatch".to_string());
    }

    let (border_insets, padding_insets) = resolve_insets(computed);
    for (index, image) in bg.background_image.0.iter().enumerate().rev() {
        match image {
            style::values::computed::image::Image::Gradient(gradient) => {
                let Some(layer) = layout_background_layer(
                    computed,
                    index,
                    bounds.pos.x,
                    bounds.pos.y,
                    bounds.size.x as f32,
                    bounds.size.y as f32,
                    &border_insets,
                    &padding_insets,
                    None,
                    None,
                ) else {
                    continue;
                };
                let layer_clip_chain_id = if clip_radius > 0.0
                    || (layer.bounds_w - layer.tile_w).abs() > 0.01
                    || (layer.bounds_h - layer.tile_h).abs() > 0.01
                {
                    background_layer_clip_chain(scene, spatial_id, clip_chain_id, &layer, clip_radius)
                } else {
                    clip_chain_id
                };
                let primitive_kind = match &**gradient {
                    style::values::computed::image::Gradient::Linear {
                        items,
                        direction,
                        flags,
                        ..
                    } => {
                        let (dx, dy) = match direction {
                            LineDirection::Horizontal(h) => {
                                use style::values::specified::position::HorizontalPositionKeyword::*;
                                match h {
                                    Right => (1.0_f32, 0.0),
                                    Left => (-1.0, 0.0),
                                }
                            }
                            LineDirection::Vertical(v) => {
                                use style::values::specified::position::VerticalPositionKeyword::*;
                                match v {
                                    Top => (0.0_f32, -1.0),
                                    Bottom => (0.0, 1.0),
                                }
                            }
                            LineDirection::Angle(angle) => {
                                let radians = angle.radians();
                                (radians.sin(), -radians.cos())
                            }
                            LineDirection::Corner(h, v) => {
                                use style::values::specified::position::HorizontalPositionKeyword::*;
                                use style::values::specified::position::VerticalPositionKeyword::*;
                                let hx = if matches!(h, Right) { 1.0_f32 } else { -1.0 };
                                let vy = if matches!(v, Bottom) { 1.0_f32 } else { -1.0 };
                                let len = (hx * hx + vy * vy).sqrt();
                                (hx / len, vy / len)
                            }
                        };
                        let grad_len = (layer.tile_w * dx).abs() + (layer.tile_h * dy).abs();
                        let half = grad_len / 2.0;
                        makepad_browser_scene::MpPrimitiveKind::LinearGradient(
                            makepad_browser_scene::MpLinearGradient {
                                start: vec2(
                                    0.5 - (dx * half) / layer.tile_w.max(0.001),
                                    0.5 - (dy * half) / layer.tile_h.max(0.001),
                                ),
                                end: vec2(
                                    0.5 + (dx * half) / layer.tile_w.max(0.001),
                                    0.5 + (dy * half) / layer.tile_h.max(0.001),
                                ),
                                repeating: flags.contains(GradientFlags::REPEATING),
                                stops: length_percentage_stops(items, grad_len, current_abs),
                            },
                        )
                    }
                    style::values::computed::image::Gradient::Radial {
                        items,
                        shape,
                        position,
                        flags,
                        ..
                    } => {
                        let center_x = position
                            .horizontal
                            .to_used_value(app_units::Au::from_f32_px(layer.tile_w))
                            .to_f32_px();
                        let center_y = position
                            .vertical
                            .to_used_value(app_units::Au::from_f32_px(layer.tile_h))
                            .to_f32_px();
                        let radius = radial_shape(shape, layer.tile_w, layer.tile_h, center_x, center_y);
                        makepad_browser_scene::MpPrimitiveKind::RadialGradient(
                            makepad_browser_scene::MpRadialGradient {
                                center: vec2(
                                    center_x / layer.tile_w.max(0.001),
                                    center_y / layer.tile_h.max(0.001),
                                ),
                                radius: vec2(
                                    radius.x / layer.tile_w.max(0.001),
                                    radius.y / layer.tile_h.max(0.001),
                                ),
                                repeating: flags.contains(GradientFlags::REPEATING),
                                stops: length_percentage_stops(items, radius.x.max(radius.y), current_abs),
                            },
                        )
                    }
                    style::values::computed::image::Gradient::Conic {
                        angle,
                        position,
                        items,
                        flags,
                        ..
                    } => {
                        let center_x = position
                            .horizontal
                            .to_used_value(app_units::Au::from_f32_px(layer.tile_w))
                            .to_f32_px();
                        let center_y = position
                            .vertical
                            .to_used_value(app_units::Au::from_f32_px(layer.tile_h))
                            .to_f32_px();
                        makepad_browser_scene::MpPrimitiveKind::ConicGradient(
                            makepad_browser_scene::MpConicGradient {
                                center: vec2(
                                    center_x / layer.tile_w.max(0.001),
                                    center_y / layer.tile_h.max(0.001),
                                ),
                                start_angle_rad: angle.radians(),
                                repeating: flags.contains(GradientFlags::REPEATING),
                                stops: angle_percentage_stops(items, current_abs),
                            },
                        )
                    }
                };
                for primitive_bounds in background_layer_tile_rects(&layer) {
                    let mut primitive = MpPrimitive {
                        id: makepad_browser_scene::MpPrimitiveId(0),
                        spatial_id,
                        clip_chain_id: layer_clip_chain_id,
                        effect_id,
                        bounds: primitive_bounds,
                        kind: primitive_kind.clone(),
                        hit_test_tag: owner_node_id.map(|id| MpHitTestTag(id as u64)),
                    };
                    primitive.effect_id = effect_id;
                    primitives.push(primitive);
                }
            }
            style::values::computed::image::Image::Url(_) => {
                let Some(url_position) = url_layer_indices.iter().position(|layer_index| *layer_index == index) else {
                    return Err("background image layer mapping missing".to_string());
                };
                let Some(background_image) = background_images.get(url_position) else {
                    return Err("background image bytes missing".to_string());
                };
                let Some(layer) = layout_background_layer(
                    computed,
                    index,
                    bounds.pos.x,
                    bounds.pos.y,
                    bounds.size.x as f32,
                    bounds.size.y as f32,
                    &border_insets,
                    &padding_insets,
                    Some(background_image.width as f32),
                    Some(background_image.height as f32),
                ) else {
                    continue;
                };
                let layer_clip_chain_id = if clip_radius > 0.0
                    || (layer.bounds_w - layer.tile_w).abs() > 0.01
                    || (layer.bounds_h - layer.tile_h).abs() > 0.01
                {
                    background_layer_clip_chain(scene, spatial_id, clip_chain_id, &layer, clip_radius)
                } else {
                    clip_chain_id
                };
                let (image_key, image_resource) = background_image_resource(owner_node_id, index, background_image);
                resources.images.entry(image_key).or_insert(image_resource);
                for primitive_bounds in background_layer_tile_rects(&layer) {
                    let mut primitive = MpPrimitive {
                        id: makepad_browser_scene::MpPrimitiveId(0),
                        spatial_id,
                        clip_chain_id: layer_clip_chain_id,
                        effect_id,
                        bounds: primitive_bounds,
                        kind: makepad_browser_scene::MpPrimitiveKind::Image(makepad_browser_scene::MpImage {
                            image_key,
                        }),
                        hit_test_tag: owner_node_id.map(|id| MpHitTestTag(id as u64)),
                    };
                    primitive.effect_id = effect_id;
                    primitives.push(primitive);
                }
            }
            _ => return Err("background images not supported by browser-scene adapter yet".to_string()),
        }
    }
    Ok(())
}

fn length_percentage_stops(
    items: &[style::values::generics::image::GradientItem<
        style::values::computed::Color,
        style::values::computed::LengthPercentage,
    >],
    gradient_length: f32,
    current_abs: &AbsoluteColor,
) -> Vec<makepad_browser_scene::MpGradientStop> {
    let mut stops = Vec::new();
    for item in items {
        match item {
            style::values::generics::image::GradientItem::SimpleColorStop(color) => {
                stops.push(makepad_browser_scene::MpGradientStop {
                    color: resolve_color(color, current_abs),
                    offset: -1.0,
                });
            }
            style::values::generics::image::GradientItem::ComplexColorStop { color, position } => {
                stops.push(makepad_browser_scene::MpGradientStop {
                    color: resolve_color(color, current_abs),
                    offset: position
                        .to_used_value(app_units::Au::from_f32_px(gradient_length.max(0.001)))
                        .to_f32_px()
                        / gradient_length.max(0.001),
                });
            }
            style::values::generics::image::GradientItem::InterpolationHint(_) => {}
        }
    }
    normalize_gradient_stops(stops)
}

fn angle_percentage_stops(
    items: &[style::values::generics::image::GradientItem<
        style::values::computed::Color,
        style::values::computed::AngleOrPercentage,
    >],
    current_abs: &AbsoluteColor,
) -> Vec<makepad_browser_scene::MpGradientStop> {
    let mut stops = Vec::new();
    for item in items {
        match item {
            style::values::generics::image::GradientItem::SimpleColorStop(color) => {
                stops.push(makepad_browser_scene::MpGradientStop {
                    color: resolve_color(color, current_abs),
                    offset: -1.0,
                });
            }
            style::values::generics::image::GradientItem::ComplexColorStop { color, position } => {
                let offset = match position {
                    style::values::computed::AngleOrPercentage::Percentage(p) => p.0,
                    style::values::computed::AngleOrPercentage::Angle(angle) => angle.degrees() / 360.0,
                };
                stops.push(makepad_browser_scene::MpGradientStop {
                    color: resolve_color(color, current_abs),
                    offset,
                });
            }
            style::values::generics::image::GradientItem::InterpolationHint(_) => {}
        }
    }
    normalize_gradient_stops(stops)
}

fn normalize_gradient_stops(
    mut stops: Vec<makepad_browser_scene::MpGradientStop>,
) -> Vec<makepad_browser_scene::MpGradientStop> {
    if stops.is_empty() {
        return stops;
    }
    if stops[0].offset < 0.0 {
        stops[0].offset = 0.0;
    }
    let last = stops.len() - 1;
    if stops[last].offset < 0.0 {
        stops[last].offset = 1.0;
    }
    let mut index = 0;
    while index < stops.len() {
        if stops[index].offset < 0.0 {
            let start = index - 1;
            let mut end = index + 1;
            while end < stops.len() && stops[end].offset < 0.0 {
                end += 1;
            }
            let range_start = stops[start].offset;
            let range_end = stops[end].offset;
            let count = end - start;
            for current in (start + 1)..end {
                stops[current].offset = range_start
                    + (range_end - range_start) * ((current - start) as f32) / (count as f32);
            }
            index = end + 1;
        } else {
            index += 1;
        }
    }
    stops
}

fn radial_shape(
    shape: &style::values::computed::image::EndingShape,
    width: f32,
    height: f32,
    center_x: f32,
    center_y: f32,
) -> Vec2f {
    use style::values::generics::image::{Circle, Ellipse};

    match shape {
        style::values::computed::image::EndingShape::Circle(circle) => match circle {
            Circle::Radius(radius) => vec2(radius.px(), radius.px()),
            Circle::Extent(extent) => {
                let radius = match extent {
                    style::values::generics::image::ShapeExtent::ClosestSide => center_x.min(center_y).min(width - center_x).min(height - center_y),
                    style::values::generics::image::ShapeExtent::FarthestSide => center_x.max(center_y).max(width - center_x).max(height - center_y),
                    style::values::generics::image::ShapeExtent::ClosestCorner => {
                        let dx = center_x.min(width - center_x);
                        let dy = center_y.min(height - center_y);
                        (dx * dx + dy * dy).sqrt()
                    }
                    style::values::generics::image::ShapeExtent::FarthestCorner
                    | style::values::generics::image::ShapeExtent::Contain
                    | style::values::generics::image::ShapeExtent::Cover => {
                        let dx = center_x.max(width - center_x);
                        let dy = center_y.max(height - center_y);
                        (dx * dx + dy * dy).sqrt()
                    }
                };
                vec2(radius, radius)
            }
        },
        style::values::computed::image::EndingShape::Ellipse(ellipse) => match ellipse {
            Ellipse::Radii(rx, ry) => vec2(
                rx.to_used_value(app_units::Au::from_f32_px(width)).to_f32_px(),
                ry.to_used_value(app_units::Au::from_f32_px(height)).to_f32_px(),
            ),
            Ellipse::Extent(extent) => {
                let dxc = center_x.min(width - center_x);
                let dyc = center_y.min(height - center_y);
                let dxf = center_x.max(width - center_x);
                let dyf = center_y.max(height - center_y);
                match extent {
                    style::values::generics::image::ShapeExtent::ClosestSide => vec2(dxc, dyc),
                    style::values::generics::image::ShapeExtent::FarthestSide => vec2(dxf, dyf),
                    style::values::generics::image::ShapeExtent::ClosestCorner
                    | style::values::generics::image::ShapeExtent::Contain => {
                        let diagonal = (dxc * dxc + dyc * dyc).sqrt();
                        vec2(
                            if dxc < 0.001 { 0.0 } else { dxc * diagonal / dxc.max(0.001) },
                            if dyc < 0.001 { 0.0 } else { dyc * diagonal / dyc.max(0.001) },
                        )
                    }
                    style::values::generics::image::ShapeExtent::FarthestCorner
                    | style::values::generics::image::ShapeExtent::Cover => {
                        let diagonal = (dxf * dxf + dyf * dyf).sqrt();
                        vec2(
                            if dxf < 0.001 { 0.0 } else { dxf * diagonal / dxf.max(0.001) },
                            if dyf < 0.001 { 0.0 } else { dyf * diagonal / dyf.max(0.001) },
                        )
                    }
                }
            }
        },
    }
}

fn border_paint(computed: &ComputedValues, current_abs: &AbsoluteColor) -> BorderPaint {
    let border = computed.get_border();
    let make_side = |style: BorderStyle,
                     width: style::values::computed::BorderSideWidth,
                     color: &style::values::computed::Color|
     -> Option<BorderSidePaint> {
        let width = width.0.to_f32_px().max(0.0) as f64;
        if width <= 0.0 || matches!(style, BorderStyle::None | BorderStyle::Hidden) {
            return None;
        }
        Some(BorderSidePaint {
            width,
            color: resolve_color(color, current_abs),
            style,
        })
    };

    BorderPaint {
        top: make_side(
            border.clone_border_top_style(),
            border.clone_border_top_width(),
            &border.clone_border_top_color(),
        ),
        right: make_side(
            border.clone_border_right_style(),
            border.clone_border_right_width(),
            &border.clone_border_right_color(),
        ),
        bottom: make_side(
            border.clone_border_bottom_style(),
            border.clone_border_bottom_width(),
            &border.clone_border_bottom_color(),
        ),
        left: make_side(
            border.clone_border_left_style(),
            border.clone_border_left_width(),
            &border.clone_border_left_color(),
        ),
    }
}

fn outline_paint(computed: &ComputedValues, current_abs: &AbsoluteColor) -> Option<OutlinePaint> {
    let outline = computed.get_outline();
    let width = outline.outline_width.0.to_f32_px().max(0.0) as f64;
    if outline.outline_style.none_or_hidden() || width <= 0.0 {
        return None;
    }
    let style = match outline.outline_style {
        style::values::specified::outline::OutlineStyle::Auto => BorderStyle::Solid,
        style::values::specified::outline::OutlineStyle::BorderStyle(style) => style,
    };
    Some(OutlinePaint {
        width,
        offset: outline.outline_offset.to_f32_px() as f64,
        color: resolve_color(&outline.outline_color, current_abs),
        style,
    })
}

fn has_border_paint(border: &BorderPaint) -> bool {
    border.top.is_some() || border.right.is_some() || border.bottom.is_some() || border.left.is_some()
}

fn uniform_rounded_border(border: &BorderPaint) -> Option<(f64, makepad_widgets::Vec4f)> {
    let top = border.top?;
    let right = border.right?;
    let bottom = border.bottom?;
    let left = border.left?;
    if top.style != BorderStyle::Solid
        || right.style != BorderStyle::Solid
        || bottom.style != BorderStyle::Solid
        || left.style != BorderStyle::Solid
    {
        return None;
    }
    if top.width != right.width
        || top.width != bottom.width
        || top.width != left.width
        || top.color != right.color
        || top.color != bottom.color
        || top.color != left.color
    {
        return None;
    }
    Some((top.width, top.color))
}

fn append_border_primitives(
    primitives: &mut Vec<MpPrimitive>,
    bounds: Rect,
    border: &BorderPaint,
    spatial_id: MpSpatialId,
    clip_chain_id: MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
    owner_node_id: Option<usize>,
) {
    if let Some(side) = border.top {
        append_border_side_primitives(
            primitives,
            bounds,
            side,
            BorderSide::Top,
            spatial_id,
            clip_chain_id,
            effect_id,
            owner_node_id,
        );
    }
    if let Some(side) = border.right {
        append_border_side_primitives(
            primitives,
            bounds,
            side,
            BorderSide::Right,
            spatial_id,
            clip_chain_id,
            effect_id,
            owner_node_id,
        );
    }
    if let Some(side) = border.bottom {
        append_border_side_primitives(
            primitives,
            bounds,
            side,
            BorderSide::Bottom,
            spatial_id,
            clip_chain_id,
            effect_id,
            owner_node_id,
        );
    }
    if let Some(side) = border.left {
        append_border_side_primitives(
            primitives,
            bounds,
            side,
            BorderSide::Left,
            spatial_id,
            clip_chain_id,
            effect_id,
            owner_node_id,
        );
    }
}

fn append_outline_primitives(
    primitives: &mut Vec<MpPrimitive>,
    bounds: Rect,
    outline: Option<OutlinePaint>,
    spatial_id: MpSpatialId,
    clip_chain_id: MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
    owner_node_id: Option<usize>,
) {
    let Some(outline) = outline else {
        return;
    };
    let expanded = Rect {
        pos: dvec2(
            bounds.pos.x - outline.offset - outline.width,
            bounds.pos.y - outline.offset - outline.width,
        ),
        size: dvec2(
            bounds.size.x + 2.0 * (outline.offset + outline.width),
            bounds.size.y + 2.0 * (outline.offset + outline.width),
        ),
    };
    let side = BorderSidePaint {
        width: outline.width,
        color: outline.color,
        style: outline.style,
    };
    append_border_side_primitives(
        primitives,
        expanded,
        side,
        BorderSide::Top,
        spatial_id,
        clip_chain_id,
        effect_id,
        owner_node_id,
    );
    append_border_side_primitives(
        primitives,
        expanded,
        side,
        BorderSide::Right,
        spatial_id,
        clip_chain_id,
        effect_id,
        owner_node_id,
    );
    append_border_side_primitives(
        primitives,
        expanded,
        side,
        BorderSide::Bottom,
        spatial_id,
        clip_chain_id,
        effect_id,
        owner_node_id,
    );
    append_border_side_primitives(
        primitives,
        expanded,
        side,
        BorderSide::Left,
        spatial_id,
        clip_chain_id,
        effect_id,
        owner_node_id,
    );
}

fn append_border_side_primitives(
    primitives: &mut Vec<MpPrimitive>,
    bounds: Rect,
    side: BorderSidePaint,
    border_side: BorderSide,
    spatial_id: MpSpatialId,
    clip_chain_id: MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
    owner_node_id: Option<usize>,
) {
    let is_horizontal = matches!(border_side, BorderSide::Top | BorderSide::Bottom);
    let thickness = side.width;
    let push_rect = |primitives: &mut Vec<MpPrimitive>, rect: Rect, color| {
        if rect.size.x <= 0.0 || rect.size.y <= 0.0 {
            return;
        }
        let mut primitive = MpPrimitive::solid_rect(
            makepad_browser_scene::MpPrimitiveId(0),
            spatial_id,
            clip_chain_id,
            rect,
            color,
        );
        primitive.effect_id = effect_id;
        primitive.hit_test_tag = owner_node_id.map(|id| MpHitTestTag(id as u64));
        primitives.push(primitive);
    };
    let rect_for_offset = |offset: f64, length: f64| -> Rect {
        match border_side {
            BorderSide::Top => Rect {
                pos: dvec2(bounds.pos.x + offset, bounds.pos.y),
                size: dvec2(length, thickness),
            },
            BorderSide::Right => Rect {
                pos: dvec2(bounds.pos.x + bounds.size.x - thickness, bounds.pos.y + offset),
                size: dvec2(thickness, length),
            },
            BorderSide::Bottom => Rect {
                pos: dvec2(bounds.pos.x + offset, bounds.pos.y + bounds.size.y - thickness),
                size: dvec2(length, thickness),
            },
            BorderSide::Left => Rect {
                pos: dvec2(bounds.pos.x, bounds.pos.y + offset),
                size: dvec2(thickness, length),
            },
        }
    };
    let side_length = if is_horizontal { bounds.size.x } else { bounds.size.y };

    match side.style {
        BorderStyle::None | BorderStyle::Hidden => {}
        BorderStyle::Solid => {
            push_rect(primitives, rect_for_offset(0.0, side_length), side.color);
        }
        BorderStyle::Double => {
            let line = (thickness / 3.0).max(1.0);
            let outer = match border_side {
                BorderSide::Top => Rect {
                    pos: bounds.pos,
                    size: dvec2(bounds.size.x, line),
                },
                BorderSide::Right => Rect {
                    pos: dvec2(bounds.pos.x + bounds.size.x - thickness, bounds.pos.y),
                    size: dvec2(line, bounds.size.y),
                },
                BorderSide::Bottom => Rect {
                    pos: dvec2(bounds.pos.x, bounds.pos.y + bounds.size.y - line),
                    size: dvec2(bounds.size.x, line),
                },
                BorderSide::Left => Rect {
                    pos: bounds.pos,
                    size: dvec2(line, bounds.size.y),
                },
            };
            let inner = match border_side {
                BorderSide::Top => Rect {
                    pos: dvec2(bounds.pos.x, bounds.pos.y + thickness - line),
                    size: dvec2(bounds.size.x, line),
                },
                BorderSide::Right => Rect {
                    pos: dvec2(bounds.pos.x + bounds.size.x - line, bounds.pos.y),
                    size: dvec2(line, bounds.size.y),
                },
                BorderSide::Bottom => Rect {
                    pos: dvec2(bounds.pos.x, bounds.pos.y + bounds.size.y - thickness),
                    size: dvec2(bounds.size.x, line),
                },
                BorderSide::Left => Rect {
                    pos: dvec2(bounds.pos.x + thickness - line, bounds.pos.y),
                    size: dvec2(line, bounds.size.y),
                },
            };
            push_rect(primitives, outer, side.color);
            push_rect(primitives, inner, side.color);
        }
        BorderStyle::Dotted | BorderStyle::Dashed => {
            let segment = if side.style == BorderStyle::Dotted {
                thickness.max(1.0)
            } else {
                (thickness * 3.0).max(1.0)
            };
            let count = (side_length / (segment * 2.0)).max(1.0) as i32;
            let spacing = side_length / count as f64;
            let drawn = segment.min(spacing * 0.5);
            for index in 0..count {
                push_rect(
                    primitives,
                    rect_for_offset(index as f64 * spacing, drawn),
                    side.color,
                );
            }
        }
        BorderStyle::Groove => {
            let half = (thickness / 2.0).max(0.5);
            let (first, second) = groove_colors(side.color, border_side);
            push_rect(primitives, rect_for_split(bounds, border_side, 0.0, half), first);
            push_rect(primitives, rect_for_split(bounds, border_side, half, thickness - half), second);
        }
        BorderStyle::Ridge => {
            let half = (thickness / 2.0).max(0.5);
            let (dark, light) = groove_colors(side.color, border_side);
            push_rect(primitives, rect_for_split(bounds, border_side, 0.0, half), light);
            push_rect(primitives, rect_for_split(bounds, border_side, half, thickness - half), dark);
        }
        BorderStyle::Inset => {
            let color = if matches!(border_side, BorderSide::Top | BorderSide::Left) {
                darken(side.color, 0.6)
            } else {
                side.color
            };
            push_rect(primitives, rect_for_offset(0.0, side_length), color);
        }
        BorderStyle::Outset => {
            let color = if matches!(border_side, BorderSide::Bottom | BorderSide::Right) {
                darken(side.color, 0.6)
            } else {
                side.color
            };
            push_rect(primitives, rect_for_offset(0.0, side_length), color);
        }
    }
}

fn rect_for_split(bounds: Rect, border_side: BorderSide, offset: f64, thickness: f64) -> Rect {
    match border_side {
        BorderSide::Top => Rect {
            pos: dvec2(bounds.pos.x, bounds.pos.y + offset),
            size: dvec2(bounds.size.x, thickness),
        },
        BorderSide::Right => Rect {
            pos: dvec2(bounds.pos.x + bounds.size.x - offset - thickness, bounds.pos.y),
            size: dvec2(thickness, bounds.size.y),
        },
        BorderSide::Bottom => Rect {
            pos: dvec2(bounds.pos.x, bounds.pos.y + bounds.size.y - offset - thickness),
            size: dvec2(bounds.size.x, thickness),
        },
        BorderSide::Left => Rect {
            pos: dvec2(bounds.pos.x + offset, bounds.pos.y),
            size: dvec2(thickness, bounds.size.y),
        },
    }
}

fn groove_colors(color: makepad_widgets::Vec4f, border_side: BorderSide) -> (makepad_widgets::Vec4f, makepad_widgets::Vec4f) {
    let dark = darken(color, 0.6);
    let light = lighten(color, 1.4);
    match border_side {
        BorderSide::Top | BorderSide::Left => (dark, light),
        BorderSide::Bottom | BorderSide::Right => (light, dark),
    }
}

fn darken(color: makepad_widgets::Vec4f, factor: f32) -> makepad_widgets::Vec4f {
    makepad_widgets::Vec4f {
        x: color.x * factor,
        y: color.y * factor,
        z: color.z * factor,
        w: color.w,
    }
}

fn lighten(color: makepad_widgets::Vec4f, factor: f32) -> makepad_widgets::Vec4f {
    makepad_widgets::Vec4f {
        x: (color.x * factor).min(1.0),
        y: (color.y * factor).min(1.0),
        z: (color.z * factor).min(1.0),
        w: color.w,
    }
}

fn make_glyph_run_resource(
    cx: &mut Cx2d,
    owner_node_id: Option<usize>,
    bounds: Rect,
    tf: &TextFragment,
) -> Result<(MpGlyphRunKey, MpGlyphRunResource), String> {
    if tf.glyphs.is_empty() {
        return Err("unshaped text not supported by browser-scene adapter yet".to_string());
    }
    let (font_key, _) = font_resource_for_text(cx, tf)?;
    let glyph_run_key = MpGlyphRunKey(hash_value(&(
        owner_node_id,
        tf.text.as_str(),
        tf.base.rect.origin.x.0,
        tf.base.rect.origin.y.0,
        tf.base.rect.size.width.0,
        tf.base.rect.size.height.0,
    )));

    let mut pen_x = 0.0_f64;
    let mut advance_width = 0.0_f32;
    let glyphs = tf
        .glyphs
        .iter()
        .map(|glyph| {
            let origin = dvec2(
                pen_x + glyph.x_offset.to_f32_px() as f64,
                tf.baseline_ascent.to_f32_px() as f64 + glyph.y_offset.to_f32_px() as f64,
            );
            pen_x += glyph.advance.to_f32_px() as f64;
            advance_width = advance_width.max((origin.x + glyph.advance.to_f32_px() as f64) as f32);
            MpPositionedGlyph {
                glyph_id: glyph.glyph_id,
                font_size_px: tf.font_size_px,
                origin,
                font_slot: 0,
            }
        })
        .collect();

    let computed = &tf.base.style;
    let current = inherited_color(computed);
    let current_abs = AbsoluteColor::new(ColorSpace::Srgb, current.x, current.y, current.z, current.w);
    let background = resolve_color(
        &computed.get_background().background_color,
        &computed.get_inherited_text().color,
    );
    let decoration_color = resolve_color(&computed.get_text().clone_text_decoration_color(), &current_abs);
    let line = computed.get_text().clone_text_decoration_line();
    let shadows = computed
        .get_inherited_text()
        .text_shadow
        .0
        .iter()
        .map(|shadow| MpTextShadow {
            offset: dvec2(shadow.horizontal.px() as f64, shadow.vertical.px() as f64),
            blur_radius_px: shadow.blur.px(),
            color: resolve_color(
                &shadow.color,
                &computed.get_inherited_text().color,
            ),
        })
        .collect();

    Ok((
        glyph_run_key,
        MpGlyphRunResource {
            text: tf.text.clone(),
            font_keys: vec![font_key],
            glyphs,
            metrics: MpGlyphRunMetrics {
                advance_width_px: advance_width.min(bounds.size.x as f32),
                baseline_ascent_px: tf.baseline_ascent.to_f32_px(),
                underline_offset_px: tf.underline_offset.to_f32_px(),
                underline_thickness_px: tf.underline_size.to_f32_px(),
                strikeout_offset_px: tf.strikeout_offset.to_f32_px(),
                strikeout_thickness_px: tf.strikeout_size.to_f32_px(),
            },
            decorations: MpTextDecorations {
                background_color: (background.w > 0.001).then_some(background),
                decoration_color: Some(decoration_color),
                underline: line.contains(TextDecorationLine::UNDERLINE),
                overline: line.contains(TextDecorationLine::OVERLINE),
                line_through: line.contains(TextDecorationLine::LINE_THROUGH),
                shadows,
            },
        },
    ))
}

fn background_image_resource(
    owner_node_id: Option<usize>,
    layer_index: usize,
    bg: &havi_fragment_semantics::fragment_tree::BackgroundImage,
) -> (makepad_browser_scene::MpImageKey, makepad_browser_scene::MpImageResource) {
    let key = makepad_browser_scene::MpImageKey(hash_value(&(
        "bg",
        owner_node_id,
        layer_index,
        bg.width,
        bg.height,
        &bg.pixels,
    )));
    (
        key,
        makepad_browser_scene::MpImageResource {
            size: dvec2(bg.width as f64, bg.height as f64),
            rgba8: bg.pixels.clone(),
        },
    )
}

fn image_resource_for_fragment(image: &ImageFragment) -> (makepad_browser_scene::MpImageKey, makepad_browser_scene::MpImageResource) {
    let bytes = image.image_data[image.frame_byte_range.clone()].to_vec();
    let key = if let Some(image_key) = image.image_key {
        makepad_browser_scene::MpImageKey(hash_value(&image_key))
    } else {
        makepad_browser_scene::MpImageKey(hash_value(&(
            image.frame_width,
            image.frame_height,
            image.frame_byte_range.start,
            image.frame_byte_range.end,
            &bytes,
        )))
    };
    (
        key,
        makepad_browser_scene::MpImageResource {
            size: dvec2(image.frame_width as f64, image.frame_height as f64),
            rgba8: bytes,
        },
    )
}

fn font_resource_for_text(
    cx: &mut Cx2d,
    tf: &TextFragment,
) -> Result<(MpFontKey, MpFontResource), String> {
    if let Some(handle) = &tf.font_handle {
        let key = MpFontKey(hash_value(&(handle.path.as_os_str(), handle.index)));
        let bytes = if let Some(data) = tf.font_data.as_ref() {
            (**data).clone()
        } else {
            let fonts = cx
                .cx
                .get_global::<Rc<RefCell<havi_fonts::HaviFonts>>>()
                .clone();
            let data = fonts
                .borrow_mut()
                .load_data(handle)
                .ok_or_else(|| "failed to load font data".to_string())?;
            (*data).clone()
        };
        return Ok((
            key,
            MpFontResource {
                bytes,
                face_index: handle.index,
            },
        ));
    }

    Err("text fragment missing font handle".to_string())
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

fn hash_value<T: Hash>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_layer_tile_rects_repeat_across_bounds() {
        let rects = background_layer_tile_rects(&BackgroundLayerGeom {
            bounds_x: 10.0,
            bounds_y: 20.0,
            bounds_w: 50.0,
            bounds_h: 30.0,
            tile_w: 20.0,
            tile_h: 10.0,
        });

        assert_eq!(rects.len(), 9);
        assert_eq!(rects[0].pos, dvec2(10.0, 20.0));
        assert_eq!(rects[1].pos, dvec2(30.0, 20.0));
        assert_eq!(rects[2].pos, dvec2(50.0, 20.0));
        assert_eq!(rects[3].pos, dvec2(10.0, 30.0));
        assert_eq!(rects[8].pos, dvec2(50.0, 40.0));
    }

    #[test]
    fn background_image_resource_key_tracks_pixels() {
        let a = havi_fragment_semantics::fragment_tree::BackgroundImage {
            width: 1,
            height: 1,
            pixels: vec![0, 0, 0, 255],
        };
        let b = havi_fragment_semantics::fragment_tree::BackgroundImage {
            width: 1,
            height: 1,
            pixels: vec![255, 255, 255, 255],
        };

        let (key_a, _) = background_image_resource(Some(7), 0, &a);
        let (key_b, _) = background_image_resource(Some(7), 0, &b);

        assert_ne!(key_a, key_b);
    }
}
