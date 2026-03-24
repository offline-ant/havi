mod box_background;
mod box_border;
mod box_shadow;
mod gradient;
mod resources;
mod text;

use havi_fragment_semantics::fragment_tree::BackgroundImage;
use havi_fragment_semantics::Fragment;
use makepad_browser_scene::{MpClipChainId, MpHitTestTag, MpPrimitive, MpResourceStore, MpScene};
use makepad_widgets::{dvec2, Cx2d, Rect};
use style::color::{AbsoluteColor, ColorSpace};
use style::properties::ComputedValues;

use self::box_background::{append_box_background_primitives, has_unsupported_background_layers};
use self::box_border::{append_box_border_primitives, border_paint, border_radius, outline_paint};
use self::box_shadow::append_box_shadow_primitives;
use self::resources::image_resource_for_fragment;
use self::text::lower_text_primitive;
use crate::color::inherited_color;
use crate::layout_stacking_context::StackingContextSection;
use crate::paint_items::RenderPaintItem;

pub(crate) fn paint_run_item_to_primitives(
    cx: &mut Cx2d,
    scene: &mut MpScene,
    resources: &mut MpResourceStore,
    item: &RenderPaintItem<'_>,
    run_owner_node_id: Option<usize>,
    spatial_id: makepad_browser_scene::MpSpatialId,
    clip_chain_id: MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
) -> Result<Vec<MpPrimitive>, String> {
    let bounds = paint_item_bounds(item);
    let owner_node_id = paint_item_owner_node_id(item.source).or(run_owner_node_id);
    match (item.section, item.source) {
        (StackingContextSection::OwnBackgroundsAndBorders, Fragment::Box(bf))
        | (StackingContextSection::OwnBackgroundsAndBorders, Fragment::Float(bf)) => lower_box_primitives(
            scene,
            resources,
            bounds,
            &bf.base.style,
            &bf.background_images,
            spatial_id,
            clip_chain_id,
            effect_id,
            owner_node_id,
        ),
        (StackingContextSection::Foreground, Fragment::IFrame(iframe)) => lower_box_primitives(
            scene,
            resources,
            bounds,
            &iframe.base.style,
            &[],
            spatial_id,
            clip_chain_id,
            effect_id,
            owner_node_id,
        ),
        (StackingContextSection::Foreground, Fragment::Text(tf)) => lower_text_primitive(
            cx,
            resources,
            owner_node_id,
            bounds,
            tf,
            spatial_id,
            clip_chain_id,
            effect_id,
        )
        .map(|primitive| vec![primitive]),
        (StackingContextSection::Foreground, Fragment::Image(image)) => {
            let (image_key, image_resource) = image_resource_for_fragment(image);
            resources.images.entry(image_key).or_insert(image_resource);
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

fn lower_box_primitives(
    scene: &mut MpScene,
    resources: &mut MpResourceStore,
    bounds: Rect,
    computed: &ComputedValues,
    background_images: &[Option<BackgroundImage>],
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
        resources,
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

fn paint_item_bounds(item: &RenderPaintItem<'_>) -> Rect {
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
