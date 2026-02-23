/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use canvas_traits::webgl::{WebGLContextId, WebGLThreads};
use euclid::default::Size2D;
use log::debug;
use paint_api::{ExternalImageSource, WebRenderExternalImageApi};
use rustc_hash::FxHashMap;
use surfman::chains::{SwapChainAPI, SwapChains, SwapChainsAPI};
use surfman::{
    Connection, Context, ContextAttributeFlags, ContextAttributes, Device, GLApi, SurfaceInfo,
    SurfaceTexture,
};
use webgl::webgl_thread::WebGLContextBusyMap;

/// Surfman device and context for surface-to-texture import, created lazily
/// to avoid disrupting the main GL context during painter initialization.
struct SurfmanState {
    device: Device,
    context: Context,
}

impl SurfmanState {
    fn new() -> Self {
        let connection = Connection::new()
            .expect("Failed to create surfman connection for WebGL texture sharing");
        let adapter = connection
            .create_adapter()
            .expect("Failed to create surfman adapter");
        let device = connection
            .create_device(&adapter)
            .expect("Failed to create surfman device");

        let flags = ContextAttributeFlags::ALPHA
            | ContextAttributeFlags::DEPTH
            | ContextAttributeFlags::STENCIL;
        let version = match connection.gl_api() {
            GLApi::GLES => surfman::GLVersion { major: 3, minor: 0 },
            GLApi::GL => surfman::GLVersion { major: 3, minor: 2 },
        };
        let context_descriptor = device
            .create_context_descriptor(&ContextAttributes { flags, version })
            .expect("Failed to create surfman context descriptor");
        let context = device
            .create_context(&context_descriptor, None)
            .expect("Failed to create surfman context");

        Self { device, context }
    }
}

impl Drop for SurfmanState {
    fn drop(&mut self) {
        let _ = self.device.destroy_context(&mut self.context);
    }
}

/// Bridge between the webrender::ExternalImage callbacks and the WebGLThreads.
pub struct WebGLExternalImages {
    webgl_threads: WebGLThreads,
    surfman: Option<SurfmanState>,
    swap_chains: SwapChains<WebGLContextId, Device>,
    busy_webgl_context_map: WebGLContextBusyMap,
    locked_front_buffers: FxHashMap<WebGLContextId, SurfaceTexture>,
}

impl WebGLExternalImages {
    pub fn new(
        webgl_threads: WebGLThreads,
        swap_chains: SwapChains<WebGLContextId, Device>,
        busy_webgl_context_map: WebGLContextBusyMap,
    ) -> Self {
        Self {
            webgl_threads,
            surfman: None,
            swap_chains,
            busy_webgl_context_map,
            locked_front_buffers: FxHashMap::default(),
        }
    }

    /// Returns the surfman state, creating it on first use.
    fn surfman(&mut self) -> &mut SurfmanState {
        self.surfman.get_or_insert_with(SurfmanState::new)
    }

    fn lock_swap_chain(&mut self, id: WebGLContextId) -> Option<(u32, Size2D<i32>)> {
        debug!("... locking chain {:?}", id);

        {
            let mut busy_webgl_context_map = self.busy_webgl_context_map.write();
            *busy_webgl_context_map.entry(id).or_default() += 1;
        }

        let front_buffer = self.swap_chains.get(id)?.take_surface()?;
        let surfman = self.surfman();
        let SurfaceInfo { size, .. } = surfman.device.surface_info(&front_buffer);
        let surface_texture = surfman
            .device
            .create_surface_texture(&mut surfman.context, front_buffer)
            .unwrap();
        let gl_texture = surfman
            .device
            .surface_texture_object(&surface_texture)
            .map(|tex| tex.0.get())
            .unwrap_or(0);
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
        let surfman = self.surfman();
        let surface = surfman
            .device
            .destroy_surface_texture(&mut surfman.context, locked_front_buffer)
            .map_err(|(error, _)| error)
            .ok()?;

        self.swap_chains
            .get(id)
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
