mod box_background;
mod box_border;
mod box_shadow;
mod gradient;
mod resources;
mod text;

use std::collections::HashMap;

use havi_types::fragment_tree as published;
use makepad_browser_scene::{
    MpClipChainId, MpGlyphRunKey, MpGlyphRunResource, MpHitTestTag, MpPrimitive, MpScene,
    ResourceRegistry,
};
use makepad_widgets::{dvec2, Cx2d, Rect};
use style::color::{AbsoluteColor, ColorSpace};
use style::properties::ComputedValues;

use self::box_background::{append_box_background_primitives, has_unsupported_background_layers};
use self::box_border::{append_box_border_primitives, border_paint, border_radius, outline_paint};
use self::box_shadow::append_box_shadow_primitives;
use self::resources::ensure_image_resource_for_fragment;
use self::text::lower_text_primitive;
use crate::color::inherited_color;
use crate::layout_stacking_context::StackingContextSection;
use crate::paint_items::RenderPaintItem;

pub(crate) fn paint_run_item_to_primitives(
    _cx: &mut Cx2d,
    generation: &published::FragmentArenaGeneration,
    scene: &mut MpScene,
    registry: &mut ResourceRegistry,
    glyph_runs: &mut HashMap<MpGlyphRunKey, MpGlyphRunResource>,
    item: &RenderPaintItem,
    run_owner_node_id: Option<usize>,
    spatial_id: makepad_browser_scene::MpSpatialId,
    clip_chain_id: MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
) -> Result<Vec<MpPrimitive>, String> {
    let bounds = paint_item_bounds(generation, item);
    let owner_node_id = paint_item_owner_node_id(generation, item.fragment_id).or(run_owner_node_id);
    match (item.section, generation.kind(item.fragment_id)) {
        (StackingContextSection::OwnBackgroundsAndBorders, published::FragmentKind::Box(bf))
        | (StackingContextSection::OwnBackgroundsAndBorders, published::FragmentKind::Float(bf)) => {
            lower_box_primitives(
                scene,
                registry,
                bounds,
                &bf.base.style,
                generation.background_images_for(item.fragment_id),
                spatial_id,
                clip_chain_id,
                effect_id,
                owner_node_id,
            )
        }
        (StackingContextSection::Foreground, published::FragmentKind::IFrame(iframe)) => {
            lower_box_primitives(
                scene,
                registry,
                bounds,
                &iframe.base.style,
                &[],
                spatial_id,
                clip_chain_id,
                effect_id,
                owner_node_id,
            )
        }
        (StackingContextSection::Foreground, published::FragmentKind::Text(tf)) => {
            lower_text_primitive(
                registry,
                glyph_runs,
                owner_node_id,
                bounds,
                tf,
                spatial_id,
                clip_chain_id,
                effect_id,
            )
            .map(|primitive| vec![primitive])
        }
        (StackingContextSection::Foreground, published::FragmentKind::Image(image)) => {
            let image_key = ensure_image_resource_for_fragment(registry, image);
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
        (StackingContextSection::Foreground, published::FragmentKind::SVGPath(_)) |
        (StackingContextSection::Foreground, published::FragmentKind::SVGText(_)) |
        (StackingContextSection::Foreground, published::FragmentKind::SVGImage(_)) |
        (StackingContextSection::Foreground, published::FragmentKind::SVGViewport(_)) |
        (StackingContextSection::Foreground, published::FragmentKind::SVGGroup(_)) |
        (StackingContextSection::Foreground, published::FragmentKind::SVGForeignObject(_)) => {
            Err("SVG paint run not supported by browser-scene adapter yet".to_string())
        }
        _ => Err("paint run not supported by browser-scene adapter yet".to_string()),
    }
}

fn lower_box_primitives(
    scene: &mut MpScene,
    registry: &mut ResourceRegistry,
    bounds: Rect,
    computed: &ComputedValues,
    background_images: &[Option<havi_types::BackgroundImage>],
    spatial_id: makepad_browser_scene::MpSpatialId,
    clip_chain_id: MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
    owner_node_id: Option<usize>,
) -> Result<Vec<MpPrimitive>, String> {
    if has_unsupported_background_layers(computed) {
        return Err("background images not supported by browser-scene adapter yet".to_string());
    }
    let current = inherited_color(computed);
    let current_abs = AbsoluteColor::new(ColorSpace::Srgb, current.x, current.y, current.z, current.w);
    let border = border_paint(computed, &current_abs);
    let outline = outline_paint(computed, &current_abs);
    let radius = border_radius(computed);

    let mut primitives = Vec::new();
    append_box_shadow_primitives(
        &mut primitives,
        computed,
        bounds,
        radius.max(),
        spatial_id,
        clip_chain_id,
        effect_id,
        owner_node_id,
        &current_abs,
    );
    append_box_background_primitives(
        scene,
        &mut primitives,
        computed,
        background_images,
        bounds,
        radius,
        spatial_id,
        clip_chain_id,
        effect_id,
        owner_node_id,
        &current_abs,
        registry,
    )?;
    append_box_border_primitives(
        &mut primitives,
        bounds,
        radius,
        &border,
        outline,
        spatial_id,
        clip_chain_id,
        effect_id,
        owner_node_id,
    )?;
    Ok(primitives)
}

fn paint_item_owner_node_id(
    generation: &published::FragmentArenaGeneration,
    fragment_id: published::FragmentId,
) -> Option<usize> {
    generation.base(fragment_id).tag.map(|tag| tag.node.0)
}

fn paint_item_bounds(
    generation: &published::FragmentArenaGeneration,
    item: &RenderPaintItem,
) -> Rect {
    let rect = match generation.kind(item.fragment_id) {
        published::FragmentKind::Box(bf) | published::FragmentKind::Float(bf) => {
            physical_rect_to_rect(bf.border_rect())
        }
        published::FragmentKind::Text(tf) => physical_rect_to_rect(tf.base.rect),
        published::FragmentKind::Image(image) => physical_rect_to_rect(image.base.rect),
        published::FragmentKind::IFrame(iframe) => physical_rect_to_rect(iframe.base.rect),
        published::FragmentKind::SVGViewport(svg) => physical_rect_to_rect(svg.base.rect),
        published::FragmentKind::SVGGroup(svg) => physical_rect_to_rect(svg.base.rect),
        published::FragmentKind::SVGPath(svg) => physical_rect_to_rect(svg.base.rect),
        published::FragmentKind::SVGText(svg) => physical_rect_to_rect(svg.base.rect),
        published::FragmentKind::SVGForeignObject(svg) => physical_rect_to_rect(svg.base.rect),
        published::FragmentKind::SVGImage(svg) => physical_rect_to_rect(svg.base.rect),
        published::FragmentKind::Positioning(_) => Rect {
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
