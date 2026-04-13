//! Common GL function loading for Linux (X11 and Wayland).
//!
//! The main type is [`GlFunctions`], which wraps all OpenGL function pointers
//! in an `Rc<GenericGlContext>`. Call [`GlFunctions::initialize`] with a loaded
//! [`Egl`](super::super::x11::dlopen::Egl) instance to resolve symbols via
//! `eglGetProcAddress` (with `libGL.so.1` as a fallback). Used by both the
//! X11 and Wayland shell backends.

use std::{
    ffi::{c_void, CString},
    rc::Rc,
};

use gl_context_loader::GenericGlContext;

use super::super::x11::dlopen::{Egl, Library};
use crate::desktop::shell2::common::dlopen::DynamicLibrary;
use crate::desktop::shell2::common::gl_loader::load_gl_context;

/// Wrapper to get access to the GL function pointers
pub struct GlFunctions {
    _opengl_lib_handle: Option<Library>,
    pub functions: Rc<GenericGlContext>,
}

impl std::fmt::Debug for GlFunctions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GlFunctions {{ ... }}")
    }
}

impl GlFunctions {
    /// Allocates and loads the OpenGL function pointers via eglGetProcAddress.
    pub fn initialize(egl: &Rc<Egl>) -> Result<Self, String> {
        let opengl_lib = Library::load("libGL.so.1").ok();

        let context = load_gl_context(|s| {
            let symbol_name = CString::new(s).unwrap();
            let addr = unsafe { (egl.eglGetProcAddress)(symbol_name.as_ptr()) };
            if !addr.is_null() {
                return addr as *mut gl_context_loader::c_void;
            }
            if let Some(lib) = &opengl_lib {
                if let Ok(addr) = unsafe { lib.get_symbol::<*mut c_void>(s) } {
                    return addr as *mut gl_context_loader::c_void;
                }
            }
            std::ptr::null_mut()
        });

        Ok(Self {
            _opengl_lib_handle: opengl_lib,
            functions: Rc::new(context),
        })
    }
}
