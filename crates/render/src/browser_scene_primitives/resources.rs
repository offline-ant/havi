use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use havi_fragment_semantics::fragment_tree::{BackgroundImage, ImageFragment, TextFragment};
use makepad_browser_scene::{MpFontKey, MpFontResource};
use makepad_widgets::{dvec2, Cx2d};

pub(super) fn background_image_resource(
    owner_node_id: Option<usize>,
    layer_index: usize,
    bg: &BackgroundImage,
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

pub(super) fn image_resource_for_fragment(
    image: &ImageFragment,
) -> (makepad_browser_scene::MpImageKey, makepad_browser_scene::MpImageResource) {
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

pub(super) fn font_resource_for_text(
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

pub(super) fn hash_value<T: Hash>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_image_resource_key_tracks_pixels() {
        let a = BackgroundImage {
            width: 1,
            height: 1,
            pixels: vec![0, 0, 0, 255],
        };
        let b = BackgroundImage {
            width: 1,
            height: 1,
            pixels: vec![255, 255, 255, 255],
        };

        let (key_a, _) = background_image_resource(Some(7), 0, &a);
        let (key_b, _) = background_image_resource(Some(7), 0, &b);

        assert_ne!(key_a, key_b);
    }
}
