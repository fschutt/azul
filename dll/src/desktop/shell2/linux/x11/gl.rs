//! EGL context management for X11 windows.

use std::rc::Rc;

use super::{
    defines::*,
    dlopen::{Egl, Xlib},
};
use crate::desktop::shell2::common::{
    debug_server::LogCategory, WindowError,
};
use crate::{log_debug, log_warn};

/// Detected EGL partial-present capabilities for a display (shared by the
/// X11 and Wayland GL contexts — both load EGL through [`Egl`]).
///
/// - `buffer_age_supported` (EGL_EXT_buffer_age): the back buffer's age can
///   be queried, so WebRender can render only the accumulated dirty region
///   (see `wr_translate2::PartialPresentDamage`).
/// - `swap_with_damage` (EGL_KHR/EXT_swap_buffers_with_damage): the swap can
///   carry the damaged region so the compositor only recomposites it.
///
/// Either can be present without the other; absence of both means full
/// render + full swap, exactly as before.
#[derive(Clone, Copy, Default)]
pub struct EglPartialPresent {
    pub buffer_age_supported: bool,
    pub swap_with_damage: Option<eglSwapBuffersWithDamage>,
}

impl EglPartialPresent {
    /// Probe the display's extension string and resolve the swap-with-damage
    /// entry point (KHR preferred, EXT fallback) via eglGetProcAddress.
    pub fn detect(egl: &Egl, egl_display: EGLDisplay) -> Self {
        let ext_ptr = unsafe { (egl.eglQueryString)(egl_display, EGL_EXTENSIONS as i32) };
        if ext_ptr.is_null() {
            return Self::default();
        }
        let exts = unsafe { std::ffi::CStr::from_ptr(ext_ptr) }.to_string_lossy();
        let has_ext = |name: &str| exts.split(' ').any(|e| e == name);

        let buffer_age_supported = has_ext("EGL_EXT_buffer_age");

        let swap_with_damage = if has_ext("EGL_KHR_swap_buffers_with_damage") {
            Some(b"eglSwapBuffersWithDamageKHR\0".as_slice())
        } else if has_ext("EGL_EXT_swap_buffers_with_damage") {
            Some(b"eglSwapBuffersWithDamageEXT\0".as_slice())
        } else {
            None
        }
        .and_then(|sym| {
            let p = unsafe { (egl.eglGetProcAddress)(sym.as_ptr() as *const _) };
            if p.is_null() {
                None
            } else {
                Some(unsafe {
                    std::mem::transmute::<*const core::ffi::c_void, eglSwapBuffersWithDamage>(p)
                })
            }
        });

        log_debug!(
            LogCategory::Platform,
            "[EGL] partial present: buffer_age={} swap_with_damage={}",
            buffer_age_supported,
            swap_with_damage.is_some()
        );

        Self {
            buffer_age_supported,
            swap_with_damage,
        }
    }
}

/// Holds the EGL display, context, and surface for an X11 window.
pub struct GlContext {
    pub egl: Rc<Egl>,
    pub egl_display: EGLDisplay,
    pub egl_context: EGLContext,
    pub egl_surface: EGLSurface,
    /// EGL_EXT_buffer_age / swap-with-damage capabilities of this display.
    pub partial_present: EglPartialPresent,
    /// Cell through which WebRender reports the buffer-age-widened total
    /// damage region of each rendered frame (see `default_renderer_options`).
    pub wr_damage: crate::desktop::wr_translate2::PartialPresentDamage,
}

impl GlContext {
    /// Creates a new EGL context for the given X11 display and window.
    pub fn new(
        _xlib: &Rc<Xlib>,
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
            log_debug!(
                LogCategory::Platform,
                "[EGL] OpenGL 3.2 Core failed, trying 3.0..."
            );

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
            log_debug!(
                LogCategory::Platform,
                "[EGL] OpenGL 3.0 failed, trying default..."
            );

            // Try default context (no version specified)
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

        let partial_present = EglPartialPresent::detect(egl, egl_display);

        Ok(Self {
            egl: egl.clone(),
            egl_display,
            egl_context,
            egl_surface,
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

        if unsafe { (self.egl.eglSwapInterval)(self.egl_display, interval) } == 0 {
            let err = unsafe { (self.egl.eglGetError)() };
            log_warn!(
                LogCategory::Platform,
                "[EGL] eglSwapInterval failed, error=0x{:x}",
                err
            );
        }
    }

    /// Makes the OpenGL context current on the calling thread.
    pub fn make_current(&self) {
        if unsafe {
            (self.egl.eglMakeCurrent)(
                self.egl_display,
                self.egl_surface,
                self.egl_surface,
                self.egl_context,
            )
        } == 0
        {
            let err = unsafe { (self.egl.eglGetError)() };
            log_warn!(
                LogCategory::Platform,
                "[EGL] eglMakeCurrent failed, error=0x{:x}",
                err
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

    /// Age of the current back buffer in frames (EGL_EXT_buffer_age).
    ///
    /// Returns 0 (= "content undefined, treat as fully invalid") when the
    /// extension is unsupported or the query fails — the conservative value:
    /// WebRender then renders and presents the full frame.
    pub fn buffer_age(&self) -> usize {
        if !self.partial_present.buffer_age_supported {
            return 0;
        }
        let mut age: i32 = 0;
        let ok = unsafe {
            (self.egl.eglQuerySurface)(
                self.egl_display,
                self.egl_surface,
                EGL_BUFFER_AGE_EXT as i32,
                &mut age,
            )
        };
        if ok == 0 || age < 0 {
            0
        } else {
            age as usize
        }
    }

    /// Swap, passing the damaged region (physical px, TOP-LEFT origin rects)
    /// to eglSwapBuffersWithDamage[KHR|EXT] so the compositor only updates
    /// that region. Rects are y-flipped here (EGL damage rects use a
    /// bottom-left origin). Falls back to a plain full swap when the
    /// extension is unavailable or `rects` is empty (empty damage ⇒ per spec
    /// "full surface damaged" anyway).
    pub fn swap_buffers_with_damage(
        &self,
        rects: &[(u32, u32, u32, u32)],
        buf_height: u32,
    ) -> Result<(), WindowError> {
        let swap_fn = match self.partial_present.swap_with_damage {
            Some(f) if !rects.is_empty() => f,
            _ => return self.swap_buffers(),
        };
        let mut egl_rects: Vec<i32> = Vec::with_capacity(rects.len() * 4);
        for &(x, y, w, h) in rects {
            // top-left (x, y, w, h) → bottom-left origin
            let flipped_y = buf_height.saturating_sub(y.saturating_add(h));
            egl_rects.extend_from_slice(&[x as i32, flipped_y as i32, w as i32, h as i32]);
        }
        let ok = unsafe {
            swap_fn(
                self.egl_display,
                self.egl_surface,
                egl_rects.as_ptr(),
                rects.len() as i32,
            )
        };
        if ok == 0 {
            Err(WindowError::PlatformError(
                "eglSwapBuffersWithDamage failed".into(),
            ))
        } else {
            Ok(())
        }
    }
}

impl Drop for GlContext {
    fn drop(&mut self) {
        unsafe {
            (self.egl.eglMakeCurrent)(
                self.egl_display,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            (self.egl.eglDestroySurface)(self.egl_display, self.egl_surface);
            (self.egl.eglDestroyContext)(self.egl_display, self.egl_context);
        }
    }
}
