/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use canvas_traits::webgl::{WebGLContextId, WebGLThreads};
use euclid::default::Size2D;
use log::debug;
use paint_api::gl_device::swap_chain::{GlSurfaceTexture, SwapChains};
use paint_api::{ExternalImageSource, WebRenderExternalImageApi};
use rustc_hash::FxHashMap;
use webgl::webgl_thread::WebGLContextBusyMap;

/// Bridge between the webrender::ExternalImage callbacks and the WebGLThreads.
pub struct WebGLExternalImages {
    webgl_threads: WebGLThreads,
    swap_chains: SwapChains<WebGLContextId>,
    busy_webgl_context_map: WebGLContextBusyMap,
    locked_front_buffers: FxHashMap<WebGLContextId, GlSurfaceTexture>,
}

impl WebGLExternalImages {
    pub fn new(
        webgl_threads: WebGLThreads,
        swap_chains: SwapChains<WebGLContextId>,
        busy_webgl_context_map: WebGLContextBusyMap,
    ) -> Self {
        Self {
            webgl_threads,
            swap_chains,
            busy_webgl_context_map,
            locked_front_buffers: FxHashMap::default(),
        }
    }

    fn lock_swap_chain(&mut self, id: WebGLContextId) -> Option<(u32, Size2D<i32>)> {
        debug!("... locking chain {:?}", id);

        {
            let mut busy_webgl_context_map = self.busy_webgl_context_map.write();
            *busy_webgl_context_map.entry(id).or_default() += 1;
        }

        let front_buffer = self.swap_chains.get(&id)?.take_surface()?;
        let size = front_buffer.size;
        let surface_texture =
            paint_api::gl_device::swap_chain::create_surface_texture(front_buffer);
        let gl_texture = surface_texture.texture_id();
        self.locked_front_buffers.insert(id, surface_texture);

        Some((gl_texture, size))
    }

    fn unlock_swap_chain(&mut self, id: WebGLContextId) -> Option<()> {
        debug!("... unlocked chain {:?}", id);

        {
            let mut busy_webgl_context_map = self.busy_webgl_context_map.write();
            *busy_webgl_context_map.entry(id).or_insert(1) -= 1;
        }

        let locked_front_buffer = self.locked_front_buffers.remove(&id)?;
        let surface =
            paint_api::gl_device::swap_chain::destroy_surface_texture(locked_front_buffer);

        self.swap_chains
            .get(&id)
            .expect("Should always have a SwapChain for a busy WebGLContext")
            .recycle_surface(surface);

        let _ = self.webgl_threads.finished_rendering_to_context(id);

        Some(())
    }
}

impl WebRenderExternalImageApi for WebGLExternalImages {
    fn lock(&mut self, id: u64) -> (ExternalImageSource<'_>, Size2D<i32>) {
        let (texture_id, size) = self.lock_swap_chain(WebGLContextId(id)).unwrap_or_default();
        (ExternalImageSource::NativeTexture(texture_id), size)
    }

    fn unlock(&mut self, id: u64) {
        self.unlock_swap_chain(WebGLContextId(id));
    }
}
