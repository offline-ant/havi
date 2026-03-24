use havi_fragment_semantics::fragment_tree::BackgroundImage;
use makepad_browser_scene::{
    MpClipChain, MpClipChainId, MpClipKind, MpClipNode, MpHitTestTag, MpPerCornerRadius,
    MpPrimitive, MpResourceStore, MpScene,
};
use makepad_widgets::{dvec2, vec2, Rect};
use style::color::AbsoluteColor;
use style::properties::ComputedValues;

use super::gradient::{angle_percentage_stops, length_percentage_stops, radial_shape};
use super::resources::background_image_resource;
use crate::background::{BackgroundLayerGeom, layout_background_layer, resolve_insets};
use crate::color::resolve_color;

pub(super) fn has_unsupported_background_layers(computed: &ComputedValues) -> bool {
    use style::values::computed::image::Image;

    computed
        .get_background()
        .background_image
        .0
        .iter()
        .any(|image| !matches!(image, Image::None | Image::Gradient(_) | Image::Url(_)))
}

pub(super) fn append_box_background_primitives(
    scene: &mut MpScene,
    primitives: &mut Vec<MpPrimitive>,
    computed: &ComputedValues,
    background_images: &[Option<BackgroundImage>],
    bounds: Rect,
    clip_radius: MpPerCornerRadius,
    spatial_id: makepad_browser_scene::MpSpatialId,
    clip_chain_id: MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
    owner_node_id: Option<usize>,
    current_abs: &AbsoluteColor,
    resources: &mut MpResourceStore,
) -> Result<(), String> {
    let background_color = resolve_color(&computed.get_background().background_color, current_abs);
    if background_color.w > 0.001 {
        let mut primitive = if clip_radius.max() > 0.0 {
            MpPrimitive::rounded_rect(
                makepad_browser_scene::MpPrimitiveId(0),
                spatial_id,
                clip_chain_id,
                bounds,
                background_color,
                clip_radius,
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
        primitives,
        computed,
        background_images,
        bounds,
        clip_radius,
        spatial_id,
        clip_chain_id,
        effect_id,
        owner_node_id,
        current_abs,
        resources,
    )
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
    spatial_id: makepad_browser_scene::MpSpatialId,
    clip_chain_id: MpClipChainId,
    layer: &BackgroundLayerGeom,
    radius: MpPerCornerRadius,
) -> MpClipChainId {
    let rect = background_layer_bounds(layer);
    let clip_id = scene.push_clip(MpClipNode {
        spatial_id,
        kind: if radius.max() > 0.0 {
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
    background_images: &[Option<BackgroundImage>],
    bounds: Rect,
    clip_radius: MpPerCornerRadius,
    spatial_id: makepad_browser_scene::MpSpatialId,
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
    let (border_insets, padding_insets) = resolve_insets(computed);
    for (index, image) in bg.background_image.0.iter().enumerate().rev() {
        match image {
            style::values::computed::image::Image::None => continue,
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
                let layer_clip_chain_id = if clip_radius.max() > 0.0
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
                let Some(background_image) = background_images.get(index).and_then(|image| image.as_ref()) else {
                    continue;
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
                let layer_clip_chain_id = if clip_radius.max() > 0.0
                    || (layer.bounds_w - layer.tile_w).abs() > 0.01
                    || (layer.bounds_h - layer.tile_h).abs() > 0.01
                {
                    background_layer_clip_chain(scene, spatial_id, clip_chain_id, &layer, clip_radius)
                } else {
                    clip_chain_id
                };
                let (image_key, image_resource) =
                    background_image_resource(owner_node_id, index, background_image);
                resources.images.entry(image_key).or_insert(image_resource);
                let primitive_kind = if (layer.bounds_w - layer.tile_w).abs() > 0.01
                    || (layer.bounds_h - layer.tile_h).abs() > 0.01
                {
                    makepad_browser_scene::MpPrimitiveKind::RepeatingImage(
                        makepad_browser_scene::MpRepeatingImage { image_key },
                    )
                } else {
                    makepad_browser_scene::MpPrimitiveKind::Image(makepad_browser_scene::MpImage {
                        image_key,
                    })
                };
                let mut primitive = MpPrimitive {
                    id: makepad_browser_scene::MpPrimitiveId(0),
                    spatial_id,
                    clip_chain_id: layer_clip_chain_id,
                    effect_id,
                    bounds: background_layer_bounds(&layer),
                    kind: primitive_kind,
                    hit_test_tag: owner_node_id.map(|id| MpHitTestTag(id as u64)),
                };
                primitive.effect_id = effect_id;
                primitives.push(primitive);
            }
            _ => return Err("background images not supported by browser-scene adapter yet".to_string()),
        }
    }
    Ok(())
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
}
