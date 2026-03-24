use std::collections::hash_map::{DefaultHasher, Entry};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use layout::fragment_tree::{ImageFragment, TextFragment};
use makepad_browser_scene::{MpFontKey, MpFontResource, MpImageKey, MpImageResource, ResourceRegistry};
use makepad_widgets::dvec2;
use pixels::RasterImage;

pub(super) fn ensure_background_image_resource(
    registry: &mut ResourceRegistry,
    owner_node_id: Option<usize>,
    layer_index: usize,
    bg: &Arc<RasterImage>,
) -> MpImageKey {
    let key = background_image_key(owner_node_id, layer_index, bg);
    if let Entry::Vacant(entry) = registry.images.entry(key) {
        entry.insert(materialize_background_image_resource(bg));
    }
    key
}

pub(super) fn background_image_key(
    owner_node_id: Option<usize>,
    layer_index: usize,
    bg: &Arc<RasterImage>,
) -> MpImageKey {
    let cache_key = bg.id.map(hash_value).unwrap_or_else(|| {
        hash_value(&(
            owner_node_id,
            layer_index,
            bg.metadata.width,
            bg.metadata.height,
            bg.bytes.as_ptr() as usize,
            bg.bytes.len(),
        ))
    });
    MpImageKey(cache_key)
}

pub(super) fn materialize_background_image_resource(bg: &Arc<RasterImage>) -> MpImageResource {
    MpImageResource {
        size: dvec2(bg.metadata.width as f64, bg.metadata.height as f64),
        rgba8: Arc::from(bg.bytes.as_ref().as_slice()),
    }
}

pub(super) fn ensure_image_resource_for_fragment(
    registry: &mut ResourceRegistry,
    image: &ImageFragment,
) -> MpImageKey {
    let key = image_key_for_fragment(image);
    if let Entry::Vacant(entry) = registry.images.entry(key) {
        entry.insert(materialize_image_resource(image));
    }
    key
}

pub(super) fn image_key_for_fragment(image: &ImageFragment) -> MpImageKey {
    if let Some(image_key) = image.image_key {
        return MpImageKey(hash_value(&image_key));
    }

    let Some(raster_image) = image.raster_image.as_ref() else {
        return MpImageKey(hash_value(&(0u8, image.base.rect.origin.x.0, image.base.rect.origin.y.0)));
    };
    let (frame_width, frame_height, frame_byte_range) = image_frame(raster_image);
    MpImageKey(hash_value(&(
        frame_width,
        frame_height,
        frame_byte_range.start,
        frame_byte_range.end,
        raster_image.bytes.as_ptr() as usize,
    )))
}

pub(super) fn materialize_image_resource(image: &ImageFragment) -> MpImageResource {
    let Some(raster_image) = image.raster_image.as_ref() else {
        return MpImageResource {
            size: dvec2(0.0, 0.0),
            rgba8: Arc::from(&[][..]),
        };
    };
    let (frame_width, frame_height, frame_byte_range) = image_frame(raster_image);
    let bytes = &raster_image.bytes[frame_byte_range];
    MpImageResource {
        size: dvec2(frame_width as f64, frame_height as f64),
        rgba8: Arc::from(bytes),
    }
}

pub(super) fn ensure_font_resource(
    registry: &mut ResourceRegistry,
    tf: &TextFragment,
) -> Result<MpFontKey, String> {
    let key = font_key_for_text(tf)?;
    if let Entry::Vacant(entry) = registry.fonts.entry(key) {
        entry.insert(materialize_font_resource(tf)?);
    }
    Ok(key)
}

pub(super) fn font_key_for_text(tf: &TextFragment) -> Result<MpFontKey, String> {
    Ok(MpFontKey(hash_value(tf.font.identifier())))
}

pub(super) fn materialize_font_resource(tf: &TextFragment) -> Result<MpFontResource, String> {
    let data_and_index = tf
        .font
        .font_data_and_index()
        .map_err(|_| "failed to load font data".to_string())?;
    Ok(MpFontResource {
        bytes: Arc::from(data_and_index.data.as_ref()),
        face_index: data_and_index.index,
    })
}

fn image_frame(raster_image: &RasterImage) -> (u32, u32, std::ops::Range<usize>) {
    let Some(frame) = raster_image.frames.first() else {
        return (
            raster_image.metadata.width as u32,
            raster_image.metadata.height as u32,
            0..raster_image.bytes.len(),
        );
    };
    (frame.width, frame.height, frame.byte_range.clone())
}

pub(super) fn hash_value<T: Hash>(value: T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_image_resource_key_tracks_pixels() {
        let a = Arc::new(RasterImage {
            metadata: pixels::ImageMetadata { width: 1, height: 1 },
            format: pixels::PixelFormat::BGRA8,
            id: None,
            cors_status: pixels::CorsStatus::Unsafe,
            bytes: Arc::new(vec![0, 0, 0, 255]),
            frames: Vec::new(),
            is_opaque: false,
        });
        let b = Arc::new(RasterImage {
            metadata: pixels::ImageMetadata { width: 1, height: 1 },
            format: pixels::PixelFormat::BGRA8,
            id: None,
            cors_status: pixels::CorsStatus::Unsafe,
            bytes: Arc::new(vec![255, 255, 255, 255]),
            frames: Vec::new(),
            is_opaque: false,
        });

        let key_a = background_image_key(Some(7), 0, &a);
        let key_b = background_image_key(Some(7), 0, &b);

        assert_ne!(key_a, key_b);
    }
}
