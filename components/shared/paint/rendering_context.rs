/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

#![deny(unsafe_code)]

use std::cell::{Cell, RefCell, RefMut};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Arc;

use dpi::PhysicalSize;
use embedder_traits::RefreshDriver;
use euclid::default::Rect;
use euclid::{Point2D, Size2D};
use gleam::gl::{self, Gl};
use glow::NativeFramebuffer;
use image::RgbaImage;
use log::{debug, trace, warn};
use raw_window_handle::{DisplayHandle, WindowHandle};
pub use surfman::Error;
use crate::gl_device::GlDisplayInfo;
use surfman::chains::{PreserveBuffer, SwapChain};
use surfman::{
    Adapter, Connection, Context, ContextAttributeFlags, ContextAttributes, Device, GLApi,
    NativeContext, NativeWidget, Surface, SurfaceAccess, SurfaceType,
};
use webrender_api::units::{DeviceIntRect, DevicePixel};

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

/// A rendering context that uses the Surfman library to create and manage
/// the OpenGL context and surface. This struct provides the default implementation
/// of the `RenderingContext` trait, handling the creation, management, and destruction
/// of the rendering context and its associated resources.
///
/// The `SurfmanRenderingContext` struct encapsulates the necessary data and methods
/// to interact with the Surfman library, including creating surfaces, binding surfaces,
/// resizing surfaces, presenting rendered frames, and managing the OpenGL context state.
struct SurfmanRenderingContext {
    gleam_gl: Rc<dyn Gl>,
    glow_gl: Arc<glow::Context>,
    device: RefCell<Device>,
    context: RefCell<Context>,
    refresh_driver: Option<Rc<dyn RefreshDriver>>,
}

impl Drop for SurfmanRenderingContext {
    fn drop(&mut self) {
        let device = &mut self.device.borrow_mut();
        let context = &mut self.context.borrow_mut();
        let _ = device.destroy_context(context);
    }
}

impl SurfmanRenderingContext {
    fn new(
        connection: &Connection,
        adapter: &Adapter,
        refresh_driver: Option<Rc<dyn RefreshDriver>>,
    ) -> Result<Self, Error> {
        let device = connection.create_device(adapter)?;

        let flags = ContextAttributeFlags::ALPHA |
            ContextAttributeFlags::DEPTH |
            ContextAttributeFlags::STENCIL;
        let gl_api = connection.gl_api();
        let version = match &gl_api {
            GLApi::GLES => surfman::GLVersion { major: 3, minor: 0 },
            GLApi::GL => surfman::GLVersion { major: 3, minor: 2 },
        };
        let context_descriptor =
            device.create_context_descriptor(&ContextAttributes { flags, version })?;
        let context = device.create_context(&context_descriptor, None)?;

        #[expect(unsafe_code)]
        let gleam_gl = {
            match gl_api {
                GLApi::GL => unsafe {
                    gl::GlFns::load_with(|func_name| device.get_proc_address(&context, func_name))
                },
                GLApi::GLES => unsafe {
                    gl::GlesFns::load_with(|func_name| device.get_proc_address(&context, func_name))
                },
            }
        };

        #[expect(unsafe_code)]
        let glow_gl = unsafe {
            glow::Context::from_loader_function(|function_name| {
                device.get_proc_address(&context, function_name)
            })
        };

        Ok(SurfmanRenderingContext {
            gleam_gl,
            glow_gl: Arc::new(glow_gl),
            device: RefCell::new(device),
            context: RefCell::new(context),
            refresh_driver,
        })
    }

    fn create_surface(&self, surface_type: SurfaceType<NativeWidget>) -> Result<Surface, Error> {
        let device = &mut self.device.borrow_mut();
        let context = &self.context.borrow();
        device.create_surface(context, SurfaceAccess::GPUOnly, surface_type)
    }

    fn bind_surface(&self, surface: Surface) -> Result<(), Error> {
        let device = &self.device.borrow();
        let context = &mut self.context.borrow_mut();
        device
            .bind_surface_to_context(context, surface)
            .map_err(|(err, mut surface)| {
                let _ = device.destroy_surface(context, &mut surface);
                err
            })?;
        Ok(())
    }

    fn create_attached_swap_chain(&self) -> Result<SwapChain<Device>, Error> {
        let device = &mut self.device.borrow_mut();
        let context = &mut self.context.borrow_mut();
        SwapChain::create_attached(device, context, SurfaceAccess::GPUOnly)
    }

    fn resize_surface(&self, size: PhysicalSize<u32>) -> Result<(), Error> {
        let size = Size2D::new(size.width as i32, size.height as i32);
        let device = &mut self.device.borrow_mut();
        let context = &mut self.context.borrow_mut();

        let mut surface = device.unbind_surface_from_context(context)?.unwrap();
        device.resize_surface(context, &mut surface, size)?;
        device
            .bind_surface_to_context(context, surface)
            .map_err(|(err, mut surface)| {
                let _ = device.destroy_surface(context, &mut surface);
                err
            })
    }

    fn present_bound_surface(&self) -> Result<(), Error> {
        let device = &self.device.borrow();
        let context = &mut self.context.borrow_mut();

        let mut surface = device
            .unbind_surface_from_context(context)?
            // todo: proper error type. This probably should be done in surfman.
            .ok_or(Error::Failed)
            .inspect_err(|_| log::error!("Unable to present bound surface: no surface bound"))?;
        device.present_surface(context, &mut surface)?;
        device
            .bind_surface_to_context(context, surface)
            .map_err(|(err, mut surface)| {
                let _ = device.destroy_surface(context, &mut surface);
                err
            })
    }

    #[expect(dead_code)]
    fn native_context(&self) -> NativeContext {
        let device = &self.device.borrow();
        let context = &self.context.borrow();
        device.native_context(context)
    }

    fn framebuffer(&self) -> Option<NativeFramebuffer> {
        let device = &self.device.borrow();
        let context = &self.context.borrow();
        device
            .context_surface_info(context)
            .unwrap_or(None)
            .and_then(|info| info.framebuffer_object)
    }

    fn prepare_for_rendering(&self) {
        let framebuffer_id = self
            .framebuffer()
            .map_or(0, |framebuffer| framebuffer.0.into());
        self.gleam_gl
            .bind_framebuffer(gleam::gl::FRAMEBUFFER, framebuffer_id);
    }

    fn read_to_image(&self, source_rectangle: DeviceIntRect) -> Option<RgbaImage> {
        let framebuffer_id = self
            .framebuffer()
            .map_or(0, |framebuffer| framebuffer.0.into());
        Framebuffer::read_framebuffer_to_image(&self.gleam_gl, framebuffer_id, source_rectangle)
    }

    fn make_current(&self) -> Result<(), Error> {
        let device = &self.device.borrow();
        let context = &mut self.context.borrow();
        device.make_context_current(context)
    }

    fn refresh_driver(&self) -> Option<Rc<dyn RefreshDriver>> {
        self.refresh_driver.clone()
    }
}

/// A software rendering context that uses a software OpenGL implementation to render
/// Servo. This will generally have bad performance, but can be used in situations where
/// it is more convenient to have consistent, but slower display output.
///
/// The results of the render can be accessed via [`RenderingContext::read_to_image`].
pub struct SoftwareRenderingContext {
    size: Cell<PhysicalSize<u32>>,
    surfman_rendering_info: SurfmanRenderingContext,
    swap_chain: SwapChain<Device>,
}

impl SoftwareRenderingContext {
    pub fn new(size: PhysicalSize<u32>) -> Result<Self, Error> {
        let connection = Connection::new()?;
        let adapter = connection.create_software_adapter()?;
        let surfman_rendering_info = SurfmanRenderingContext::new(&connection, &adapter, None)?;

        let surfman_size = Size2D::new(size.width as i32, size.height as i32);
        let surface =
            surfman_rendering_info.create_surface(SurfaceType::Generic { size: surfman_size })?;
        surfman_rendering_info.bind_surface(surface)?;
        surfman_rendering_info.make_current()?;

        let swap_chain = surfman_rendering_info.create_attached_swap_chain()?;
        Ok(SoftwareRenderingContext {
            size: Cell::new(size),
            surfman_rendering_info,
            swap_chain,
        })
    }
}

impl Drop for SoftwareRenderingContext {
    fn drop(&mut self) {
        let device = &mut self.surfman_rendering_info.device.borrow_mut();
        let context = &mut self.surfman_rendering_info.context.borrow_mut();
        let _ = self.swap_chain.destroy(device, context);
    }
}

impl RenderingContext for SoftwareRenderingContext {
    fn prepare_for_rendering(&self) {
        self.surfman_rendering_info.prepare_for_rendering();
    }

    fn read_to_image(&self, source_rectangle: DeviceIntRect) -> Option<RgbaImage> {
        self.surfman_rendering_info.read_to_image(source_rectangle)
    }

    fn size(&self) -> PhysicalSize<u32> {
        self.size.get()
    }

    fn resize(&self, size: PhysicalSize<u32>) {
        if self.size.get() == size {
            return;
        }

        self.size.set(size);

        let device = &mut self.surfman_rendering_info.device.borrow_mut();
        let context = &mut self.surfman_rendering_info.context.borrow_mut();
        let size = Size2D::new(size.width as i32, size.height as i32);
        let _ = self.swap_chain.resize(device, context, size);
    }

    fn present(&self) {
        let device = &mut self.surfman_rendering_info.device.borrow_mut();
        let context = &mut self.surfman_rendering_info.context.borrow_mut();
        let _ = self
            .swap_chain
            .swap_buffers(device, context, PreserveBuffer::No);
    }

    fn make_current(&self) -> Result<(), Error> {
        self.surfman_rendering_info.make_current()
    }

    fn gleam_gl_api(&self) -> Rc<dyn gleam::gl::Gl> {
        self.surfman_rendering_info.gleam_gl.clone()
    }

    fn glow_gl_api(&self) -> Arc<glow::Context> {
        self.surfman_rendering_info.glow_gl.clone()
    }

}

/// A [`RenderingContext`] that uses the `surfman` library to render to a
/// `raw-window-handle` identified window. `surfman` will attempt to create an
/// OpenGL context and surface for this window. This is a simple implementation
/// of the [`RenderingContext`] crate, but by default it paints to the entire window
/// surface.
///
/// If you would like to paint to only a portion of the window, consider using
/// [`OffscreenRenderingContext`] by calling [`WindowRenderingContext::offscreen_context`].
pub struct WindowRenderingContext {
    /// The inner size of the window in physical pixels which excludes OS decorations.
    size: Cell<PhysicalSize<u32>>,
    surfman_context: SurfmanRenderingContext,
}

impl WindowRenderingContext {
    pub fn new(
        display_handle: DisplayHandle,
        window_handle: WindowHandle,
        size: PhysicalSize<u32>,
    ) -> Result<Self, Error> {
        Self::new_with_optional_refresh_driver(display_handle, window_handle, size, None)
    }

    pub fn new_with_refresh_driver(
        display_handle: DisplayHandle,
        window_handle: WindowHandle,
        size: PhysicalSize<u32>,
        refresh_driver: Rc<dyn RefreshDriver>,
    ) -> Result<Self, Error> {
        Self::new_with_optional_refresh_driver(
            display_handle,
            window_handle,
            size,
            Some(refresh_driver),
        )
    }

    fn new_with_optional_refresh_driver(
        display_handle: DisplayHandle,
        window_handle: WindowHandle,
        size: PhysicalSize<u32>,
        refresh_driver: Option<Rc<dyn RefreshDriver>>,
    ) -> Result<Self, Error> {
        let connection = Connection::from_display_handle(display_handle)?;
        let adapter = connection.create_adapter()?;
        let surfman_context = SurfmanRenderingContext::new(&connection, &adapter, refresh_driver)?;

        let native_widget = connection
            .create_native_widget_from_window_handle(
                window_handle,
                Size2D::new(size.width as i32, size.height as i32),
            )
            .expect("Failed to create native widget");

        let surface = surfman_context.create_surface(SurfaceType::Widget { native_widget })?;
        surfman_context.bind_surface(surface)?;
        surfman_context.make_current()?;

        Ok(Self {
            size: Cell::new(size),
            surfman_context,
        })
    }

    pub fn offscreen_context(
        self: &Rc<Self>,
        size: PhysicalSize<u32>,
    ) -> OffscreenRenderingContext {
        OffscreenRenderingContext::new(self.clone(), size)
    }

    /// Stop rendering to the window that was used to create this `WindowRenderingContext`
    /// or last set with [`Self::set_window`].
    ///
    /// TODO: This should be removed once `WebView`s can replace their `RenderingContext`s.
    pub fn take_window(&self) -> Result<(), Error> {
        let device = self.surfman_context.device.borrow_mut();
        let mut context = self.surfman_context.context.borrow_mut();
        let mut surface = device.unbind_surface_from_context(&mut context)?.unwrap();
        device.destroy_surface(&mut context, &mut surface)?;
        Ok(())
    }

    /// Replace the window that this [`WindowRenderingContext`] renders to and give it a new
    /// size.
    ///
    /// TODO: This should be removed once `WebView`s can replace their `RenderingContext`s.
    pub fn set_window(
        &self,
        window_handle: WindowHandle,
        size: PhysicalSize<u32>,
    ) -> Result<(), Error> {
        let device = self.surfman_context.device.borrow_mut();
        let mut context = self.surfman_context.context.borrow_mut();

        let native_widget = device
            .connection()
            .create_native_widget_from_window_handle(
                window_handle,
                Size2D::new(size.width as i32, size.height as i32),
            )
            .expect("Failed to create native widget");

        let surface_access = SurfaceAccess::GPUOnly;
        let surface_type = SurfaceType::Widget { native_widget };
        let surface = device.create_surface(&context, surface_access, surface_type)?;

        device
            .bind_surface_to_context(&mut context, surface)
            .map_err(|(err, mut surface)| {
                let _ = device.destroy_surface(&mut context, &mut surface);
                err
            })?;
        device.make_context_current(&context)?;
        Ok(())
    }

    pub fn surfman_details(&self) -> (RefMut<'_, Device>, RefMut<'_, Context>) {
        (
            self.surfman_context.device.borrow_mut(),
            self.surfman_context.context.borrow_mut(),
        )
    }
}

impl RenderingContext for WindowRenderingContext {
    fn prepare_for_rendering(&self) {
        self.surfman_context.prepare_for_rendering();
    }

    fn read_to_image(&self, source_rectangle: DeviceIntRect) -> Option<RgbaImage> {
        self.surfman_context.read_to_image(source_rectangle)
    }

    fn size(&self) -> PhysicalSize<u32> {
        self.size.get()
    }

    fn resize(&self, size: PhysicalSize<u32>) {
        match self.surfman_context.resize_surface(size) {
            Ok(..) => self.size.set(size),
            Err(error) => warn!("Error resizing surface: {error:?}"),
        }
    }

    fn present(&self) {
        if let Err(error) = self.surfman_context.present_bound_surface() {
            warn!("Error presenting surface: {error:?}");
        }
    }

    fn make_current(&self) -> Result<(), Error> {
        self.surfman_context.make_current()
    }

    fn gleam_gl_api(&self) -> Rc<dyn gleam::gl::Gl> {
        self.surfman_context.gleam_gl.clone()
    }

    fn glow_gl_api(&self) -> Arc<glow::Context> {
        self.surfman_context.glow_gl.clone()
    }

    fn refresh_driver(&self) -> Option<Rc<dyn RefreshDriver>> {
        self.surfman_context.refresh_driver()
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

impl Framebuffer {
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
    /// The caller is responsible for the texture's lifecycle.
    fn new_with_external_texture(
        gl: Rc<dyn Gl>,
        size: PhysicalSize<u32>,
        texture_id: gl::GLuint,
    ) -> Self {
        let framebuffer_ids = gl.gen_framebuffers(1);
        gl.bind_framebuffer(gl::FRAMEBUFFER, framebuffer_ids[0]);

        gl.framebuffer_texture_2d(
            gl::FRAMEBUFFER,
            gl::COLOR_ATTACHMENT0,
            gl::TEXTURE_2D,
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
    fn set_external_texture(&mut self, new_texture_id: gl::GLuint, size: PhysicalSize<u32>) {
        self.gl
            .bind_framebuffer(gl::FRAMEBUFFER, self.framebuffer_id);
        self.gl.framebuffer_texture_2d(
            gl::FRAMEBUFFER,
            gl::COLOR_ATTACHMENT0,
            gl::TEXTURE_2D,
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
        // I think this can only be some kind of synchronization
        // bug in OSMesa, but explicitly un-binding any vertex
        // array here seems to work around that bug.
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

pub struct OffscreenRenderingContext {
    parent_context: Rc<WindowRenderingContext>,
    size: Cell<PhysicalSize<u32>>,
    framebuffer: RefCell<Framebuffer>,
}

type RenderToParentCallback = Box<dyn Fn(&glow::Context, Rect<i32>) + Send + Sync>;

impl OffscreenRenderingContext {
    fn new(parent_context: Rc<WindowRenderingContext>, size: PhysicalSize<u32>) -> Self {
        let framebuffer = RefCell::new(Framebuffer::new(parent_context.gleam_gl_api(), size));
        Self {
            parent_context,
            size: Cell::new(size),
            framebuffer,
        }
    }

    pub fn parent_context(&self) -> &WindowRenderingContext {
        &self.parent_context
    }

    pub fn render_to_parent_callback(&self) -> Option<RenderToParentCallback> {
        // Don't accept a `None` context for the source framebuffer.
        let front_framebuffer_id =
            NonZeroU32::new(self.framebuffer.borrow().framebuffer_id).map(NativeFramebuffer)?;
        let parent_context_framebuffer_id = self.parent_context.surfman_context.framebuffer();
        let size = self.size.get();
        let size = Size2D::new(size.width as i32, size.height as i32);
        Some(Box::new(move |gl, target_rect| {
            Self::blit_framebuffer(
                gl,
                Rect::new(Point2D::origin(), size.to_i32()),
                front_framebuffer_id,
                target_rect,
                parent_context_framebuffer_id,
            );
        }))
    }

    #[expect(unsafe_code)]
    fn blit_framebuffer(
        gl: &glow::Context,
        source_rect: Rect<i32>,
        source_framebuffer_id: NativeFramebuffer,
        target_rect: Rect<i32>,
        target_framebuffer_id: Option<NativeFramebuffer>,
    ) {
        use glow::HasContext as _;
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
}

impl RenderingContext for OffscreenRenderingContext {
    fn size(&self) -> PhysicalSize<u32> {
        self.size.get()
    }

    fn resize(&self, new_size: PhysicalSize<u32>) {
        let old_size = self.size.get();
        if old_size == new_size {
            return;
        }

        let gl = self.parent_context.gleam_gl_api();
        let new_framebuffer = Framebuffer::new(gl.clone(), new_size);

        let old_framebuffer =
            std::mem::replace(&mut *self.framebuffer.borrow_mut(), new_framebuffer);
        self.size.set(new_size);

        let blit_size = new_size.min(old_size);
        let rect = Rect::new(
            Point2D::origin(),
            Size2D::new(blit_size.width, blit_size.height),
        )
        .to_i32();

        let Some(old_framebuffer_id) =
            NonZeroU32::new(old_framebuffer.framebuffer_id).map(NativeFramebuffer)
        else {
            return;
        };
        let new_framebuffer_id =
            NonZeroU32::new(self.framebuffer.borrow().framebuffer_id).map(NativeFramebuffer);
        Self::blit_framebuffer(
            &self.glow_gl_api(),
            rect,
            old_framebuffer_id,
            rect,
            new_framebuffer_id,
        );
    }

    fn prepare_for_rendering(&self) {
        self.framebuffer.borrow().bind();
    }

    fn present(&self) {}

    fn make_current(&self) -> Result<(), surfman::Error> {
        self.parent_context.make_current()
    }

    fn gleam_gl_api(&self) -> Rc<dyn gleam::gl::Gl> {
        self.parent_context.gleam_gl_api()
    }

    fn glow_gl_api(&self) -> Arc<glow::Context> {
        self.parent_context.glow_gl_api()
    }

    fn read_to_image(&self, source_rectangle: DeviceIntRect) -> Option<RgbaImage> {
        self.framebuffer.borrow().read_to_image(source_rectangle)
    }

    fn refresh_driver(&self) -> Option<Rc<dyn RefreshDriver>> {
        self.parent_context().refresh_driver()
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
    display_info: Option<GlDisplayInfo>,
}

impl MakepadRenderingContext {
    /// Create a new MakepadRenderingContext using the host's GL context directly.
    ///
    /// GL function pointers are loaded via the provided loader (typically backed
    /// by `eglGetProcAddress`). No second GL context is created — Servo renders
    /// into an FBO within Makepad's own GL context.
    ///
    /// # Safety
    /// The host GL context must be current on the calling thread.
    #[expect(unsafe_code)]
    pub unsafe fn new_from_loader(
        size: PhysicalSize<u32>,
        gl_loader: &dyn Fn(&str) -> *const std::ffi::c_void,
        display_info: Option<GlDisplayInfo>,
    ) -> Result<Self, Error> {
        debug!("MakepadRenderingContext (direct context): size={:?}", size);

        let gleam_gl: Rc<dyn Gl> = unsafe {
            gl::GlesFns::load_with(|name| gl_loader(name))
        };
        let glow_gl = unsafe {
            Arc::new(glow::Context::from_loader_function(|name| gl_loader(name)))
        };

        let framebuffer = RefCell::new(Framebuffer::new(gleam_gl.clone(), size));

        Ok(MakepadRenderingContext {
            size: Cell::new(size),
            gleam_gl,
            glow_gl,
            framebuffer,
            display_info,
        })
    }

    /// Switch to using an externally-owned texture as the render target.
    /// The texture must be valid in the current GL context.
    pub fn set_external_texture(&self, texture_id: gl::GLuint, size: PhysicalSize<u32>) {
        let mut fb = self.framebuffer.borrow_mut();
        if fb.owns_texture {
            // First time switching: replace entire framebuffer
            let gl = fb.gl.clone();
            let new_fb = Framebuffer::new_with_external_texture(gl, size, texture_id);
            *fb = new_fb; // Old fb dropped here, deleting its owned texture + FBO + RB
        } else {
            // Already using external: just reattach
            fb.set_external_texture(texture_id, size);
        }
        drop(fb);
        self.size.set(size);
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

    fn make_current(&self) -> Result<(), surfman::Error> {
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

// ---------------------------------------------------------------------------
// macOS IOSurface-backed CGL rendering context
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
const GL_TEXTURE_RECTANGLE: u32 = 0x84F5;
#[cfg(target_os = "macos")]
const GL_BGRA: u32 = 0x80E1;
#[cfg(target_os = "macos")]
const GL_UNSIGNED_INT_8_8_8_8_REV: u32 = 0x8367;


#[cfg(target_os = "macos")]
#[expect(unsafe_code)]
unsafe extern "C" {
    fn dlopen(filename: *const i8, flags: i32) -> *mut std::ffi::c_void;
    fn dlsym(handle: *mut std::ffi::c_void, symbol: *const i8) -> *const std::ffi::c_void;
}

#[cfg(target_os = "macos")]
const RTLD_LAZY: i32 = 1;

#[cfg(target_os = "macos")]
struct DlHandle(*mut std::ffi::c_void);
#[cfg(target_os = "macos")]
#[expect(unsafe_code)]
unsafe impl Send for DlHandle {}
#[cfg(target_os = "macos")]
#[expect(unsafe_code)]
unsafe impl Sync for DlHandle {}

#[cfg(target_os = "macos")]
#[expect(unsafe_code)]
fn macos_gl_proc_address(name: &str) -> *const std::ffi::c_void {
    use std::sync::OnceLock;
    static LIB: OnceLock<DlHandle> = OnceLock::new();
    let lib = LIB.get_or_init(|| unsafe {
        DlHandle(dlopen(
            b"/System/Library/Frameworks/OpenGL.framework/OpenGL\0".as_ptr() as *const _,
            RTLD_LAZY,
        ))
    }).0;
    if lib.is_null() {
        return std::ptr::null();
    }
    let c_name = std::ffi::CString::new(name).unwrap();
    unsafe { dlsym(lib, c_name.as_ptr()) }
}

/// A [`RenderingContext`] for macOS that creates a CGL context rendering into an
/// IOSurface-backed FBO. The IOSurface is created and owned by Makepad (via Metal);
/// CGL renders into it via `CGLTexImageIOSurface2D`, and Makepad's Metal reads the
/// same IOSurface for zero-copy display.
#[cfg(target_os = "macos")]
pub struct MacosRenderingContext {
    cgl_context: cgl::CGLContextObj,
    pixel_format: cgl::CGLPixelFormatObj,
    gleam_gl: Rc<dyn Gl>,
    glow_gl: Arc<glow::Context>,
    size: Cell<PhysicalSize<u32>>,
    fbo_id: Cell<u32>,
    texture_id: Cell<u32>,
    depth_rbo: Cell<u32>,
    stencil_rbo: Cell<u32>,
}

#[cfg(target_os = "macos")]
impl MacosRenderingContext {
    /// Create a new `MacosRenderingContext` that renders into the given IOSurface.
    ///
    /// # Safety
    /// `iosurface_ref` must be a valid `IOSurfaceRef` that remains alive for the
    /// duration of rendering. The IOSurface is owned by Makepad.
    #[expect(unsafe_code)]
    pub unsafe fn new(
        size: PhysicalSize<u32>,
        iosurface_ref: *mut std::ffi::c_void,
    ) -> Result<Self, Error> {
        debug!("MacosRenderingContext: size={:?}", size);

        // 1. Create CGL pixel format
        let attributes: [cgl::CGLPixelFormatAttribute; 9] = [
            cgl::kCGLPFAOpenGLProfile,
            0x3200, // GL 3.2 Core
            cgl::kCGLPFAAlphaSize,
            8,
            cgl::kCGLPFADepthSize,
            24,
            cgl::kCGLPFAStencilSize,
            8,
            0, // null terminator
        ];
        let mut pixel_format: cgl::CGLPixelFormatObj = std::ptr::null_mut();
        let mut num_formats: i32 = 0;
        let err = unsafe {
            cgl::CGLChoosePixelFormat(attributes.as_ptr(), &mut pixel_format, &mut num_formats)
        };
        if err != cgl::kCGLNoError {
            return Err(Error::Failed);
        }

        // 2. Create CGL context
        let mut cgl_context: cgl::CGLContextObj = std::ptr::null_mut();
        let err =
            unsafe { cgl::CGLCreateContext(pixel_format, std::ptr::null_mut(), &mut cgl_context) };
        if err != cgl::kCGLNoError {
            unsafe { cgl::CGLReleasePixelFormat(pixel_format) };
            return Err(Error::Failed);
        }

        // 3. Make current
        let err = unsafe { cgl::CGLSetCurrentContext(cgl_context) };
        if err != cgl::kCGLNoError {
            unsafe {
                cgl::CGLDestroyContext(cgl_context);
                cgl::CGLReleasePixelFormat(pixel_format);
            }
            return Err(Error::Failed);
        }

        // 4. Load GL functions
        let gleam_gl: Rc<dyn Gl> =
            unsafe { gl::GlFns::load_with(|name| macos_gl_proc_address(name)) };
        let glow_gl = unsafe {
            Arc::new(glow::Context::from_loader_function(|name| {
                macos_gl_proc_address(name)
            }))
        };

        // 5-9. Create FBO resources bound to IOSurface
        let (tex, fbo, depth, stencil) =
            unsafe { Self::create_fbo_resources(&gleam_gl, cgl_context, iosurface_ref, size)? };

        Ok(MacosRenderingContext {
            cgl_context,
            pixel_format,
            gleam_gl,
            glow_gl,
            size: Cell::new(size),
            fbo_id: Cell::new(fbo),
            texture_id: Cell::new(tex),
            depth_rbo: Cell::new(depth),
            stencil_rbo: Cell::new(stencil),
        })
    }

    /// Create texture + FBO + renderbuffers bound to the given IOSurface.
    /// CGL context must be current.
    #[expect(unsafe_code)]
    unsafe fn create_fbo_resources(
        gleam_gl: &Rc<dyn Gl>,
        cgl_context: cgl::CGLContextObj,
        iosurface_ref: *mut std::ffi::c_void,
        size: PhysicalSize<u32>,
    ) -> Result<(u32, u32, u32, u32), Error> {
        let w = size.width as i32;
        let h = size.height as i32;

        // Create texture
        let tex = gleam_gl.gen_textures(1)[0];
        gleam_gl.bind_texture(GL_TEXTURE_RECTANGLE, tex);

        // Bind IOSurface to texture
        let err = unsafe {
            cgl::CGLTexImageIOSurface2D(
                cgl_context,
                GL_TEXTURE_RECTANGLE,
                gl::RGBA as u32,
                w,
                h,
                GL_BGRA,
                GL_UNSIGNED_INT_8_8_8_8_REV,
                iosurface_ref as cgl::IOSurfaceRef,
                0,
            )
        };
        if err != cgl::kCGLNoError {
            gleam_gl.delete_textures(&[tex]);
            return Err(Error::Failed);
        }

        // Create FBO
        let fbo = gleam_gl.gen_framebuffers(1)[0];
        gleam_gl.bind_framebuffer(gl::FRAMEBUFFER, fbo);
        gleam_gl.framebuffer_texture_2d(
            gl::FRAMEBUFFER,
            gl::COLOR_ATTACHMENT0,
            GL_TEXTURE_RECTANGLE,
            tex,
            0,
        );

        // Combined depth+stencil renderbuffer (separate attachments are
        // GL_FRAMEBUFFER_UNSUPPORTED with GL_TEXTURE_RECTANGLE on Intel GPUs)
        let depth_stencil = gleam_gl.gen_renderbuffers(1)[0];
        gleam_gl.bind_renderbuffer(gl::RENDERBUFFER, depth_stencil);
        gleam_gl.renderbuffer_storage(gl::RENDERBUFFER, gl::DEPTH24_STENCIL8, w, h);
        gleam_gl.framebuffer_renderbuffer(
            gl::FRAMEBUFFER,
            gl::DEPTH_STENCIL_ATTACHMENT,
            gl::RENDERBUFFER,
            depth_stencil,
        );
        let depth = depth_stencil;
        let stencil = depth_stencil;

        // Check completeness
        let status = gleam_gl.check_frame_buffer_status(gl::FRAMEBUFFER);
        if status != gl::FRAMEBUFFER_COMPLETE {
            gleam_gl.delete_framebuffers(&[fbo]);
            gleam_gl.delete_renderbuffers(&[depth]);
            gleam_gl.delete_textures(&[tex]);
            return Err(Error::Failed);
        }

        Ok((tex, fbo, depth, stencil))
    }

    /// Delete the current FBO resources (texture, renderbuffers, FBO).
    fn delete_fbo_resources(&self) {
        self.gleam_gl
            .delete_framebuffers(&[self.fbo_id.get()]);
        // depth_rbo and stencil_rbo are the same combined renderbuffer
        self.gleam_gl
            .delete_renderbuffers(&[self.depth_rbo.get()]);
        self.gleam_gl
            .delete_textures(&[self.texture_id.get()]);
    }

    /// Rebind to a new IOSurface at a new size. Destroys old FBO resources and
    /// creates new ones.
    ///
    /// # Safety
    /// `iosurface_ref` must be a valid `IOSurfaceRef`. CGL context will be made current.
    #[expect(unsafe_code)]
    pub unsafe fn rebind_iosurface(
        &self,
        iosurface_ref: *mut std::ffi::c_void,
        size: PhysicalSize<u32>,
    ) {
        unsafe { cgl::CGLSetCurrentContext(self.cgl_context) };
        self.delete_fbo_resources();

        match unsafe {
            Self::create_fbo_resources(&self.gleam_gl, self.cgl_context, iosurface_ref, size)
        } {
            Ok((tex, fbo, depth, stencil)) => {
                self.texture_id.set(tex);
                self.fbo_id.set(fbo);
                self.depth_rbo.set(depth);
                self.stencil_rbo.set(stencil);
                self.size.set(size);
            },
            Err(e) => {
                warn!("MacosRenderingContext::rebind_iosurface failed: {:?}", e);
            },
        }
    }
}

#[cfg(target_os = "macos")]
impl RenderingContext for MacosRenderingContext {
    fn prepare_for_rendering(&self) {
        let _ = self.make_current();
        self.gleam_gl
            .bind_framebuffer(gl::FRAMEBUFFER, self.fbo_id.get());
    }

    fn present(&self) {
        self.gleam_gl.flush();
        self.gleam_gl.bind_framebuffer(gl::FRAMEBUFFER, 0);
    }

    #[expect(unsafe_code)]
    fn make_current(&self) -> Result<(), Error> {
        let err = unsafe { cgl::CGLSetCurrentContext(self.cgl_context) };
        if err != cgl::kCGLNoError {
            return Err(surfman::Error::MakeCurrentFailed(
                surfman::WindowingApiError::Failed,
            ));
        }
        Ok(())
    }

    fn size(&self) -> PhysicalSize<u32> {
        self.size.get()
    }

    fn resize(&self, _size: PhysicalSize<u32>) {
        // No-op: resize is done via rebind_iosurface from the caller.
    }

    fn gleam_gl_api(&self) -> Rc<dyn gleam::gl::Gl> {
        self.gleam_gl.clone()
    }

    fn glow_gl_api(&self) -> Arc<glow::Context> {
        self.glow_gl.clone()
    }

    fn read_to_image(&self, source_rectangle: DeviceIntRect) -> Option<RgbaImage> {
        let _ = self.make_current();
        self.gleam_gl
            .bind_framebuffer(gl::FRAMEBUFFER, self.fbo_id.get());
        Framebuffer::read_framebuffer_to_image(&self.gleam_gl, self.fbo_id.get(), source_rectangle)
    }
}

#[cfg(target_os = "macos")]
impl Drop for MacosRenderingContext {
    #[expect(unsafe_code)]
    fn drop(&mut self) {
        unsafe { cgl::CGLSetCurrentContext(self.cgl_context) };
        self.delete_fbo_resources();
        unsafe {
            cgl::CGLSetCurrentContext(std::ptr::null_mut());
            cgl::CGLDestroyContext(self.cgl_context);
            cgl::CGLReleasePixelFormat(self.pixel_format);
        }
    }
}

#[cfg(test)]
mod test {
    use dpi::PhysicalSize;
    use euclid::{Box2D, Point2D, Size2D};
    use gleam::gl;
    use image::Rgba;
    use surfman::{Connection, ContextAttributeFlags, ContextAttributes, Error, GLApi, GLVersion};

    use super::Framebuffer;

    #[test]
    #[expect(unsafe_code)]
    fn test_read_pixels() -> Result<(), Error> {
        let connection = Connection::new()?;
        let adapter = connection.create_software_adapter()?;
        let device = connection.create_device(&adapter)?;
        let context_descriptor = device.create_context_descriptor(&ContextAttributes {
            version: GLVersion::new(3, 0),
            flags: ContextAttributeFlags::empty(),
        })?;
        let mut context = device.create_context(&context_descriptor, None)?;

        let gl = match connection.gl_api() {
            GLApi::GL => unsafe { gl::GlFns::load_with(|s| device.get_proc_address(&context, s)) },
            GLApi::GLES => unsafe {
                gl::GlesFns::load_with(|s| device.get_proc_address(&context, s))
            },
        };

        device.make_context_current(&context)?;

        {
            const SIZE: u32 = 16;
            let framebuffer = Framebuffer::new(gl, PhysicalSize::new(SIZE, SIZE));
            framebuffer.bind();
            framebuffer
                .gl
                .clear_color(12.0 / 255.0, 34.0 / 255.0, 56.0 / 255.0, 78.0 / 255.0);
            framebuffer.gl.clear(gl::COLOR_BUFFER_BIT);

            let rect = Box2D::from_origin_and_size(Point2D::zero(), Size2D::new(SIZE, SIZE));
            let img = framebuffer
                .read_to_image(rect.to_i32())
                .expect("Should have been able to read back image.");
            assert_eq!(img.width(), SIZE);
            assert_eq!(img.height(), SIZE);

            let expected_pixel: Rgba<u8> = Rgba([12, 34, 56, 78]);
            assert!(img.pixels().all(|&p| p == expected_pixel));
        }

        device.destroy_context(&mut context)?;

        Ok(())
    }
}
