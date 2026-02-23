/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Cross-platform GL device abstraction replacing surfman.
//!
//! Provides context creation, FBO-based surface management, and swap chains
//! for WebGL rendering. Platform-specific code is confined to the `egl` and
//! `cgl` backend modules.

#![allow(unsafe_code)]

use std::ffi::c_void;
use std::rc::Rc;

use euclid::default::Size2D;
use glow::{self as gl, HasContext};

#[cfg(any(target_os = "linux", target_os = "android"))]
pub mod egl;
#[cfg(target_os = "macos")]
pub mod cgl;

pub mod surface;
pub mod swap_chain;

pub use surface::GlSurface;
pub use swap_chain::{
    GlSurfaceTexture, SwapChain, SwapChains, create_surface_texture, destroy_surface_texture,
};

/// GL API type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlApi {
    /// Desktop OpenGL.
    GL,
    /// OpenGL ES.
    GLES,
}

/// GL version.
#[derive(Clone, Copy, Debug)]
pub struct GlVersion {
    pub major: u8,
    pub minor: u8,
}

/// Context creation attributes.
#[derive(Clone, Debug)]
pub struct GlContextAttributes {
    pub version: GlVersion,
    pub alpha: bool,
    pub depth: bool,
    pub stencil: bool,
}

/// Info about a bound surface.
pub struct GlSurfaceInfo {
    pub size: Size2D<i32>,
    pub framebuffer_object: Option<gl::NativeFramebuffer>,
}

/// Platform display info for creating a GlDevice.
#[cfg(any(target_os = "linux", target_os = "android"))]
pub type GlDisplayInfo = egl::EglDisplayInfo;
#[cfg(target_os = "macos")]
pub type GlDisplayInfo = cgl::CglDisplayInfo;

/// Cross-platform GL device. Manages context creation and surface operations.
///
/// All GL contexts created by a single `GlDevice` share a texture namespace,
/// enabling cross-context texture sampling (e.g. WebGL → WebRender compositor).
pub struct GlDevice {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    backend: egl::EglBackend,
    #[cfg(target_os = "macos")]
    backend: cgl::CglBackend,
}

/// An opaque GL context handle.
pub struct GlContext {
    raw: *mut c_void,
    /// Glow GL bindings loaded for this context.
    gl: Rc<glow::Context>,
    /// The surface currently bound to this context, if any.
    bound_surface: Option<GlSurface>,
}

// GlContext is not Send — it must stay on the thread that created it.
// The raw context handle is thread-local.

impl GlDevice {
    /// Create a device from platform display info.
    pub fn new(info: &GlDisplayInfo) -> Self {
        GlDevice {
            #[cfg(any(target_os = "linux", target_os = "android"))]
            backend: egl::EglBackend::new(info),
            #[cfg(target_os = "macos")]
            backend: cgl::CglBackend::new(info),
        }
    }

    /// GL API type (GL or GLES).
    pub fn gl_api(&self) -> GlApi {
        self.backend.gl_api()
    }

    /// Create a new GL context, optionally sharing with another context.
    /// If `share_with` is None, shares with the device's root context.
    pub fn create_context(
        &self,
        _attrs: &GlContextAttributes,
        share_with: Option<&GlContext>,
    ) -> GlContext {
        let share = share_with.map(|c| c.raw);
        let raw = self.backend.create_context(share);
        self.backend.make_context_current(raw);

        let gl = unsafe {
            Rc::new(glow::Context::from_loader_function(|name| {
                self.backend.get_proc_address(name) as *const c_void
            }))
        };

        GlContext {
            raw,
            gl,
            bound_surface: None,
        }
    }

    /// Destroy a GL context.
    pub fn destroy_context(&self, ctx: &mut GlContext) {
        // Destroy any bound surface first
        if let Some(surface) = ctx.bound_surface.take() {
            surface.destroy(&ctx.gl);
        }
        self.backend.destroy_context(ctx.raw);
        ctx.raw = std::ptr::null_mut();
    }

    /// Make a context current on this thread.
    pub fn make_context_current(&self, ctx: &GlContext) {
        self.backend.make_context_current(ctx.raw);
    }

    /// Get GL function pointer by name.
    pub fn get_proc_address(&self, _ctx: &GlContext, name: &str) -> *const c_void {
        self.backend.get_proc_address(name) as *const c_void
    }

    /// Create a new FBO surface. The given context must be current.
    pub fn create_surface(&self, ctx: &GlContext, size: Size2D<i32>) -> GlSurface {
        GlSurface::new(&ctx.gl, size)
    }

    /// Destroy a surface. The owning context must be current.
    pub fn destroy_surface(&self, ctx: &GlContext, surface: &mut GlSurface) {
        surface.destroy(&ctx.gl);
    }

    /// Bind a surface as the render target for a context.
    pub fn bind_surface_to_context(
        &self,
        ctx: &mut GlContext,
        surface: GlSurface,
    ) -> Result<(), String> {
        if ctx.bound_surface.is_some() {
            return Err("Surface already bound to context".into());
        }
        unsafe {
            ctx.gl
                .bind_framebuffer(gl::FRAMEBUFFER, Some(surface.framebuffer));
        }
        ctx.bound_surface = Some(surface);
        Ok(())
    }

    /// Unbind the surface from a context. Returns the surface.
    pub fn unbind_surface_from_context(&self, ctx: &mut GlContext) -> Option<GlSurface> {
        let surface = ctx.bound_surface.take()?;
        unsafe {
            ctx.gl.bind_framebuffer(gl::FRAMEBUFFER, None);
        }
        Some(surface)
    }

    /// Get info about the surface bound to a context.
    pub fn context_surface_info(&self, ctx: &GlContext) -> Option<GlSurfaceInfo> {
        ctx.bound_surface.as_ref().map(|s| GlSurfaceInfo {
            size: s.size,
            framebuffer_object: Some(s.framebuffer),
        })
    }

    /// Surface info without needing the context.
    pub fn surface_info(&self, surface: &GlSurface) -> GlSurfaceInfo {
        GlSurfaceInfo {
            size: surface.size,
            framebuffer_object: Some(surface.framebuffer),
        }
    }

    /// Wrap a surface as a texture for compositing (trivial: surface is already texture-backed).
    pub fn create_surface_texture(
        &self,
        _ctx: &mut GlContext,
        surface: GlSurface,
    ) -> Result<GlSurfaceTexture, (String, GlSurface)> {
        Ok(swap_chain::create_surface_texture(surface))
    }

    /// Unwrap a surface texture back to a surface.
    pub fn destroy_surface_texture(
        &self,
        _ctx: &mut GlContext,
        tex: GlSurfaceTexture,
    ) -> Result<GlSurface, (String, GlSurfaceTexture)> {
        Ok(swap_chain::destroy_surface_texture(tex))
    }

    /// GL texture id from a surface texture.
    pub fn surface_texture_object(&self, tex: &GlSurfaceTexture) -> u32 {
        tex.texture_id()
    }

    /// GL texture target for surface textures. Always `GL_TEXTURE_2D`.
    pub fn surface_gl_texture_target(&self) -> u32 {
        gl::TEXTURE_2D
    }

    /// Resize a surface. The owning context must be current.
    /// Creates a new surface and destroys the old one.
    pub fn resize_surface(
        &self,
        ctx: &GlContext,
        surface: &mut GlSurface,
        new_size: Size2D<i32>,
    ) {
        let new = GlSurface::new(&ctx.gl, new_size);
        surface.destroy(&ctx.gl);
        *surface = new;
    }
}

impl GlContext {
    /// Get the glow GL bindings for this context.
    pub fn gl(&self) -> &glow::Context {
        &self.gl
    }

    /// Get the glow GL bindings as Rc (for sharing).
    pub fn gl_rc(&self) -> Rc<glow::Context> {
        self.gl.clone()
    }

    /// Get the raw platform context handle.
    pub fn raw(&self) -> *mut c_void {
        self.raw
    }

    /// Whether a surface is currently bound.
    pub fn has_bound_surface(&self) -> bool {
        self.bound_surface.is_some()
    }

    /// Get the framebuffer of the bound surface, if any.
    pub fn bound_framebuffer(&self) -> Option<gl::NativeFramebuffer> {
        self.bound_surface.as_ref().map(|s| s.framebuffer)
    }
}

// Platform backends expose the same method interface:
//   fn gl_api(&self) -> GlApi
//   fn create_context(&self, share_with: Option<*mut c_void>) -> *mut c_void
//   fn destroy_context(&self, ctx: *mut c_void)
//   fn make_context_current(&self, ctx: *mut c_void)
//   fn get_proc_address(&self, name: &str) -> *mut c_void
// No trait needed — the #[cfg] in GlDevice selects the concrete backend.
