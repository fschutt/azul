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
    /// EGL_EXT_buffer_age / swap-with-damage capabilities (shared detection
    /// with the X11 EGL backend).
    pub partial_present: crate::desktop::shell2::linux::x11::gl::EglPartialPresent,
    /// Cell through which WebRender reports the buffer-age-widened total
    /// damage region of each rendered frame (see `default_renderer_options`).
    pub wr_damage: crate::desktop::wr_translate2::PartialPresentDamage,
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
            partial_present: Default::default(),
            wr_damage: Default::default(),
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

        // On Wayland, plain eglGetDisplay(wl_display) is ambiguous when both
        // DISPLAY and WAYLAND_DISPLAY are set (Mesa can't infer the platform),
        // and eglInitialize then fails. Request the Wayland platform explicitly
        // via eglGetPlatformDisplay[EXT] (resolved through eglGetProcAddress, so
        // no extra dlsym), falling back to legacy eglGetDisplay.
        const EGL_PLATFORM_WAYLAND_KHR: u32 = 0x31D8;
        type GetPlatformDisplay =
            unsafe extern "C" fn(u32, *mut core::ffi::c_void, *const isize) -> EGLDisplay;
        let egl_display = unsafe {
            let mut gpd = (egl.eglGetProcAddress)(b"eglGetPlatformDisplay\0".as_ptr() as *const _);
            if gpd.is_null() {
                gpd = (egl.eglGetProcAddress)(b"eglGetPlatformDisplayEXT\0".as_ptr() as *const _);
            }
            let mut d: EGLDisplay = std::ptr::null_mut();
            if !gpd.is_null() {
                let f: GetPlatformDisplay = std::mem::transmute(gpd);
                d = f(
                    EGL_PLATFORM_WAYLAND_KHR,
                    display as *mut core::ffi::c_void,
                    std::ptr::null(),
                );
                if !d.is_null() {
                    log_debug!(LogCategory::Platform, "[EGL] Using Wayland platform display");
                }
            }
            if d.is_null() {
                (egl.eglGetDisplay)(display as EGLNativeDisplayType)
            } else {
                d
            }
        };
        if egl_display.is_null() {
            return Err(WindowError::PlatformError("eglGetDisplay/PlatformDisplay failed".into()));
        }

        let mut major = 0;
        let mut minor = 0;
        if unsafe { (egl.eglInitialize)(egl_display, &mut major, &mut minor) } == 0 {
            let ec = unsafe { (egl.eglGetError)() };
            return Err(WindowError::PlatformError(format!(
                "eglInitialize failed (EGL error 0x{:04x})",
                ec
            )));
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

        let partial_present =
            crate::desktop::shell2::linux::x11::gl::EglPartialPresent::detect(&egl, egl_display);

        Ok(Self {
            egl: Some(egl),
            egl_display: Some(egl_display),
            egl_context: Some(egl_context),
            egl_surface: Some(egl_surface),
            wl_egl_window: Some(egl_window),
            wayland: Some(wayland.clone()),
            partial_present,
            wr_damage: Default::default(),
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

    /// Age of the current back buffer in frames (EGL_EXT_buffer_age).
    /// 0 = unsupported / query failed / undefined content ⇒ full render.
    pub fn buffer_age(&self) -> usize {
        if !self.partial_present.buffer_age_supported {
            return 0;
        }
        let (egl, display, surface) = match (self.egl.as_ref(), self.egl_display, self.egl_surface)
        {
            (Some(e), Some(d), Some(s)) => (e, d, s),
            _ => return 0,
        };
        let mut age: i32 = 0;
        let ok = unsafe {
            (egl.eglQuerySurface)(display, surface, EGL_BUFFER_AGE_EXT as i32, &mut age)
        };
        if ok == 0 || age < 0 {
            0
        } else {
            age as usize
        }
    }

    /// Swap, passing the damaged region (physical px, TOP-LEFT origin) to
    /// eglSwapBuffersWithDamage[KHR|EXT]. Rects are y-flipped here (EGL
    /// damage rects use a bottom-left origin). Falls back to a plain full
    /// swap when the extension is unavailable or `rects` is empty.
    ///
    /// On Wayland this also posts the surface damage to the compositor, so
    /// callers using it must NOT additionally call wl_surface_damage.
    pub fn swap_buffers_with_damage(
        &self,
        rects: &[(u32, u32, u32, u32)],
        buf_height: u32,
    ) -> Result<(), WindowError> {
        let swap_fn = match self.partial_present.swap_with_damage {
            Some(f) if !rects.is_empty() => f,
            _ => return self.swap_buffers(),
        };
        let (display, surface) = match (self.egl_display, self.egl_surface) {
            (Some(d), Some(s)) => (d, s),
            _ => return Ok(()),
        };
        let mut egl_rects: Vec<i32> = Vec::with_capacity(rects.len() * 4);
        for &(x, y, w, h) in rects {
            // top-left (x, y, w, h) → bottom-left origin
            let flipped_y = buf_height.saturating_sub(y.saturating_add(h));
            egl_rects.extend_from_slice(&[x as i32, flipped_y as i32, w as i32, h as i32]);
        }
        let ok = unsafe { swap_fn(display, surface, egl_rects.as_ptr(), rects.len() as i32) };
        if ok == 0 {
            Err(WindowError::PlatformError(
                "eglSwapBuffersWithDamage failed".into(),
            ))
        } else {
            Ok(())
        }
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
