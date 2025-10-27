//! EGL context management for Wayland.

use std::{
    ffi::{c_void, CString},
    mem,
    rc::Rc,
};

use gl_context_loader::GenericGlContext;

use super::{
    defines::*,
    dlopen::{Library, Wayland},
};
use crate::desktop::shell2::{
    common::{dlopen::DynamicLibrary, WindowError},
    linux::x11::dlopen::Egl,
};

#[derive(Default)]
pub struct GlContext {
    pub egl: Option<Rc<Egl>>,
    pub egl_display: Option<EGLDisplay>,
    pub egl_context: Option<EGLContext>,
    pub egl_surface: Option<EGLSurface>,
    wl_egl_window: *mut wl_egl_window,
}

impl GlContext {
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

        let context_attribs = [
            EGL_CONTEXT_MAJOR_VERSION as i32,
            3,
            EGL_CONTEXT_MINOR_VERSION as i32,
            2,
            EGL_CONTEXT_OPENGL_PROFILE_MASK as i32,
            EGL_CONTEXT_OPENGL_CORE_PROFILE_BIT as i32,
            EGL_NONE as i32,
        ];
        let egl_context = unsafe {
            (egl.eglCreateContext)(
                egl_display,
                config,
                std::ptr::null_mut(),
                context_attribs.as_ptr(),
            )
        };
        if egl_context.is_null() {
            return Err(WindowError::ContextCreationFailed);
        }

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
            wl_egl_window: egl_window,
        })
    }

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

    pub fn resize(&self, wayland: &Rc<Wayland>, width: i32, height: i32) {
        unsafe { (wayland.wl_egl_window_resize)(self.wl_egl_window, width, height, 0, 0) };
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
            }
        }
    }
}
