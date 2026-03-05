//! VideoTextureMap: maps raw image keys to coherent video texture bindings.
//!
//! Accessed only from the Makepad main thread, so `thread_local!` storage
//! is used (Texture is Rc-based and not Send).

use makepad_widgets::makepad_platform::event::video_playback::VideoYuvMetadata;
use makepad_widgets::makepad_platform::Texture;
use std::collections::HashMap;

#[derive(Clone)]
pub struct VideoYuvPlanes {
    pub tex_y: Texture,
    pub tex_u: Texture,
    pub tex_v: Texture,
}

#[derive(Clone)]
pub struct VideoBinding {
    pub external_texture: Option<Texture>,
    pub yuv_planes: Option<VideoYuvPlanes>,
    pub yuv_metadata: VideoYuvMetadata,
}

impl VideoBinding {
    fn new() -> Self {
        Self {
            external_texture: None,
            yuv_planes: None,
            yuv_metadata: VideoYuvMetadata::disabled(),
        }
    }
}

thread_local! {
    /// Maps (namespace, index) image key → full video binding.
    static VIDEO_BINDINGS: std::cell::RefCell<HashMap<(u32, u32), VideoBinding>> =
        std::cell::RefCell::new(HashMap::new());
}

/// Register or update the external video texture for an image key.
pub fn set_external_texture(image_key: (u32, u32), texture: Texture) {
    VIDEO_BINDINGS.with(|m| {
        let mut m = m.borrow_mut();
        let binding = m.entry(image_key).or_insert_with(VideoBinding::new);
        binding.external_texture = Some(texture);
    });
}

/// Register or update YUV plane textures for an image key.
pub fn set_yuv_planes(
    image_key: (u32, u32),
    tex_y: Texture,
    tex_u: Texture,
    tex_v: Texture,
) {
    VIDEO_BINDINGS.with(|m| {
        let mut m = m.borrow_mut();
        let binding = m.entry(image_key).or_insert_with(VideoBinding::new);
        binding.yuv_planes = Some(VideoYuvPlanes {
            tex_y,
            tex_u,
            tex_v,
        });
    });
}

/// Update latest platform-provided YUV metadata for an image key.
pub fn set_yuv_metadata(image_key: (u32, u32), yuv_metadata: VideoYuvMetadata) {
    VIDEO_BINDINGS.with(|m| {
        let mut m = m.borrow_mut();
        let binding = m.entry(image_key).or_insert_with(VideoBinding::new);
        binding.yuv_metadata = yuv_metadata;
    });
}

/// Remove full video binding for an image key.
pub fn remove_video_binding(image_key: (u32, u32)) {
    VIDEO_BINDINGS.with(|m| {
        m.borrow_mut().remove(&image_key);
    });
}

/// Look up full video binding for an image key.
pub fn get_video_binding(image_key: (u32, u32)) -> Option<VideoBinding> {
    VIDEO_BINDINGS.with(|m| m.borrow().get(&image_key).cloned())
}
