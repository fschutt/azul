//! EGL context management for X11 and OpenGL function loading.

use std::{
    ffi::{c_void, CString},
    mem,
    rc::Rc,
};

use gl_context_loader::GenericGlContext;

use super::{
    defines::*,
    dlopen::{Egl, Library, Xlib},
};
use crate::desktop::shell2::common::{
    debug_server::LogCategory,
    dlopen::DynamicLibrary, 
    WindowError,
};
use crate::{log_debug, log_warn};

/// Holds the EGL display, context, and surface for an X11 window.
pub struct GlContext {
    pub egl: Rc<Egl>,
    pub egl_display: EGLDisplay,
    pub egl_context: EGLContext,
    pub egl_surface: EGLSurface,
}

impl GlContext {
    /// Creates a new EGL context for the given X11 display and window.
    pub fn new(
        xlib: &Rc<Xlib>,
        egl: &Rc<Egl>,
        display: *mut Display,
        window: Window,
    ) -> Result<Self, WindowError> {
        let egl_display = unsafe { (egl.eglGetDisplay)(display as EGLNativeDisplayType) };
        if egl_display.is_null() {
            return Err(WindowError::PlatformError("eglGetDisplay failed".into()));
        }

        let mut major = 0;
        let mut minor = 0;
        if unsafe { (egl.eglInitialize)(egl_display, &mut major, &mut minor) } == 0 {
            return Err(WindowError::PlatformError("eglInitialize failed".into()));
        }
        log_debug!(LogCategory::Platform, "[EGL] Initialized EGL {}.{}", major, minor);

        if unsafe { (egl.eglBindAPI)(EGL_OPENGL_API) } == 0 {
            return Err(WindowError::ContextCreationFailed);
        }

        let config_attribs = [
            EGL_RED_SIZE as i32,
            8,
            EGL_GREEN_SIZE as i32,
            8,
            EGL_BLUE_SIZE as i32,
            8,
            EGL_ALPHA_SIZE as i32,
            8,
            EGL_DEPTH_SIZE as i32,
            24,
            EGL_STENCIL_SIZE as i32,
            8,
            EGL_SURFACE_TYPE as i32,
            EGL_WINDOW_BIT as i32,
            EGL_RENDERABLE_TYPE as i32,
            EGL_OPENGL_BIT as i32,
            EGL_NONE as i32,
        ];

        let mut config = std::ptr::null_mut();
        let mut num_config = 0;
        let choose_result = unsafe {
            (egl.eglChooseConfig)(
                egl_display,
                config_attribs.as_ptr(),
                &mut config,
                1,
                &mut num_config,
            )
        };
        if choose_result == 0 || num_config == 0 {
            return Err(WindowError::ContextCreationFailed);
        }

        let egl_surface = unsafe {
            (egl.eglCreateWindowSurface)(
                egl_display,
                config,
                window as EGLNativeWindowType,
                std::ptr::null(),
            )
        };
        if egl_surface.is_null() {
            return Err(WindowError::PlatformError(
                "eglCreateWindowSurface failed".into(),
            ));
        }

        // Try OpenGL 3.2 Core first, then fall back to simpler contexts
        let context_attribs_32_core = [
            EGL_CONTEXT_MAJOR_VERSION as i32,
            3,
            EGL_CONTEXT_MINOR_VERSION as i32,
            2,
            EGL_CONTEXT_OPENGL_PROFILE_MASK as i32,
            EGL_CONTEXT_OPENGL_CORE_PROFILE_BIT as i32,
            EGL_NONE as i32,
        ];
        
        let mut egl_context = unsafe {
            (egl.eglCreateContext)(
                egl_display,
                config,
                std::ptr::null_mut(),
                context_attribs_32_core.as_ptr(),
            )
        };
        
        if egl_context.is_null() {
            log_debug!(LogCategory::Platform, "[EGL] OpenGL 3.2 Core failed, trying 3.0...");
            
            // Try OpenGL 3.0 without profile (compatibility)
            let context_attribs_30 = [
                EGL_CONTEXT_MAJOR_VERSION as i32,
                3,
                EGL_CONTEXT_MINOR_VERSION as i32,
                0,
                EGL_NONE as i32,
            ];
            
            egl_context = unsafe {
                (egl.eglCreateContext)(
                    egl_display,
                    config,
                    std::ptr::null_mut(),
                    context_attribs_30.as_ptr(),
                )
            };
        }
        
        if egl_context.is_null() {
            log_debug!(LogCategory::Platform, "[EGL] OpenGL 3.0 failed, trying default...");
            
            // Try default context (no version specified)
            let context_attribs_default = [
                EGL_NONE as i32,
            ];
            
            egl_context = unsafe {
                (egl.eglCreateContext)(
                    egl_display,
                    config,
                    std::ptr::null_mut(),
                    context_attribs_default.as_ptr(),
                )
            };
        }
        
        if egl_context.is_null() {
            let egl_error = unsafe { (egl.eglGetError)() };
            log_warn!(LogCategory::Platform, "[EGL] All context creation attempts failed, last error=0x{:x}", egl_error);
            return Err(WindowError::ContextCreationFailed);
        }
        log_debug!(LogCategory::Platform, "[EGL] OpenGL context created successfully");

        Ok(Self {
            egl: egl.clone(),
            egl_display,
            egl_context,
            egl_surface,
        })
    }

    /// Configure VSync using eglSwapInterval
    pub fn configure_vsync(&self, vsync: azul_core::window::Vsync) {
        use azul_core::window::Vsync;

        let interval = match vsync {
            Vsync::Enabled => 1,
            Vsync::Disabled => 0,
            Vsync::DontCare => 1,
        };

        unsafe {
            (self.egl.eglSwapInterval)(self.egl_display, interval);
        }
    }

    /// Makes the OpenGL context current on the calling thread.
    pub fn make_current(&self) {
        unsafe {
            (self.egl.eglMakeCurrent)(
                self.egl_display,
                self.egl_surface,
                self.egl_surface,
                self.egl_context,
            );
        }
    }

    /// Swaps the front and back buffers.
    pub fn swap_buffers(&self) -> Result<(), WindowError> {
        if unsafe { (self.egl.eglSwapBuffers)(self.egl_display, self.egl_surface) } == 0 {
            Err(WindowError::PlatformError("eglSwapBuffers failed".into()))
        } else {
            Ok(())
        }
    }
}
