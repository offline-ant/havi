
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

// Accessed only from the Makepad main thread. `Texture` is Rc-backed and not
// `Send`, so this state stays thread-local.
thread_local! {
    static VIDEO_BINDINGS: std::cell::RefCell<HashMap<(u32, u32), VideoBinding>> =
        std::cell::RefCell::new(HashMap::new());
}

pub fn set_external_texture(image_key: (u32, u32), texture: Texture) {
    VIDEO_BINDINGS.with(|m| {
        let mut m = m.borrow_mut();
        let binding = m.entry(image_key).or_insert_with(VideoBinding::new);
        binding.external_texture = Some(texture);
    });
}

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

pub fn set_yuv_metadata(image_key: (u32, u32), yuv_metadata: VideoYuvMetadata) {
    VIDEO_BINDINGS.with(|m| {
        let mut m = m.borrow_mut();
        let binding = m.entry(image_key).or_insert_with(VideoBinding::new);
        binding.yuv_metadata = yuv_metadata;
    });
}

pub fn remove_video_binding(image_key: (u32, u32)) {
    VIDEO_BINDINGS.with(|m| {
        m.borrow_mut().remove(&image_key);
    });
}

pub fn get_video_binding(image_key: (u32, u32)) -> Option<VideoBinding> {
    VIDEO_BINDINGS.with(|m| m.borrow().get(&image_key).cloned())
}
