#![allow(unsafe_code)]

use std::ffi::{c_char, c_void, CString};
use std::ptr;

// EGL constants
const EGL_NO_SURFACE: *mut c_void = ptr::null_mut();
const EGL_NO_CONTEXT: *mut c_void = ptr::null_mut();
const EGL_CONTEXT_MAJOR_VERSION: i32 = 0x3098;
const EGL_CONTEXT_MINOR_VERSION: i32 = 0x30FB; // EGL_CONTEXT_MINOR_VERSION_KHR
const EGL_NONE: i32 = 0x3038;

// EGL function pointer types
type EglCreateContextFn =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const i32) -> *mut c_void;
type EglDestroyContextFn = unsafe extern "C" fn(*mut c_void, *mut c_void) -> u32;
type EglMakeCurrentFn =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut c_void) -> u32;
type EglGetErrorFn = unsafe extern "C" fn() -> i32;

/// EGL display information from the host (e.g. makepad's OpenglCx).
#[derive(Clone)]
pub struct EglDisplayInfo {
    pub display: *mut c_void,
    pub config: *mut c_void,
    pub share_context: *mut c_void,
    /// eglGetProcAddress function pointer.
    pub get_proc_address: unsafe extern "C" fn(*const c_char) -> *mut c_void,
}

unsafe impl Send for EglDisplayInfo {}
unsafe impl Sync for EglDisplayInfo {}

pub(crate) struct EglBackend {
    display: *mut c_void,
    config: *mut c_void,
    share_context: *mut c_void,
    get_proc_address: unsafe extern "C" fn(*const c_char) -> *mut c_void,
    // EGL function pointers loaded at construction
    egl_create_context: EglCreateContextFn,
    egl_destroy_context: EglDestroyContextFn,
    egl_make_current: EglMakeCurrentFn,
    egl_get_error: EglGetErrorFn,
}

impl EglBackend {
    /// Create an EGL backend from host display info.
    ///
    /// Panics if any required EGL function pointer cannot be loaded.
    pub fn new(info: &EglDisplayInfo) -> Self {
        let gpa = info.get_proc_address;

        let egl_create_context: EglCreateContextFn = unsafe {
            let p = load_egl_fn(gpa, "eglCreateContext");
            std::mem::transmute(p)
        };
        let egl_destroy_context: EglDestroyContextFn = unsafe {
            let p = load_egl_fn(gpa, "eglDestroyContext");
            std::mem::transmute(p)
        };
        let egl_make_current: EglMakeCurrentFn = unsafe {
            let p = load_egl_fn(gpa, "eglMakeCurrent");
            std::mem::transmute(p)
        };
        let egl_get_error: EglGetErrorFn = unsafe {
            let p = load_egl_fn(gpa, "eglGetError");
            std::mem::transmute(p)
        };

        EglBackend {
            display: info.display,
            config: info.config,
            share_context: info.share_context,
            get_proc_address: gpa,
            egl_create_context,
            egl_destroy_context,
            egl_make_current,
            egl_get_error,
        }
    }

    /// Create a new EGL context for GLES 3.0.
    ///
    /// If `share_with` is `Some`, shares resources with that context.
    /// Otherwise shares with the stored host context.
    ///
    /// Panics on EGL failure.
    pub fn create_context(&self, share_with: Option<*mut c_void>) -> *mut c_void {
        let share = share_with.unwrap_or(self.share_context);
        let attribs: [i32; 5] = [
            EGL_CONTEXT_MAJOR_VERSION,
            3,
            EGL_CONTEXT_MINOR_VERSION,
            0,
            EGL_NONE,
        ];
        let ctx = unsafe {
            (self.egl_create_context)(self.display, self.config, share, attribs.as_ptr())
        };
        if ctx.is_null() || ctx == EGL_NO_CONTEXT {
            let err = unsafe { (self.egl_get_error)() };
            panic!(
                "eglCreateContext failed: error 0x{:04x} ({})",
                err as u32,
                egl_error_name(err)
            );
        }
        ctx
    }

    /// Make a context current with no read/draw surfaces (surfaceless rendering).
    ///
    /// Panics on EGL failure.
    pub fn make_context_current(&self, ctx: *mut c_void) {
        let ok =
            unsafe { (self.egl_make_current)(self.display, EGL_NO_SURFACE, EGL_NO_SURFACE, ctx) };
        if ok == 0 {
            let err = unsafe { (self.egl_get_error)() };
            panic!(
                "eglMakeCurrent failed: error 0x{:04x} ({})",
                err as u32,
                egl_error_name(err)
            );
        }
    }

    /// Unbind the current context (make no context current).
    ///
    /// Panics on EGL failure.
    pub fn unbind_context(&self) {
        let ok = unsafe {
            (self.egl_make_current)(self.display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT)
        };
        if ok == 0 {
            let err = unsafe { (self.egl_get_error)() };
            panic!(
                "eglMakeCurrent(EGL_NO_CONTEXT) failed: error 0x{:04x} ({})",
                err as u32,
                egl_error_name(err)
            );
        }
    }

    /// Destroy an EGL context.
    ///
    /// Panics on EGL failure.
    pub fn destroy_context(&self, ctx: *mut c_void) {
        let ok = unsafe { (self.egl_destroy_context)(self.display, ctx) };
        if ok == 0 {
            let err = unsafe { (self.egl_get_error)() };
            panic!(
                "eglDestroyContext failed: error 0x{:04x} ({})",
                err as u32,
                egl_error_name(err)
            );
        }
    }

    /// Look up a GL/EGL function by name.
    pub fn get_proc_address(&self, name: &str) -> *mut c_void {
        let cname = CString::new(name).expect("GL function name contains null byte");
        unsafe { (self.get_proc_address)(cname.as_ptr()) }
    }

    /// Returns `GlApi::GLES` — EGL on Linux/Android always uses OpenGL ES.
    pub fn gl_api(&self) -> super::GlApi {
        super::GlApi::GLES
    }
}

/// Load an EGL function pointer via eglGetProcAddress. Panics if null.
unsafe fn load_egl_fn(
    gpa: unsafe extern "C" fn(*const c_char) -> *mut c_void,
    name: &str,
) -> *mut c_void {
    let cname = CString::new(name).unwrap();
    let p = unsafe { gpa(cname.as_ptr()) };
    assert!(!p.is_null(), "failed to load EGL function: {}", name);
    p
}

fn egl_error_name(error: i32) -> &'static str {
    match error as u32 {
        0x3000 => "EGL_SUCCESS",
        0x3001 => "EGL_NOT_INITIALIZED",
        0x3002 => "EGL_BAD_ACCESS",
        0x3003 => "EGL_BAD_ALLOC",
        0x3004 => "EGL_BAD_ATTRIBUTE",
        0x3005 => "EGL_BAD_CONFIG",
        0x3006 => "EGL_BAD_CONTEXT",
        0x3007 => "EGL_BAD_CURRENT_SURFACE",
        0x3008 => "EGL_BAD_DISPLAY",
        0x3009 => "EGL_BAD_MATCH",
        0x300A => "EGL_BAD_NATIVE_PIXMAP",
        0x300B => "EGL_BAD_NATIVE_WINDOW",
        0x300C => "EGL_BAD_PARAMETER",
        0x300D => "EGL_BAD_SURFACE",
        0x300E => "EGL_CONTEXT_LOST",
        _ => "EGL_UNKNOWN_ERROR",
    }
}
