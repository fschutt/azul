//! macOS OpenGL function-pointer loader.
//!
//! Opens `OpenGL.framework` via `dlopen` and resolves every GL entry point
//! with `dlsym`, storing them in a [`GenericGlContext`] for the renderer.

use std::{
    ffi::{c_char, c_void, CString},
    fmt,
    rc::Rc,
};

use gl_context_loader::GenericGlContext;

use crate::desktop::shell2::common::gl_loader::load_gl_context;

// Ensure we can call dlopen/dlsym/dlclose
#[link(name = "dl")]
extern "C" {
    fn dlopen(filename: *const c_char, flag: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

// Wrapper to get access to the GL function pointers
pub struct GlFunctions {
    /// The handle returned by dlopen.
    _opengl_lib_handle: *mut c_void,

    /// Actual GL function pointers (glClear, glClearColor, etc.)
    pub functions: Rc<GenericGlContext>,
}

impl fmt::Debug for GlFunctions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Just show the pointer's numeric value
        write!(
            f,
            "GlFunctions {{ handle = {:p} }}",
            self._opengl_lib_handle
        )
    }
}

impl GlFunctions {
    /// Allocates and loads the OpenGL function pointers via dlopen
    pub fn initialize() -> Result<Self, String> {
        const RTLD_NOW: i32 = 2;
        const RTLD_GLOBAL: i32 = 8;

        // Full path to Apple's OpenGL framework library
        // Alternatively: "/System/Library/Frameworks/OpenGL.framework/OpenGL"
        let framework_path =
            CString::new("/System/Library/Frameworks/OpenGL.framework/OpenGL").unwrap();

        // RTLD_NOW means "resolve all symbols immediately".
        // RTLD_GLOBAL or RTLD_LOCAL depends on your use-case.
        // Miri can't call `dlopen`; report unavailable (null) so the guard bails.
        #[cfg(miri)]
        let handle: *mut c_void = core::ptr::null_mut();
        #[cfg(not(miri))]
        let handle = unsafe { dlopen(framework_path.as_ptr(), RTLD_NOW | RTLD_GLOBAL) };
        if handle.is_null() {
            return Err("Could not dlopen OpenGL.framework/OpenGL".to_string());
        }

        let context = load_gl_context(|s| {
            let c_string = CString::new(s).unwrap();
            (unsafe { dlsym(handle, c_string.as_ptr()) }) as *mut _
        });

        Ok(Self {
            _opengl_lib_handle: handle,
            functions: Rc::new(context),
        })
    }

    /// Returns the loaded function pointers (for use in your GL code).
    pub fn get_context(&self) -> Rc<GenericGlContext> {
        self.functions.clone()
    }
}

impl Drop for GlFunctions {
    fn drop(&mut self) {
        // Intentionally do NOT call dlclose here.
        // The Rc<GenericGlContext> returned by get_context() may outlive this
        // struct (e.g. in GLView ivars via prepare_opengl), and closing the
        // handle would invalidate those function pointers.
        // On macOS, dlclose on system frameworks is a no-op anyway.
    }
}
