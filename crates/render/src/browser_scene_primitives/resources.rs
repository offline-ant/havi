use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use havi_types::fragment_tree::{
    FragmentImageKey, ImageFragment, ImageSourceKind, TextFragment,
};
use makepad_browser_scene::{
    MpFontKey, MpFontResource, MpImageKey, MpImageSource, MpRendererImageHandle,
    MpRendererImageProducer, ResourceRegistry,
};
use makepad_widgets::dvec2;

pub(super) fn ensure_background_image_resource(
    registry: &mut ResourceRegistry,
    owner_node_id: Option<usize>,
    layer_index: usize,
    bg: &havi_types::BackgroundImage,
) -> MpImageKey {
    let key = background_image_key(owner_node_id, layer_index, bg);
    if !registry.images.contains_key(&key) {
        registry.upsert_image(key, materialize_background_image_resource(bg));
    }
    key
}

pub(super) fn background_image_key(
    owner_node_id: Option<usize>,
    layer_index: usize,
    bg: &havi_types::BackgroundImage,
) -> MpImageKey {
    if let Some(image_key) = bg.image_key {
        return MpImageKey(image_key.packed());
    }
    MpImageKey(hash_value(&(
        owner_node_id,
        layer_index,
        bg.source_kind,
        bg.revision,
        bg.width,
        bg.height,
        bg.byte_range.start,
        bg.byte_range.end,
        bg.data.as_ptr() as usize,
    )))
}

pub(super) fn materialize_background_image_resource(bg: &havi_types::BackgroundImage) -> MpImageSource {
    let size = dvec2(bg.width as f64, bg.height as f64);
    let bytes = &bg.data[bg.byte_range.clone()];
    MpImageSource::decoded_rgba8(size, Arc::from(bytes), bg.revision)
}

pub(super) fn ensure_image_resource_for_fragment(
    registry: &mut ResourceRegistry,
    image: &ImageFragment,
) -> MpImageKey {
    let key = image_key_for_fragment(image);
    if !registry.images.contains_key(&key) {
        registry.upsert_image(key, materialize_image_resource(image));
    }
    key
}

pub(super) fn image_key_for_fragment(image: &ImageFragment) -> MpImageKey {
    if let Some(image_key) = image.image_key {
        return MpImageKey(image_key.packed());
    }
    MpImageKey(hash_value(&(
        image.source_kind,
        image.image_revision,
        image.frame_width,
        image.frame_height,
        image.frame_byte_range.start,
        image.frame_byte_range.end,
        image.image_data.as_ptr() as usize,
    )))
}

pub(super) fn materialize_image_resource(image: &ImageFragment) -> MpImageSource {
    let size = dvec2(image.frame_width as f64, image.frame_height as f64);
    let bytes = &image.image_data[image.frame_byte_range.clone()];
    if bytes.is_empty() {
        if let Some(image_key) = image.image_key {
            let producer = match image.source_kind {
                ImageSourceKind::Raster => {
                    MpRendererImageProducer::PaintExternalImage {
                        handle: renderer_image_handle(image_key),
                    }
                }
                ImageSourceKind::Canvas => MpRendererImageProducer::CanvasImage {
                    handle: renderer_image_handle(image_key),
                },
                ImageSourceKind::Video => MpRendererImageProducer::VideoBinding {
                    handle: renderer_image_handle(image_key),
                },
                ImageSourceKind::NativeSvg => unreachable!(),
            };
            return MpImageSource::renderer_backed(size, producer, 0);
        }
    }
    MpImageSource::decoded_rgba8(size, Arc::from(bytes), image.image_revision)
}

fn renderer_image_handle(image_key: FragmentImageKey) -> MpRendererImageHandle {
    MpRendererImageHandle {
        namespace: image_key.namespace,
        image: image_key.image,
    }
}

pub(super) fn ensure_font_resource(
    registry: &mut ResourceRegistry,
    tf: &TextFragment,
) -> Result<MpFontKey, String> {
    ensure_font_resource_from_parts(registry, tf.font_data.as_ref(), tf.font_index, "text fragment")
}

fn ensure_font_resource_from_parts(
    registry: &mut ResourceRegistry,
    font_data: Option<&Arc<Vec<u8>>>,
    font_index: u32,
    missing_label: &str,
) -> Result<MpFontKey, String> {
    let key = font_key_for_parts(font_data, font_index, missing_label)?;
    if !registry.fonts.contains_key(&key) {
        registry.upsert_font(
            key,
            materialize_font_resource_from_parts(font_data, font_index, missing_label)?,
        );
    }
    Ok(key)
}

fn font_key_for_parts(
    font_data: Option<&Arc<Vec<u8>>>,
    font_index: u32,
    missing_label: &str,
) -> Result<MpFontKey, String> {
    let Some(font_data) = font_data else {
        return Err(format!("{missing_label} missing font data"));
    };
    Ok(MpFontKey(hash_value(&(font_data.as_ptr() as usize, font_data.len(), font_index))))
}

fn materialize_font_resource_from_parts(
    font_data: Option<&Arc<Vec<u8>>>,
    font_index: u32,
    missing_label: &str,
) -> Result<MpFontResource, String> {
    let Some(font_data) = font_data else {
        return Err(format!("{missing_label} missing font data"));
    };
    Ok(MpFontResource {
        bytes: Arc::from(font_data.as_slice()),
        face_index: font_index,
    })
}

pub(super) fn hash_value<T: Hash>(value: T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}
