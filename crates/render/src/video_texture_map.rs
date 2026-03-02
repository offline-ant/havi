//! VideoTextureMap: maps raw image keys to Makepad video textures.
//!
//! Accessed only from the Makepad main thread, so `thread_local!` storage
//! is used (Texture is Rc-based and not Send).

use makepad_widgets::makepad_platform::Texture;
use std::collections::HashMap;

thread_local! {
    /// Maps (namespace, index) image key → Makepad Texture for live video frames.
    static VIDEO_TEXTURES: std::cell::RefCell<HashMap<(u32, u32), Texture>> =
        std::cell::RefCell::new(HashMap::new());
}

/// Register a Makepad texture for a video element's image key.
pub fn register_video_texture(image_key: (u32, u32), texture: Texture) {
    VIDEO_TEXTURES.with(|m| m.borrow_mut().insert(image_key, texture));
}

/// Remove a video texture registration.
pub fn deregister_video_texture(image_key: (u32, u32)) {
    VIDEO_TEXTURES.with(|m| m.borrow_mut().remove(&image_key));
}

/// Look up the Makepad texture for a video image key.
pub fn get_video_texture(image_key: (u32, u32)) -> Option<Texture> {
    VIDEO_TEXTURES.with(|m| m.borrow().get(&image_key).cloned())
}
