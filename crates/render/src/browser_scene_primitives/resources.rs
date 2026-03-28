use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use havi_types::fragment_tree::{FragmentImageKey, ImageFragment, ImageSourceKind, TextFragment};
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
    MpImageKey(hash_value(&(
        owner_node_id,
        layer_index,
        bg.width,
        bg.height,
        bg.pixels.as_ptr() as usize,
        bg.pixels.len(),
    )))
}

pub(super) fn materialize_background_image_resource(bg: &havi_types::BackgroundImage) -> MpImageSource {
    MpImageSource::decoded_rgba8(
        dvec2(bg.width as f64, bg.height as f64),
        Arc::from(bg.pixels.as_slice()),
        0,
    )
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
            };
            return MpImageSource::renderer_backed(size, producer, 0);
        }
    }
    MpImageSource::decoded_rgba8(size, Arc::from(bytes), 0)
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
    let key = font_key_for_text(tf)?;
    if !registry.fonts.contains_key(&key) {
        registry.upsert_font(key, materialize_font_resource(tf)?);
    }
    Ok(key)
}

pub(super) fn font_key_for_text(tf: &TextFragment) -> Result<MpFontKey, String> {
    let Some(font_data) = tf.font_data.as_ref() else {
        return Err("text fragment missing font data".to_string());
    };
    Ok(MpFontKey(hash_value(&(font_data.as_ptr() as usize, font_data.len(), tf.font_index))))
}

pub(super) fn materialize_font_resource(tf: &TextFragment) -> Result<MpFontResource, String> {
    let Some(font_data) = tf.font_data.as_ref() else {
        return Err("text fragment missing font data".to_string());
    };
    Ok(MpFontResource {
        bytes: Arc::from(font_data.as_slice()),
        face_index: tf.font_index,
    })
}

pub(super) fn hash_value<T: Hash>(value: T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}
