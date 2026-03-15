#![allow(unsafe_code)]

use std::ffi::{CString, c_char, c_void};
use std::ptr;

unsafe extern "C" {
    fn objc_getClass(name: *const c_char) -> *mut c_void;
    fn sel_registerName(name: *const c_char) -> *mut c_void;
    fn objc_msgSend(receiver: *mut c_void, sel: *mut c_void, ...) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

/// EAGL display info for iOS. Carries the host EAGLContext (for sharegroup-based
/// context sharing) and a handle to OpenGLES.framework for GL function loading.
#[derive(Clone)]
pub struct EaglDisplayInfo {
    /// The host EAGLContext (ObjC id cast to `*mut c_void`).
    /// New contexts join its sharegroup for texture sharing.
    pub share_context: *mut c_void,
    /// `dlopen` handle for OpenGLES.framework.
    pub opengles_framework: *mut c_void,
}

unsafe impl Send for EaglDisplayInfo {}
unsafe impl Sync for EaglDisplayInfo {}

pub(crate) struct EaglBackend {
    share_context: *mut c_void,
    opengles_framework: *mut c_void,
}

impl EaglBackend {
    pub fn new(info: &EaglDisplayInfo) -> Self {
        EaglBackend {
            share_context: info.share_context,
            opengles_framework: info.opengles_framework,
        }
    }

    /// Create a new EAGL context sharing the same sharegroup.
    ///
    /// If `share_with` is `Some`, uses that context's sharegroup.
    /// Otherwise uses the stored host context's sharegroup.
    pub fn create_context(&self, share_with: Option<*mut c_void>) -> *mut c_void {
        let source = share_with.unwrap_or(self.share_context);
        unsafe {
            // [source sharegroup]
            let sel_sharegroup = sel_registerName(b"sharegroup\0".as_ptr() as *const _);
            let sharegroup = objc_msgSend(source, sel_sharegroup);
            assert!(!sharegroup.is_null(), "EAGLContext sharegroup is null");

            // [[EAGLContext alloc] initWithAPI:3 sharegroup:sharegroup]
            let cls = objc_getClass(b"EAGLContext\0".as_ptr() as *const _);
            let sel_alloc = sel_registerName(b"alloc\0".as_ptr() as *const _);
            let obj = objc_msgSend(cls, sel_alloc);

            let sel_init = sel_registerName(
                b"initWithAPI:sharegroup:\0".as_ptr() as *const _,
            );
            // kEAGLRenderingAPIOpenGLES3 = 3
            let ctx = objc_msgSend(obj, sel_init, 3u64, sharegroup);
            assert!(!ctx.is_null(), "EAGLContext initWithAPI:sharegroup: failed");

            ctx
        }
    }

    /// Make an EAGL context current on the calling thread.
    pub fn make_context_current(&self, ctx: *mut c_void) {
        unsafe {
            let cls = objc_getClass(b"EAGLContext\0".as_ptr() as *const _);
            let sel = sel_registerName(b"setCurrentContext:\0".as_ptr() as *const _);
            // setCurrentContext: returns BOOL (YES=1, NO=0), received as pointer-sized int.
            let ok = objc_msgSend(cls, sel, ctx) as usize;
            assert!(ok != 0, "EAGLContext setCurrentContext: failed");
        }
    }

    /// Destroy an EAGL context. Unbinds it first if current.
    pub fn destroy_context(&self, ctx: *mut c_void) {
        unsafe {
            // Check if this context is current and unbind if so.
            let cls = objc_getClass(b"EAGLContext\0".as_ptr() as *const _);
            let sel_current = sel_registerName(b"currentContext\0".as_ptr() as *const _);
            let current = objc_msgSend(cls, sel_current);
            if current == ctx {
                let sel_set = sel_registerName(b"setCurrentContext:\0".as_ptr() as *const _);
                objc_msgSend(cls, sel_set, ptr::null_mut::<c_void>());
            }

            // [ctx release]
            let sel_release = sel_registerName(b"release\0".as_ptr() as *const _);
            objc_msgSend(ctx, sel_release);
        }
    }

    /// Look up a GL function by name via dlsym on OpenGLES.framework.
    pub fn get_proc_address(&self, name: &str) -> *mut c_void {
        let c_name = CString::new(name).expect("GL function name contains null byte");
        unsafe { dlsym(self.opengles_framework, c_name.as_ptr()) }
    }

    /// Returns `GlApi::GLES` — iOS always uses OpenGL ES via EAGL.
    pub fn gl_api(&self) -> super::GlApi {
        super::GlApi::GLES
    }
}
