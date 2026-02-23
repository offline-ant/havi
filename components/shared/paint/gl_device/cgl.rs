#![allow(unsafe_code)]

use std::ffi::{c_char, c_void, CString};
use std::ptr;
use std::sync::OnceLock;

/// CGL display info. CGL is self-contained on macOS — no external display handle needed.
/// The share_context enables resource sharing between contexts.
#[derive(Clone)]
pub struct CglDisplayInfo {
    pub pixel_format: *mut c_void, // CGLPixelFormatObj
    pub share_context: *mut c_void, // CGLContextObj for sharing (can be null for root)
}

unsafe impl Send for CglDisplayInfo {}
unsafe impl Sync for CglDisplayInfo {}

pub(crate) struct CglBackend {
    pixel_format: *mut c_void,
    share_context: *mut c_void,
}

impl CglBackend {
    /// Create a CGL backend from external display info.
    pub fn new(info: &CglDisplayInfo) -> Self {
        CglBackend {
            pixel_format: info.pixel_format,
            share_context: info.share_context,
        }
    }

    /// Create a standalone CGL backend with its own pixel format (GL 3.2 Core,
    /// RGBA8, depth24, stencil8) and a root sharing context.
    pub fn new_standalone() -> Self {
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
        let mut pixel_format: cgl::CGLPixelFormatObj = ptr::null_mut();
        let mut num_formats: i32 = 0;
        let err = unsafe {
            cgl::CGLChoosePixelFormat(attributes.as_ptr(), &mut pixel_format, &mut num_formats)
        };
        assert!(
            err == cgl::kCGLNoError && !pixel_format.is_null(),
            "CGLChoosePixelFormat failed: {err}"
        );

        // Create a root context for resource sharing.
        let mut root_ctx: cgl::CGLContextObj = ptr::null_mut();
        let err = unsafe { cgl::CGLCreateContext(pixel_format, ptr::null_mut(), &mut root_ctx) };
        assert!(
            err == cgl::kCGLNoError && !root_ctx.is_null(),
            "CGLCreateContext (root) failed: {err}"
        );

        CglBackend {
            pixel_format: pixel_format as *mut c_void,
            share_context: root_ctx as *mut c_void,
        }
    }

    /// Create a new CGL context.
    ///
    /// If `share_with` is `Some`, shares resources with that context.
    /// Otherwise shares with the stored root context.
    ///
    /// Panics on CGL failure.
    pub fn create_context(&self, share_with: Option<*mut c_void>) -> *mut c_void {
        let share = share_with.unwrap_or(self.share_context) as cgl::CGLContextObj;
        let mut ctx: cgl::CGLContextObj = ptr::null_mut();
        let err = unsafe {
            cgl::CGLCreateContext(self.pixel_format as cgl::CGLPixelFormatObj, share, &mut ctx)
        };
        assert!(
            err == cgl::kCGLNoError && !ctx.is_null(),
            "CGLCreateContext failed: {err}"
        );
        ctx as *mut c_void
    }

    /// Make a CGL context current.
    ///
    /// Panics on CGL failure.
    pub fn make_context_current(&self, ctx: *mut c_void) {
        let err = unsafe { cgl::CGLSetCurrentContext(ctx as cgl::CGLContextObj) };
        assert!(
            err == cgl::kCGLNoError,
            "CGLSetCurrentContext failed: {err}"
        );
    }

    /// Unbind the current context (make no context current).
    ///
    /// Panics on CGL failure.
    pub fn unbind_context(&self) {
        let err = unsafe { cgl::CGLSetCurrentContext(ptr::null_mut()) };
        assert!(
            err == cgl::kCGLNoError,
            "CGLSetCurrentContext(null) failed: {err}"
        );
    }

    /// Destroy a CGL context. Does not release the pixel format (owned by the backend).
    ///
    /// Panics on CGL failure.
    pub fn destroy_context(&self, ctx: *mut c_void) {
        let err = unsafe { cgl::CGLDestroyContext(ctx as cgl::CGLContextObj) };
        assert!(
            err == cgl::kCGLNoError,
            "CGLDestroyContext failed: {err}"
        );
    }

    /// Look up a GL function by name from OpenGL.framework via dlsym.
    pub fn get_proc_address(&self, name: &str) -> *mut c_void {
        gl_proc_address(name) as *mut c_void
    }

    /// Returns `GlApi::GL` — macOS always uses desktop GL via CGL.
    pub fn gl_api(&self) -> super::GlApi {
        super::GlApi::GL
    }
}

// ---------------------------------------------------------------------------
// OpenGL.framework function loading via dlsym
// ---------------------------------------------------------------------------

unsafe extern "C" {
    fn dlopen(filename: *const c_char, flags: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *const c_void;
}

const RTLD_LAZY: i32 = 1;

struct DlHandle(*mut c_void);
unsafe impl Send for DlHandle {}
unsafe impl Sync for DlHandle {}

fn gl_proc_address(name: &str) -> *const c_void {
    static LIB: OnceLock<DlHandle> = OnceLock::new();
    let lib = LIB.get_or_init(|| unsafe {
        DlHandle(dlopen(
            b"/System/Library/Frameworks/OpenGL.framework/OpenGL\0".as_ptr() as *const _,
            RTLD_LAZY,
        ))
    }).0;
    if lib.is_null() {
        return ptr::null();
    }
    let c_name = CString::new(name).unwrap();
    unsafe { dlsym(lib, c_name.as_ptr()) }
}
