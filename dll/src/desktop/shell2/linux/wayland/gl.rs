//! EGL context management for Wayland.
//!
//! Provides [`GlContext`] which wraps EGL display, context, and surface
//! for OpenGL rendering on Wayland. EGL function pointers are loaded
//! via [`Egl`](crate::desktop::shell2::linux::x11::dlopen::Egl).

use std::rc::Rc;

use super::{
    defines::*,
    dlopen::{Library, Wayland},
};
use crate::desktop::shell2::{
    common::{debug_server::LogCategory, dlopen::DynamicLibrary, WindowError},
    linux::x11::dlopen::Egl,
};
use crate::{log_debug, log_warn};

/// EGL-based OpenGL context for a Wayland surface.
pub struct GlContext {
    pub egl: Option<Rc<Egl>>,
    pub egl_display: Option<EGLDisplay>,
    pub egl_context: Option<EGLContext>,
    pub egl_surface: Option<EGLSurface>,
    wl_egl_window: Option<*mut wl_egl_window>,
    wayland: Option<Rc<Wayland>>,
}

impl Default for GlContext {
    fn default() -> Self {
        Self {
            egl: None,
            egl_display: None,
            egl_context: None,
            egl_surface: None,
            wl_egl_window: None,
            wayland: None,
        }
    }
}

impl GlContext {
    /// Creates a new EGL context for the given Wayland display and surface.
    pub fn new(
        wayland: &Rc<Wayland>,
        display: *mut wl_display,
        surface: *mut wl_surface,
        width: i32,
        height: i32,
    ) -> Result<Self, WindowError> {
        let egl = Egl::new()
            .map_err(|e| WindowError::PlatformError(format!("Failed to load libEGL: {:?}", e)))?;

        let egl_display = unsafe { (egl.eglGetDisplay)(display as EGLNativeDisplayType) };
        if egl_display.is_null() {
            return Err(WindowError::PlatformError("eglGetDisplay failed".into()));
        }

        let mut major = 0;
        let mut minor = 0;
        if unsafe { (egl.eglInitialize)(egl_display, &mut major, &mut minor) } == 0 {
            return Err(WindowError::PlatformError("eglInitialize failed".into()));
        }
        log_debug!(
            LogCategory::Platform,
            "[EGL] Initialized EGL {}.{}",
            major,
            minor
        );

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
        if unsafe {
            (egl.eglChooseConfig)(
                egl_display,
                config_attribs.as_ptr(),
                &mut config,
                1,
                &mut num_config,
            )
        } == 0
            || num_config == 0
        {
            return Err(WindowError::ContextCreationFailed);
        }

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
            log_debug!(
                LogCategory::Platform,
                "[EGL] OpenGL 3.2 Core failed, trying 3.0..."
            );

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
            log_debug!(
                LogCategory::Platform,
                "[EGL] OpenGL 3.0 failed, trying default..."
            );

            let context_attribs_default = [EGL_NONE as i32];

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
            log_warn!(
                LogCategory::Platform,
                "[EGL] All context creation attempts failed, last error=0x{:x}",
                egl_error
            );
            return Err(WindowError::ContextCreationFailed);
        }
        log_debug!(
            LogCategory::Platform,
            "[EGL] OpenGL context created successfully"
        );

        let egl_window = unsafe { (wayland.wl_egl_window_create)(surface, width, height) };
        if egl_window.is_null() {
            return Err(WindowError::PlatformError(
                "wl_egl_window_create failed".into(),
            ));
        }

        let egl_surface = unsafe {
            (egl.eglCreateWindowSurface)(egl_display, config, egl_window as _, std::ptr::null())
        };
        if egl_surface.is_null() {
            unsafe { (wayland.wl_egl_window_destroy)(egl_window) };
            return Err(WindowError::PlatformError(
                "eglCreateWindowSurface failed".into(),
            ));
        }

        Ok(Self {
            egl: Some(egl),
            egl_display: Some(egl_display),
            egl_context: Some(egl_context),
            egl_surface: Some(egl_surface),
            wl_egl_window: Some(egl_window),
            wayland: Some(wayland.clone()),
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

        if let (Some(egl), Some(display)) = (self.egl.as_ref(), self.egl_display) {
            unsafe {
                (egl.eglSwapInterval)(display, interval);
            }
        }
    }

    /// Makes this EGL context current for the calling thread.
    pub fn make_current(&self) {
        if let (Some(egl), Some(display), Some(surface), Some(context)) = (
            self.egl.as_ref(),
            self.egl_display,
            self.egl_surface,
            self.egl_context,
        ) {
            unsafe { (egl.eglMakeCurrent)(display, surface, surface, context) };
        }
    }

    /// Swaps the front and back buffers for the EGL surface.
    pub fn swap_buffers(&self) -> Result<(), WindowError> {
        if let (Some(egl), Some(display), Some(surface)) =
            (self.egl.as_ref(), self.egl_display, self.egl_surface)
        {
            if unsafe { (egl.eglSwapBuffers)(display, surface) } == 0 {
                return Err(WindowError::PlatformError("eglSwapBuffers failed".into()));
            }
        }
        Ok(())
    }

    /// Resizes the underlying `wl_egl_window` to the given dimensions.
    pub fn resize(&self, wayland: &Rc<Wayland>, width: i32, height: i32) {
        if let Some(wl_egl_window) = self.wl_egl_window {
            unsafe { (wayland.wl_egl_window_resize)(wl_egl_window, width, height, 0, 0) };
        }
    }
}

impl Drop for GlContext {
    fn drop(&mut self) {
        if let (Some(egl), Some(display), Some(surface), Some(context)) = (
            self.egl.take(),
            self.egl_display.take(),
            self.egl_surface.take(),
            self.egl_context.take(),
        ) {
            unsafe {
                (egl.eglMakeCurrent)(
                    display,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                );
                (egl.eglDestroySurface)(display, surface);
                (egl.eglDestroyContext)(display, context);
                (egl.eglTerminate)(display);
            }
        }

        if let (Some(wayland), Some(wl_egl_window)) =
            (self.wayland.take(), self.wl_egl_window.take())
        {
            unsafe { (wayland.wl_egl_window_destroy)(wl_egl_window) };
        }
    }
}
