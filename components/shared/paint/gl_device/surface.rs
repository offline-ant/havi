/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! FBO + texture surface management. Pure GL — no platform-specific code.

#![allow(unsafe_code)]

use euclid::default::Size2D;
use glow::{self as gl, HasContext};

/// A renderable surface backed by an FBO with a texture color attachment
/// and a depth24+stencil8 renderbuffer.
///
/// Textures are shared across GL contexts created from the same share group,
/// so a surface created in one context can be sampled as a texture in another.
pub struct GlSurface {
    pub(crate) framebuffer: gl::NativeFramebuffer,
    pub(crate) texture: gl::NativeTexture,
    pub(crate) depth_stencil_rb: gl::NativeRenderbuffer,
    /// Size of the surface in pixels.
    pub size: Size2D<i32>,
}

impl GlSurface {
    /// Create a new FBO+texture surface. The GL context must be current.
    pub fn new(gl: &glow::Context, size: Size2D<i32>) -> Self {
        unsafe {
            let texture = gl.create_texture().expect("Failed to create GL texture");
            gl.bind_texture(gl::TEXTURE_2D, Some(texture));
            gl.tex_image_2d(
                gl::TEXTURE_2D,
                0,
                gl::RGBA8 as i32,
                size.width,
                size.height,
                0,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                gl::PixelUnpackData::Slice(None),
            );
            gl.tex_parameter_i32(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl.tex_parameter_i32(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
            gl.bind_texture(gl::TEXTURE_2D, None);

            let depth_stencil_rb = gl
                .create_renderbuffer()
                .expect("Failed to create GL renderbuffer");
            gl.bind_renderbuffer(gl::RENDERBUFFER, Some(depth_stencil_rb));
            gl.renderbuffer_storage(
                gl::RENDERBUFFER,
                gl::DEPTH24_STENCIL8,
                size.width,
                size.height,
            );
            gl.bind_renderbuffer(gl::RENDERBUFFER, None);

            let framebuffer = gl
                .create_framebuffer()
                .expect("Failed to create GL framebuffer");
            gl.bind_framebuffer(gl::FRAMEBUFFER, Some(framebuffer));
            gl.framebuffer_texture_2d(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                Some(texture),
                0,
            );
            gl.framebuffer_renderbuffer(
                gl::FRAMEBUFFER,
                gl::DEPTH_STENCIL_ATTACHMENT,
                gl::RENDERBUFFER,
                Some(depth_stencil_rb),
            );

            let status = gl.check_framebuffer_status(gl::FRAMEBUFFER);
            assert_eq!(
                status,
                gl::FRAMEBUFFER_COMPLETE,
                "GlSurface framebuffer incomplete: 0x{:04x}",
                status
            );

            gl.bind_framebuffer(gl::FRAMEBUFFER, None);

            GlSurface {
                framebuffer,
                texture,
                depth_stencil_rb,
                size,
            }
        }
    }

    /// Destroy this surface's GL resources. The owning context must be current.
    pub fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_framebuffer(self.framebuffer);
            gl.delete_texture(self.texture);
            gl.delete_renderbuffer(self.depth_stencil_rb);
        }
    }

    /// GL texture id for sampling this surface in another context.
    pub fn texture_id(&self) -> u32 {
        self.texture.0.get()
    }
}
