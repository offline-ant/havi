/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

#![deny(unsafe_code)]

use std::cell::{Cell, RefCell};
use std::fmt;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Arc;

use dpi::PhysicalSize;
use embedder_traits::RefreshDriver;
use euclid::default::Rect;
use euclid::Size2D;
use gleam::gl::{self, Gl};
use glow::NativeFramebuffer;
use image::RgbaImage;
use log::{debug, trace, warn};
use webrender_api::units::{DeviceIntRect, DevicePixel};

use crate::gl_device::{GlApi, GlDisplayInfo};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    message: String,
}

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}

impl std::error::Error for Error {}

/// The `RenderingContext` trait defines a set of methods for managing
/// an OpenGL or GLES rendering context.
/// Implementors of this trait are responsible for handling the creation,
/// management, and destruction of the rendering context and its associated
/// resources.
pub trait RenderingContext {
    /// Prepare this [`RenderingContext`] to be rendered upon by Servo. For instance,
    /// by binding a framebuffer to the current OpenGL context.
    fn prepare_for_rendering(&self) {}

    /// Read the contents of this [`Renderingcontext`] into an in-memory image. If the
    /// image cannot be read (for instance, if no rendering has taken place yet), then
    /// `None` is returned.
    ///
    /// In a double-buffered [`RenderingContext`] this is expected to read from the back
    /// buffer. That means that once Servo renders to the context, this should return those
    /// results, even before [`RenderingContext::present`] is called.
    fn read_to_image(&self, source_rectangle: DeviceIntRect) -> Option<RgbaImage>;

    /// Get the current size of this [`RenderingContext`].
    fn size(&self) -> PhysicalSize<u32>;

    /// Get the current size of this [`RenderingContext`] as [`Size2D`].
    fn size2d(&self) -> Size2D<u32, DevicePixel> {
        let size = self.size();
        Size2D::new(size.width, size.height)
    }

    /// Resizes the rendering surface to the given size.
    fn resize(&self, size: PhysicalSize<u32>);

    /// Presents the rendered frame to the screen. In a double-buffered context, this would
    /// swap buffers.
    fn present(&self);

    /// Makes the context the current OpenGL context for this thread.
    /// After calling this function, it is valid to use OpenGL rendering
    /// commands.
    fn make_current(&self) -> Result<(), Error>;

    /// Returns the `gleam` version of the OpenGL or GLES API.
    fn gleam_gl_api(&self) -> Rc<dyn gleam::gl::Gl>;

    /// Returns the OpenGL or GLES API.
    fn glow_gl_api(&self) -> Arc<glow::Context>;

    /// Return the [`RefreshDriver`] for this [`RenderingContext`]. If `None` is returned,
    /// then the default timer-based [`RefreshDriver`] will be used.
    fn refresh_driver(&self) -> Option<Rc<dyn RefreshDriver>> {
        None
    }

    /// Return GL display info for creating `GlDevice` instances (for WebGL).
    /// Returns `None` for rendering contexts that don't provide GL display info.
    fn gl_display_info(&self) -> Option<GlDisplayInfo> {
        None
    }
}

struct Framebuffer {
    gl: Rc<dyn Gl>,
    framebuffer_id: gl::GLuint,
    renderbuffer_id: gl::GLuint,
    texture_id: gl::GLuint,
    owns_texture: bool,
}

impl Framebuffer {
    fn bind(&self) {
        trace!("Binding FBO {}", self.framebuffer_id);
        self.gl
            .bind_framebuffer(gl::FRAMEBUFFER, self.framebuffer_id)
    }

    fn new(gl: Rc<dyn Gl>, size: PhysicalSize<u32>) -> Self {
        let framebuffer_ids = gl.gen_framebuffers(1);
        gl.bind_framebuffer(gl::FRAMEBUFFER, framebuffer_ids[0]);

        let texture_ids = gl.gen_textures(1);
        gl.bind_texture(gl::TEXTURE_2D, texture_ids[0]);
        gl.tex_image_2d(
            gl::TEXTURE_2D,
            0,
            gl::RGBA as gl::GLint,
            size.width as gl::GLsizei,
            size.height as gl::GLsizei,
            0,
            gl::RGBA,
            gl::UNSIGNED_BYTE,
            None,
        );
        gl.tex_parameter_i(
            gl::TEXTURE_2D,
            gl::TEXTURE_MAG_FILTER,
            gl::NEAREST as gl::GLint,
        );
        gl.tex_parameter_i(
            gl::TEXTURE_2D,
            gl::TEXTURE_MIN_FILTER,
            gl::NEAREST as gl::GLint,
        );

        gl.framebuffer_texture_2d(
            gl::FRAMEBUFFER,
            gl::COLOR_ATTACHMENT0,
            gl::TEXTURE_2D,
            texture_ids[0],
            0,
        );

        gl.bind_texture(gl::TEXTURE_2D, 0);

        let renderbuffer_ids = gl.gen_renderbuffers(1);
        let depth_rb = renderbuffer_ids[0];
        gl.bind_renderbuffer(gl::RENDERBUFFER, depth_rb);
        gl.renderbuffer_storage(
            gl::RENDERBUFFER,
            gl::DEPTH_COMPONENT24,
            size.width as gl::GLsizei,
            size.height as gl::GLsizei,
        );
        gl.framebuffer_renderbuffer(
            gl::FRAMEBUFFER,
            gl::DEPTH_ATTACHMENT,
            gl::RENDERBUFFER,
            depth_rb,
        );

        Self {
            gl,
            framebuffer_id: *framebuffer_ids
                .first()
                .expect("Guaranteed by GL operations"),
            renderbuffer_id: *renderbuffer_ids
                .first()
                .expect("Guaranteed by GL operations"),
            texture_id: *texture_ids.first().expect("Guaranteed by GL operations"),
            owns_texture: true,
        }
    }

    /// Create an FBO that renders into an externally-owned texture.
    /// `texture_target` is `GL_TEXTURE_2D` or `GL_TEXTURE_RECTANGLE`.
    /// The caller is responsible for the texture's lifecycle.
    fn new_with_external_texture(
        gl: Rc<dyn Gl>,
        size: PhysicalSize<u32>,
        texture_id: gl::GLuint,
        texture_target: u32,
    ) -> Self {
        let framebuffer_ids = gl.gen_framebuffers(1);
        gl.bind_framebuffer(gl::FRAMEBUFFER, framebuffer_ids[0]);

        gl.framebuffer_texture_2d(
            gl::FRAMEBUFFER,
            gl::COLOR_ATTACHMENT0,
            texture_target,
            texture_id,
            0,
        );

        let renderbuffer_ids = gl.gen_renderbuffers(1);
        let depth_rb = renderbuffer_ids[0];
        gl.bind_renderbuffer(gl::RENDERBUFFER, depth_rb);
        gl.renderbuffer_storage(
            gl::RENDERBUFFER,
            gl::DEPTH_COMPONENT24,
            size.width as gl::GLsizei,
            size.height as gl::GLsizei,
        );
        gl.framebuffer_renderbuffer(
            gl::FRAMEBUFFER,
            gl::DEPTH_ATTACHMENT,
            gl::RENDERBUFFER,
            depth_rb,
        );

        Self {
            gl,
            framebuffer_id: *framebuffer_ids
                .first()
                .expect("Guaranteed by GL operations"),
            renderbuffer_id: *renderbuffer_ids
                .first()
                .expect("Guaranteed by GL operations"),
            texture_id,
            owns_texture: false,
        }
    }

    /// Reattach this FBO to a different external texture (e.g., after resize).
    /// `texture_target` is `GL_TEXTURE_2D` or `GL_TEXTURE_RECTANGLE`.
    fn set_external_texture(
        &mut self,
        new_texture_id: gl::GLuint,
        size: PhysicalSize<u32>,
        texture_target: u32,
    ) {
        self.gl
            .bind_framebuffer(gl::FRAMEBUFFER, self.framebuffer_id);
        self.gl.framebuffer_texture_2d(
            gl::FRAMEBUFFER,
            gl::COLOR_ATTACHMENT0,
            texture_target,
            new_texture_id,
            0,
        );
        self.gl
            .bind_renderbuffer(gl::RENDERBUFFER, self.renderbuffer_id);
        self.gl.renderbuffer_storage(
            gl::RENDERBUFFER,
            gl::DEPTH_COMPONENT24,
            size.width as gl::GLsizei,
            size.height as gl::GLsizei,
        );
        self.gl.bind_framebuffer(gl::FRAMEBUFFER, 0);
        self.texture_id = new_texture_id;
    }

    fn read_to_image(&self, source_rectangle: DeviceIntRect) -> Option<RgbaImage> {
        Self::read_framebuffer_to_image(&self.gl, self.framebuffer_id, source_rectangle)
    }

    fn read_framebuffer_to_image(
        gl: &Rc<dyn Gl>,
        framebuffer_id: u32,
        source_rectangle: DeviceIntRect,
    ) -> Option<RgbaImage> {
        gl.bind_framebuffer(gl::FRAMEBUFFER, framebuffer_id);

        // For some reason, OSMesa fails to render on the 3rd
        // attempt in headless mode, under some conditions.
        // See https://github.com/servo/servo/issues/18606.
        gl.bind_vertex_array(0);

        let mut pixels = gl.read_pixels(
            source_rectangle.min.x,
            source_rectangle.min.y,
            source_rectangle.width(),
            source_rectangle.height(),
            gl::RGBA,
            gl::UNSIGNED_BYTE,
        );
        let gl_error = gl.get_error();
        if gl_error != gl::NO_ERROR {
            warn!("GL error code 0x{gl_error:x} set after read_pixels");
        }

        // flip image vertically (texture is upside down)
        let source_rectangle = source_rectangle.to_usize();
        let orig_pixels = pixels.clone();
        let stride = source_rectangle.width() * 4;
        for y in 0..source_rectangle.height() {
            let dst_start = y * stride;
            let src_start = (source_rectangle.height() - y - 1) * stride;
            let src_slice = &orig_pixels[src_start..src_start + stride];
            pixels[dst_start..dst_start + stride].clone_from_slice(&src_slice[..stride]);
        }

        RgbaImage::from_raw(
            source_rectangle.width() as u32,
            source_rectangle.height() as u32,
            pixels,
        )
    }
}

impl Drop for Framebuffer {
    fn drop(&mut self) {
        self.gl.bind_framebuffer(gl::FRAMEBUFFER, 0);
        if self.owns_texture {
            self.gl.delete_textures(&[self.texture_id]);
        }
        self.gl.delete_renderbuffers(&[self.renderbuffer_id]);
        self.gl.delete_framebuffers(&[self.framebuffer_id]);
    }
}

/// A [`RenderingContext`] designed for embedding Servo within a Makepad application.
///
/// Servo renders into an FBO within Makepad's own GL context — no second GL context
/// is created. The FBO's color attachment is a texture owned by Makepad, enabling
/// zero-copy display without any GPU→CPU→GPU roundtrip.
///
/// GL function pointers are loaded from the host's `eglGetProcAddress` at construction
/// time via [`MakepadRenderingContext::new_from_loader`].
///
/// # Safety
/// The caller must ensure that:
/// - The host GL context is current on the calling thread when this context is created
/// - All rendering calls happen on the same thread
pub struct MakepadRenderingContext {
    size: Cell<PhysicalSize<u32>>,
    gleam_gl: Rc<dyn Gl>,
    glow_gl: Arc<glow::Context>,
    framebuffer: RefCell<Framebuffer>,
    /// GL texture target for FBO color attachments. `GL_TEXTURE_2D` on EGL
    /// platforms, `GL_TEXTURE_RECTANGLE` on macOS (CGL/IOSurface).
    texture_target: u32,
    display_info: Option<GlDisplayInfo>,
}

impl MakepadRenderingContext {
    /// Create a new MakepadRenderingContext using the host's GL context directly.
    ///
    /// GL function pointers are loaded via the provided loader (typically backed
    /// by `eglGetProcAddress` or `dlsym`). `gl_api` selects the binding flavour:
    /// `GlApi::GL` loads desktop GL (macOS), `GlApi::GLES` loads GLES
    /// (Linux/Android/Windows). `texture_target` is the GL texture target for
    /// FBO color attachments (`GL_TEXTURE_2D` for EGL, `GL_TEXTURE_RECTANGLE`
    /// for CGL/IOSurface).
    ///
    /// # Safety
    /// The host GL context must be current on the calling thread.
    #[expect(unsafe_code)]
    pub unsafe fn new_from_loader(
        size: PhysicalSize<u32>,
        gl_loader: &dyn Fn(&str) -> *const std::ffi::c_void,
        gl_api: GlApi,
        texture_target: u32,
        display_info: Option<GlDisplayInfo>,
    ) -> Result<Self, Error> {
        debug!(
            "MakepadRenderingContext (direct context): size={:?} api={:?}",
            size, gl_api
        );

        let gleam_gl: Rc<dyn Gl> = unsafe {
            match gl_api {
                GlApi::GL => gl::GlFns::load_with(gl_loader),
                GlApi::GLES => gl::GlesFns::load_with(gl_loader),
            }
        };
        let glow_gl = unsafe { Arc::new(glow::Context::from_loader_function(gl_loader)) };

        let framebuffer = RefCell::new(Framebuffer::new(gleam_gl.clone(), size));

        Ok(MakepadRenderingContext {
            size: Cell::new(size),
            gleam_gl,
            glow_gl,
            framebuffer,
            texture_target,
            display_info,
        })
    }

    /// Switch to using an externally-owned texture as the render target.
    /// The texture must be valid in the current GL context.
    pub fn set_external_texture(&self, texture_id: gl::GLuint, size: PhysicalSize<u32>) {
        let target = self.texture_target;
        let mut fb = self.framebuffer.borrow_mut();
        if fb.owns_texture {
            // First time switching: replace entire framebuffer.
            let gl = fb.gl.clone();
            let new_fb = Framebuffer::new_with_external_texture(gl, size, texture_id, target);
            *fb = new_fb;
        } else {
            // Already using external texture: reattach.
            fb.set_external_texture(texture_id, size, target);
        }
        drop(fb);
        self.size.set(size);
    }

    #[allow(dead_code)]
    fn front_framebuffer(&self) -> Option<NativeFramebuffer> {
        NonZeroU32::new(self.framebuffer.borrow().framebuffer_id).map(NativeFramebuffer)
    }

    #[allow(dead_code)]
    fn blit_framebuffer(
        gl: &glow::Context,
        source_rect: Rect<i32>,
        source_framebuffer_id: NativeFramebuffer,
        target_rect: Rect<i32>,
        target_framebuffer_id: Option<NativeFramebuffer>,
    ) {
        use glow::HasContext as _;
        #[expect(unsafe_code)]
        unsafe {
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.scissor(
                target_rect.origin.x,
                target_rect.origin.y,
                target_rect.width(),
                target_rect.height(),
            );
            gl.enable(gl::SCISSOR_TEST);
            gl.clear(gl::COLOR_BUFFER_BIT);
            gl.disable(gl::SCISSOR_TEST);

            gl.bind_framebuffer(gl::READ_FRAMEBUFFER, Some(source_framebuffer_id));
            gl.bind_framebuffer(gl::DRAW_FRAMEBUFFER, target_framebuffer_id);

            gl.blit_framebuffer(
                source_rect.origin.x,
                source_rect.origin.y,
                source_rect.origin.x + source_rect.width(),
                source_rect.origin.y + source_rect.height(),
                target_rect.origin.x,
                target_rect.origin.y,
                target_rect.origin.x + target_rect.width(),
                target_rect.origin.y + target_rect.height(),
                gl::COLOR_BUFFER_BIT,
                gl::NEAREST,
            );
            gl.bind_framebuffer(gl::FRAMEBUFFER, target_framebuffer_id);
        }
    }

    #[allow(dead_code)]
    fn copy_rect_to(
        &self,
        source_rect: Rect<i32>,
        target_rect: Rect<i32>,
        target_framebuffer_id: Option<NativeFramebuffer>,
    ) {
        let Some(source_framebuffer_id) = self.front_framebuffer() else {
            return;
        };
        Self::blit_framebuffer(
            &self.glow_gl,
            source_rect,
            source_framebuffer_id,
            target_rect,
            target_framebuffer_id,
        );
    }

}

impl RenderingContext for MakepadRenderingContext {
    fn size(&self) -> PhysicalSize<u32> {
        self.size.get()
    }

    fn resize(&self, new_size: PhysicalSize<u32>) {
        let old_size = self.size.get();
        if old_size == new_size {
            return;
        }

        // If using an external texture, resize is handled by the caller
        // via set_external_texture() — don't recreate the framebuffer here.
        if !self.framebuffer.borrow().owns_texture {
            return;
        }

        let new_framebuffer = Framebuffer::new(self.gleam_gl.clone(), new_size);
        let _ = std::mem::replace(&mut *self.framebuffer.borrow_mut(), new_framebuffer);
        self.size.set(new_size);
    }

    fn prepare_for_rendering(&self) {
        self.framebuffer.borrow().bind();
    }

    fn present(&self) {
        // glFlush ensures all queued GL commands are submitted to the GPU.
        // Servo and Makepad share the same GL context on the same thread,
        // so this is sufficient — Makepad's subsequent draw calls are
        // guaranteed to see the completed FBO contents.
        self.gleam_gl.flush();
        // Unbind the FBO so Makepad's rendering targets the default framebuffer.
        self.gleam_gl.bind_framebuffer(gl::FRAMEBUFFER, 0);
    }

    fn make_current(&self) -> Result<(), Error> {
        // No-op: Servo renders within Makepad's own GL context.
        Ok(())
    }

    fn gleam_gl_api(&self) -> Rc<dyn gleam::gl::Gl> {
        self.gleam_gl.clone()
    }

    fn glow_gl_api(&self) -> Arc<glow::Context> {
        self.glow_gl.clone()
    }

    fn read_to_image(&self, source_rectangle: DeviceIntRect) -> Option<RgbaImage> {
        self.framebuffer.borrow().read_to_image(source_rectangle)
    }

    fn gl_display_info(&self) -> Option<GlDisplayInfo> {
        self.display_info.clone()
    }
}
