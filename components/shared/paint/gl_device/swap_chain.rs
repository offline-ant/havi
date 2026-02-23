/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Thread-safe swap chain for double-buffered WebGL rendering.
//!
//! Replaces `surfman::chains`. The producer (WebGL thread) renders to the back
//! buffer and swaps. The consumer (compositor thread) takes the front buffer
//! texture for compositing, then recycles it.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex, RwLock};

use euclid::default::Size2D;
use glow::{self as gl, HasContext};

use super::surface::GlSurface;

/// A surface currently held as a "texture" by the consumer.
/// In our FBO model this is trivial — the surface already IS a texture.
pub struct GlSurfaceTexture {
    surface: GlSurface,
}

impl GlSurfaceTexture {
    /// GL texture id for compositing.
    pub fn texture_id(&self) -> u32 {
        self.surface.texture_id()
    }
}

/// Internal state of a single swap chain.
struct SwapChainData {
    size: Size2D<i32>,
    /// Back buffer: the FBO the producer renders into.
    /// `None` when the surface has been temporarily taken (attached to context).
    back: Option<GlSurface>,
    /// Front buffer: completed frame ready for the consumer.
    pending: Option<GlSurface>,
    /// Surfaces returned by the consumer, available for reuse.
    recycled: Vec<GlSurface>,
    /// Whether the back buffer is currently bound to the producer context.
    attached: bool,
}

/// Handle to a single swap chain. Clone is cheap (Arc).
#[derive(Clone)]
pub struct SwapChain(Arc<Mutex<SwapChainData>>);

impl SwapChain {
    /// Create a new swap chain with an initial back buffer.
    /// The GL context must be current.
    pub fn new(gl: &glow::Context, size: Size2D<i32>) -> Self {
        let back = GlSurface::new(gl, size);
        SwapChain(Arc::new(Mutex::new(SwapChainData {
            size,
            back: Some(back),
            pending: None,
            recycled: Vec::new(),
            attached: false,
        })))
    }

    /// Swap buffers. Moves back → pending, allocates or recycles a new back.
    /// The producer GL context must be current.
    pub fn swap_buffers(
        &self,
        gl: &glow::Context,
        preserve: bool,
    ) {
        let mut data = self.0.lock().unwrap_or_else(|e| e.into_inner());

        // Old pending → recycled
        if let Some(old_pending) = data.pending.take() {
            data.recycled.push(old_pending);
        }

        // Back → pending
        let old_back = data.back.take().expect("back buffer missing during swap");
        if data.attached {
            // Unbind before transfer
            unsafe {
                gl.bind_framebuffer(gl::FRAMEBUFFER, None);
            }
            data.attached = false;
        }
        let new_pending = old_back;

        // Get a new back: recycle a matching-size surface or create fresh
        let size = data.size;
        let new_back = Self::take_recycled(&mut data.recycled, size, gl)
            .unwrap_or_else(|| GlSurface::new(gl, size));

        if preserve {
            // Blit old front → new back
            Self::blit(gl, &new_pending, &new_back, data.size);
        }

        data.pending = Some(new_pending);
        data.back = Some(new_back);
    }

    /// Bind the back buffer FBO as the current render target.
    pub fn bind(&self, gl: &glow::Context) {
        let mut data = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref back) = data.back {
            unsafe {
                gl.bind_framebuffer(gl::FRAMEBUFFER, Some(back.framebuffer));
            }
            data.attached = true;
        }
    }

    /// Unbind the back buffer FBO.
    pub fn unbind(&self, gl: &glow::Context) {
        let mut data = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if data.attached {
            unsafe {
                gl.bind_framebuffer(gl::FRAMEBUFFER, None);
            }
            data.attached = false;
        }
    }

    /// Get info about the back buffer (size and framebuffer).
    pub fn back_buffer_info(&self) -> Option<(Size2D<i32>, gl::NativeFramebuffer)> {
        let data = self.0.lock().unwrap_or_else(|e| e.into_inner());
        data.back
            .as_ref()
            .map(|s| (s.size, s.framebuffer))
    }

    /// Take the front buffer for compositing. Returns the surface.
    /// Called by the consumer (compositor thread).
    pub fn take_surface(&self) -> Option<GlSurface> {
        let mut data = self.0.lock().unwrap_or_else(|e| e.into_inner());
        data.pending
            .take()
            .or_else(|| data.recycled.pop())
    }

    /// Return a surface after compositing. Called by the consumer.
    pub fn recycle_surface(&self, surface: GlSurface) {
        let mut data = self.0.lock().unwrap_or_else(|e| e.into_inner());
        data.recycled.push(surface);
    }

    /// Resize the swap chain. Destroys the old back buffer and creates a new one.
    /// The producer GL context must be current.
    pub fn resize(&self, gl: &glow::Context, new_size: Size2D<i32>) {
        let mut data = self.0.lock().unwrap_or_else(|e| e.into_inner());

        // Destroy old back buffer
        if let Some(old_back) = data.back.take() {
            if data.attached {
                unsafe {
                    gl.bind_framebuffer(gl::FRAMEBUFFER, None);
                }
                data.attached = false;
            }
            old_back.destroy(gl);
        }

        // Destroy recycled surfaces that don't match new size
        let mut keep = Vec::new();
        for s in data.recycled.drain(..) {
            if s.size == new_size {
                keep.push(s);
            } else {
                s.destroy(gl);
            }
        }
        data.recycled = keep;

        // Create new back buffer
        data.back = Some(GlSurface::new(gl, new_size));
        data.size = new_size;
    }

    /// Clear the back buffer surface.
    /// The producer GL context must be current.
    pub fn clear_surface(&self, gl: &glow::Context, clear_color: [f32; 4]) {
        let data = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref back) = data.back {
            unsafe {
                gl.bind_framebuffer(gl::FRAMEBUFFER, Some(back.framebuffer));
                gl.clear_color(
                    clear_color[0],
                    clear_color[1],
                    clear_color[2],
                    clear_color[3],
                );
                gl.clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT | gl::STENCIL_BUFFER_BIT);
            }
        }
    }

    /// Current size of the back buffer.
    pub fn size(&self) -> Size2D<i32> {
        let data = self.0.lock().unwrap_or_else(|e| e.into_inner());
        data.size
    }

    /// Whether the back buffer is currently bound to the GL context.
    pub fn is_attached(&self) -> bool {
        let data = self.0.lock().unwrap_or_else(|e| e.into_inner());
        data.attached
    }

    /// Take the back buffer as a surface texture for reading.
    /// Used by WebXR begin_frame.
    pub fn take_surface_texture(&self) -> Option<GlSurfaceTexture> {
        let mut data = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let surface = data.pending.take().or_else(|| data.recycled.pop())?;
        Some(GlSurfaceTexture { surface })
    }

    /// Return a surface texture after reading.
    /// Used by WebXR end_frame.
    pub fn recycle_surface_texture(&self, tex: GlSurfaceTexture) {
        self.recycle_surface(tex.surface);
    }

    /// Destroy all surfaces. The producer GL context must be current.
    pub fn destroy(&self, gl: &glow::Context) {
        let mut data = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(back) = data.back.take() {
            back.destroy(gl);
        }
        if let Some(pending) = data.pending.take() {
            pending.destroy(gl);
        }
        for s in data.recycled.drain(..) {
            s.destroy(gl);
        }
    }

    fn take_recycled(
        recycled: &mut Vec<GlSurface>,
        size: Size2D<i32>,
        gl: &glow::Context,
    ) -> Option<GlSurface> {
        // Find a surface matching the requested size
        if let Some(pos) = recycled.iter().position(|s| s.size == size) {
            return Some(recycled.swap_remove(pos));
        }
        // No matching size — destroy all recycled and return None
        for s in recycled.drain(..) {
            s.destroy(gl);
        }
        None
    }

    fn blit(gl: &glow::Context, src: &GlSurface, dst: &GlSurface, size: Size2D<i32>) {
        unsafe {
            gl.bind_framebuffer(gl::READ_FRAMEBUFFER, Some(src.framebuffer));
            gl.bind_framebuffer(gl::DRAW_FRAMEBUFFER, Some(dst.framebuffer));
            gl.blit_framebuffer(
                0,
                0,
                size.width,
                size.height,
                0,
                0,
                size.width,
                size.height,
                gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT | gl::STENCIL_BUFFER_BIT,
                gl::NEAREST,
            );
            gl.bind_framebuffer(gl::FRAMEBUFFER, None);
        }
    }
}

/// Thread-safe collection of swap chains keyed by ID.
#[derive(Clone)]
pub struct SwapChains<ID: Eq + Hash + Clone> {
    table: Arc<RwLock<HashMap<ID, SwapChain>>>,
}

impl<ID: Eq + Hash + Clone> SwapChains<ID> {
    pub fn new() -> Self {
        SwapChains {
            table: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new swap chain and insert it.
    pub fn create(
        &self,
        id: ID,
        gl: &glow::Context,
        size: Size2D<i32>,
    ) -> SwapChain {
        let chain = SwapChain::new(gl, size);
        let mut table = self.table.write().unwrap_or_else(|e| e.into_inner());
        table.insert(id, chain.clone());
        chain
    }

    /// Look up a swap chain by ID.
    pub fn get(&self, id: &ID) -> Option<SwapChain> {
        let table = self.table.read().unwrap_or_else(|e| e.into_inner());
        table.get(id).cloned()
    }

    /// Remove and destroy a swap chain. The producer GL context must be current.
    pub fn destroy(&self, id: &ID, gl: &glow::Context) {
        let mut table = self.table.write().unwrap_or_else(|e| e.into_inner());
        if let Some(chain) = table.remove(id) {
            chain.destroy(gl);
        }
    }
}

impl<ID: Eq + Hash + Clone> Default for SwapChains<ID> {
    fn default() -> Self {
        Self::new()
    }
}

/// Wrap a surface as a "texture" for compositing. Trivial in our FBO model.
pub fn create_surface_texture(surface: GlSurface) -> GlSurfaceTexture {
    GlSurfaceTexture { surface }
}

/// Unwrap a surface texture back to a surface.
pub fn destroy_surface_texture(tex: GlSurfaceTexture) -> GlSurface {
    tex.surface
}
