//! EGL for OpenGL ES, loaded at run time: the context, the window surface and the swap of the phone shells' GPU path,
//! and the Linux host test of it (Mesa on a runner without a window system).
//!
//! `libEGL.so` is opened with `dlopen` (`libloading`), not linked: a device or a runner without it is a diagnosed
//! fallback to the CPU renderer, not a process that fails to start. Nothing here is global: the library, the display, the
//! context and the surfaces are owned by an [`EglSession`], which the window keeps, and every call is made by the thread
//! that owns the session (an EGL context is current on one thread at a time; on Android that is the `android_main`
//! thread, never the Java UI thread).
//!
//! The session NEVER calls `eglTerminate`: the display is the process's, and Android's WebView (a `<webview>` of the app)
//! uses the same one.
//!
//! The pure tables (constants, extension caps, config requests, damage rectangles, the meaning of a swap error) are in
//! `egl_pure`, which a host tests without EGL.

use std::{
    cell::RefCell,
    ffi::{c_char, c_void, CStr, CString},
    fmt,
    rc::Rc,
};

use gl_context_loader::GenericGlContext;

use super::{
    egl_pure::{
        classify_swap_error, context_attribs, damage_rects_for_swap, usable_buffer_age, EglCaps,
        EglInt, SwapError, CONFIG_WISHES, EGL_BUFFER_AGE_EXT, EGL_EXTENSIONS, EGL_HEIGHT,
        EGL_NATIVE_VISUAL_ID, EGL_NONE, EGL_OPENGL_ES_API, EGL_PBUFFER_BIT,
        EGL_PLATFORM_SURFACELESS_MESA, EGL_VENDOR, EGL_VERSION, EGL_WIDTH,
    },
    gl_loader::load_gl_context,
    gles_probe::REQUIRED_FUNCTIONS,
};

/// An `EGLDisplay`.
pub type EglDisplay = *mut c_void;
/// An `EGLConfig`.
pub type EglConfig = *mut c_void;
/// An `EGLContext`.
pub type EglContext = *mut c_void;
/// An `EGLSurface`.
pub type EglSurface = *mut c_void;

type FnGetDisplay = unsafe extern "C" fn(*mut c_void) -> EglDisplay;
type FnGetPlatformDisplay = unsafe extern "C" fn(u32, *mut c_void, *const EglInt) -> EglDisplay;
type FnInitialize = unsafe extern "C" fn(EglDisplay, *mut EglInt, *mut EglInt) -> u32;
type FnBindApi = unsafe extern "C" fn(u32) -> u32;
type FnChooseConfig =
    unsafe extern "C" fn(EglDisplay, *const EglInt, *mut EglConfig, EglInt, *mut EglInt) -> u32;
type FnGetConfigAttrib = unsafe extern "C" fn(EglDisplay, EglConfig, EglInt, *mut EglInt) -> u32;
type FnCreateContext =
    unsafe extern "C" fn(EglDisplay, EglConfig, EglContext, *const EglInt) -> EglContext;
type FnCreateWindowSurface =
    unsafe extern "C" fn(EglDisplay, EglConfig, *mut c_void, *const EglInt) -> EglSurface;
type FnCreatePbufferSurface =
    unsafe extern "C" fn(EglDisplay, EglConfig, *const EglInt) -> EglSurface;
type FnMakeCurrent = unsafe extern "C" fn(EglDisplay, EglSurface, EglSurface, EglContext) -> u32;
type FnSwapBuffers = unsafe extern "C" fn(EglDisplay, EglSurface) -> u32;
type FnSwapBuffersWithDamage =
    unsafe extern "C" fn(EglDisplay, EglSurface, *const EglInt, EglInt) -> u32;
type FnSwapInterval = unsafe extern "C" fn(EglDisplay, EglInt) -> u32;
type FnDestroySurface = unsafe extern "C" fn(EglDisplay, EglSurface) -> u32;
type FnDestroyContext = unsafe extern "C" fn(EglDisplay, EglContext) -> u32;
type FnQuerySurface = unsafe extern "C" fn(EglDisplay, EglSurface, EglInt, *mut EglInt) -> u32;
type FnQueryString = unsafe extern "C" fn(EglDisplay, EglInt) -> *const c_char;
type FnGetError = unsafe extern "C" fn() -> EglInt;
type FnGetProcAddress = unsafe extern "C" fn(*const c_char) -> *mut c_void;

/// Why a step of the EGL path failed: which stage of the GPU path it belongs to (the names the diagnostic line and
/// the `AZ_GPU_FAIL` hook use), which call, and what EGL said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EglFailure {
    /// `library`, `context` (display, initialize, config, context), `surface` or `make_current`.
    pub stage: &'static str,
    /// The EGL function (or `dlopen`) that failed.
    pub call: &'static str,
    /// `eglGetError` after it, 0 when none was asked.
    pub code: EglInt,
    /// More: the loader's message, what was asked.
    pub detail: String,
}

impl EglFailure {
    fn new(stage: &'static str, call: &'static str, code: EglInt, detail: impl Into<String>) -> Self {
        Self {
            stage,
            call,
            code,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for EglFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} failed", self.call)?;
        if self.code != 0 {
            write!(f, " ({} 0x{:04x})", super::egl_pure::egl_error_name(self.code), self.code)?;
        }
        if !self.detail.is_empty() {
            write!(f, ": {}", self.detail)?;
        }
        Ok(())
    }
}

/// The EGL library: the functions the GPU path calls, resolved once.
pub struct EglApi {
    get_display: FnGetDisplay,
    get_platform_display: Option<FnGetPlatformDisplay>,
    initialize: FnInitialize,
    bind_api: FnBindApi,
    choose_config: FnChooseConfig,
    get_config_attrib: FnGetConfigAttrib,
    create_context: FnCreateContext,
    create_window_surface: FnCreateWindowSurface,
    create_pbuffer_surface: FnCreatePbufferSurface,
    make_current: FnMakeCurrent,
    swap_buffers: FnSwapBuffers,
    swap_interval: FnSwapInterval,
    destroy_surface: FnDestroySurface,
    destroy_context: FnDestroyContext,
    query_surface: FnQuerySurface,
    query_string: FnQueryString,
    get_error: FnGetError,
    get_proc_address: FnGetProcAddress,
    /// Last: the functions above point into it.
    _library: libloading::Library,
}

impl fmt::Debug for EglApi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EglApi")
    }
}

impl EglApi {
    /// Opens the first of `candidates` that loads (`libEGL.so` on Android, `libEGL.so.1` on Linux) and resolves the
    /// functions the path needs.
    ///
    /// # Errors
    ///
    /// No candidate loads, or one that does lacks a function (stage `library`).
    pub fn load(candidates: &[&str]) -> Result<Rc<Self>, EglFailure> {
        let mut tried = Vec::new();
        for name in candidates {
            // SAFETY: opening a system library runs its initialisers, as every dlopen does.
            match unsafe { libloading::Library::new(name) } {
                Ok(library) => return Self::from_library(library).map(Rc::new),
                Err(e) => tried.push(format!("{name}: {e}")),
            }
        }
        Err(EglFailure::new("library", "dlopen", 0, tried.join("; ")))
    }

    fn from_library(library: libloading::Library) -> Result<Self, EglFailure> {
        // One symbol, as the function type the field has. SAFETY: the types above are the signatures of the EGL
        // specification; a library that exports the name with another signature is not an EGL.
        macro_rules! symbol {
            ($ty:ty, $name:literal) => {
                // SAFETY: see above.
                match unsafe { library.get::<$ty>(concat!($name, "\0").as_bytes()) } {
                    Ok(symbol) => *symbol,
                    Err(e) => return Err(EglFailure::new("library", $name, 0, e.to_string())),
                }
            };
        }
        // SAFETY: as above; an absent optional function is `None`.
        let get_platform_display = unsafe {
            library
                .get::<FnGetPlatformDisplay>(b"eglGetPlatformDisplayEXT\0")
                .ok()
                .map(|s| *s)
        };
        // Each symbol in a statement of its own: the borrow of the library ends before the library moves into the struct.
        let get_display = symbol!(FnGetDisplay, "eglGetDisplay");
        let initialize = symbol!(FnInitialize, "eglInitialize");
        let bind_api = symbol!(FnBindApi, "eglBindAPI");
        let choose_config = symbol!(FnChooseConfig, "eglChooseConfig");
        let get_config_attrib = symbol!(FnGetConfigAttrib, "eglGetConfigAttrib");
        let create_context = symbol!(FnCreateContext, "eglCreateContext");
        let create_window_surface = symbol!(FnCreateWindowSurface, "eglCreateWindowSurface");
        let create_pbuffer_surface = symbol!(FnCreatePbufferSurface, "eglCreatePbufferSurface");
        let make_current = symbol!(FnMakeCurrent, "eglMakeCurrent");
        let swap_buffers = symbol!(FnSwapBuffers, "eglSwapBuffers");
        let swap_interval = symbol!(FnSwapInterval, "eglSwapInterval");
        let destroy_surface = symbol!(FnDestroySurface, "eglDestroySurface");
        let destroy_context = symbol!(FnDestroyContext, "eglDestroyContext");
        let query_surface = symbol!(FnQuerySurface, "eglQuerySurface");
        let query_string = symbol!(FnQueryString, "eglQueryString");
        let get_error = symbol!(FnGetError, "eglGetError");
        let get_proc_address = symbol!(FnGetProcAddress, "eglGetProcAddress");
        Ok(Self {
            get_display,
            get_platform_display,
            initialize,
            bind_api,
            choose_config,
            get_config_attrib,
            create_context,
            create_window_surface,
            create_pbuffer_surface,
            make_current,
            swap_buffers,
            swap_interval,
            destroy_surface,
            destroy_context,
            query_surface,
            query_string,
            get_error,
            get_proc_address,
            _library: library,
        })
    }

    /// `eglGetError`.
    fn error(&self) -> EglInt {
        // SAFETY: no arguments; reads the calling thread's error.
        unsafe { (self.get_error)() }
    }

    /// A string of `eglQueryString`, empty when EGL answers none.
    fn string(&self, display: EglDisplay, name: EglInt) -> String {
        // SAFETY: EGL answers a NUL-terminated string it owns, or null.
        let p = unsafe { (self.query_string)(display, name) };
        if p.is_null() {
            String::new()
        } else {
            // SAFETY: non-null and NUL-terminated, valid until the next call; copied at once.
            unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
        }
    }

    /// The address of a GL or EGL function by name (`eglGetProcAddress`), null when the driver does not have it.
    #[must_use]
    pub fn proc_address(&self, name: &CStr) -> *mut c_void {
        // SAFETY: a NUL-terminated name.
        unsafe { (self.get_proc_address)(name.as_ptr()) }
    }
}

/// Which display the session takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayKind {
    /// `eglGetDisplay(EGL_DEFAULT_DISPLAY)`: the platform's own (Android).
    Default,
    /// Mesa's surfaceless platform (`EGL_MESA_platform_surfaceless`): no window system at all, the host test on a runner.
    /// Falls back to the default display when the platform extension is not there.
    SurfacelessMesa,
}

/// What the session learned about the EGL it opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EglInfo {
    /// `eglInitialize`'s version.
    pub version: (EglInt, EglInt),
    /// `EGL_VENDOR`.
    pub vendor: String,
    /// `EGL_VERSION` as a string.
    pub version_string: String,
    /// `EGL_NATIVE_VISUAL_ID` of the chosen config: the pixel format a window must be given before its surface.
    pub native_visual_id: EglInt,
    /// The depth and stencil sizes of the config that was asked for and found.
    pub depth_stencil: (EglInt, EglInt),
    /// What the display's extensions allow.
    pub caps: EglCaps,
    /// The config can make a pbuffer surface (a context can be current without the window).
    pub pbuffer_capable: bool,
}

/// What the context is current on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Binding {
    /// The window's surface.
    Window,
    /// No surface (`EGL_KHR_surfaceless_context`).
    Surfaceless,
    /// A 1 x 1 pbuffer.
    Pbuffer,
}

/// A display, a config, an OpenGL ES 3 context and the surfaces it draws to, owned by one window and used by one thread.
pub struct EglSession {
    api: Rc<EglApi>,
    display: EglDisplay,
    config: EglConfig,
    context: EglContext,
    window_surface: EglSurface,
    pbuffer: EglSurface,
    swap_with_damage: Option<FnSwapBuffersWithDamage>,
    info: EglInfo,
}

impl fmt::Debug for EglSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EglSession")
            .field("info", &self.info)
            .field("has_window_surface", &!self.window_surface.is_null())
            .finish()
    }
}

impl EglSession {
    /// Opens the display, binds the OpenGL ES API, finds a config and makes an OpenGL ES 3 context. `surface_types`
    /// are the `EGL_SURFACE_TYPE` masks to try, best first: `EGL_WINDOW_BIT | EGL_PBUFFER_BIT` and then
    /// `EGL_WINDOW_BIT` on a phone, `EGL_PBUFFER_BIT` for the host test.
    ///
    /// # Errors
    ///
    /// The first step that failed, with its stage: the display (`eglGetDisplay`, `eglInitialize`), the API, no config
    /// of any wish, no ES 3 context (all stage `context`).
    pub fn open(
        api: Rc<EglApi>,
        kind: DisplayKind,
        surface_types: &[EglInt],
    ) -> Result<Self, EglFailure> {
        let display = api.open_display(kind);
        if display.is_null() {
            return Err(EglFailure::new("context", "eglGetDisplay", api.error(), "no display"));
        }
        let (mut major, mut minor) = (0, 0);
        // SAFETY: two valid out pointers.
        if unsafe { (api.initialize)(display, &mut major, &mut minor) } == 0 {
            return Err(EglFailure::new("context", "eglInitialize", api.error(), ""));
        }
        // SAFETY: a constant.
        if unsafe { (api.bind_api)(EGL_OPENGL_ES_API) } == 0 {
            return Err(EglFailure::new(
                "context",
                "eglBindAPI",
                api.error(),
                "EGL_OPENGL_ES_API",
            ));
        }
        let extensions = api.string(display, EGL_EXTENSIONS);
        let caps = EglCaps::from_extensions(&extensions);

        let (config, depth_stencil, surface_type) = api.pick_config(display, surface_types)?;
        let mut native_visual_id = 0;
        // SAFETY: a valid config of this display and an out pointer.
        let _ = unsafe {
            (api.get_config_attrib)(display, config, EGL_NATIVE_VISUAL_ID, &mut native_visual_id)
        };

        let attribs = context_attribs(3);
        // SAFETY: the attribute list ends with EGL_NONE; no shared context.
        let context = unsafe {
            (api.create_context)(display, config, std::ptr::null_mut(), attribs.as_ptr())
        };
        if context.is_null() {
            return Err(EglFailure::new(
                "context",
                "eglCreateContext",
                api.error(),
                "an OpenGL ES 3 context",
            ));
        }

        let swap_with_damage = caps.swap_with_damage_symbol().and_then(|name| {
            let name = CString::new(name).ok()?;
            let p = api.proc_address(&name);
            if p.is_null() {
                None
            } else {
                // SAFETY: the symbol of the extension has the signature of `FnSwapBuffersWithDamage`.
                Some(unsafe { std::mem::transmute::<*mut c_void, FnSwapBuffersWithDamage>(p) })
            }
        });

        let info = EglInfo {
            version: (major, minor),
            vendor: api.string(display, EGL_VENDOR),
            version_string: api.string(display, EGL_VERSION),
            native_visual_id,
            depth_stencil,
            caps,
            pbuffer_capable: surface_type & EGL_PBUFFER_BIT != 0,
        };
        Ok(Self {
            api,
            display,
            config,
            context,
            window_surface: std::ptr::null_mut(),
            pbuffer: std::ptr::null_mut(),
            swap_with_damage,
            info,
        })
    }

    /// What the session learned when it opened.
    #[must_use]
    pub const fn info(&self) -> &EglInfo {
        &self.info
    }

    /// The library handle, for resolving GL functions.
    #[must_use]
    pub fn api(&self) -> &Rc<EglApi> {
        &self.api
    }

    /// Whether the window has a surface now.
    #[must_use]
    pub fn has_window_surface(&self) -> bool {
        !self.window_surface.is_null()
    }

    /// Whether the context can run GL with no window (surfaceless, or on a pbuffer): what keeps the renderer's GPU
    /// memory across a lost window surface.
    #[must_use]
    pub const fn can_run_without_window(&self) -> bool {
        self.info.caps.surfaceless_context || self.info.pbuffer_capable
    }

    /// Makes a surface of the native window `window` (an `ANativeWindow*`). The window must already have been given
    /// `info().native_visual_id` as its pixel format. An earlier surface of this session is destroyed first.
    ///
    /// # Errors
    ///
    /// `eglCreateWindowSurface` failed (stage `surface`): `EGL_BAD_NATIVE_WINDOW` for a window that is gone, or one that
    /// another producer (the CPU path's `ANativeWindow_lock`) is connected to.
    pub fn create_window_surface(&mut self, window: *mut c_void) -> Result<(), EglFailure> {
        self.destroy_window_surface();
        // SAFETY: a valid display and config; the list is empty (EGL_NONE only); `window` is the caller's live window.
        let surface = unsafe {
            (self.api.create_window_surface)(
                self.display,
                self.config,
                window,
                [EGL_NONE].as_ptr(),
            )
        };
        if surface.is_null() {
            return Err(EglFailure::new(
                "surface",
                "eglCreateWindowSurface",
                self.api.error(),
                "",
            ));
        }
        self.window_surface = surface;
        Ok(())
    }

    /// Makes a pbuffer of `width` x `height` pixels in the place of the window's surface: what a host with no window
    /// system draws to (the test). It is the "window surface" of every other method (`bind`, `swap`, `surface_size`);
    /// a swap posts nothing. An earlier surface of this session is destroyed first.
    ///
    /// # Errors
    ///
    /// `eglCreatePbufferSurface` failed (stage `surface`): the config cannot make a pbuffer, or the size is too large.
    pub fn create_offscreen_surface(&mut self, width: u32, height: u32) -> Result<(), EglFailure> {
        self.destroy_window_surface();
        let to_int = |v: u32| EglInt::try_from(v).unwrap_or(EglInt::MAX).max(1);
        let attribs = [EGL_WIDTH, to_int(width), EGL_HEIGHT, to_int(height), EGL_NONE];
        // SAFETY: a valid config; the list ends with EGL_NONE.
        let surface = unsafe {
            (self.api.create_pbuffer_surface)(self.display, self.config, attribs.as_ptr())
        };
        if surface.is_null() {
            return Err(EglFailure::new(
                "surface",
                "eglCreatePbufferSurface",
                self.api.error(),
                format!("{width} x {height}"),
            ));
        }
        self.window_surface = surface;
        Ok(())
    }

    /// Destroys the window's surface, in the order that disconnects the window: the context is bound to something else
    /// FIRST (a current surface is only marked for deletion, and the native window stays connected to it), then the
    /// surface goes. Only then may the `ANativeWindow` be released.
    pub fn destroy_window_surface(&mut self) {
        if self.window_surface.is_null() {
            return;
        }
        // Bind away from it; whether that worked or not, the surface is destroyed (a failure leaves nothing current).
        if self.bind_without_window().is_err() {
            self.unbind();
        }
        // SAFETY: the surface is this session's; the display is valid.
        let _ = unsafe { (self.api.destroy_surface)(self.display, self.window_surface) };
        self.window_surface = std::ptr::null_mut();
    }

    /// Makes the context current on the calling thread with the best surface there is: the window's, else none
    /// (surfaceless), else the pbuffer.
    ///
    /// # Errors
    ///
    /// `eglMakeCurrent` failed (stage `make_current`), or there is no window surface and the context cannot run
    /// without one.
    pub fn bind(&mut self) -> Result<Binding, EglFailure> {
        if self.has_window_surface() {
            self.make_current(self.window_surface, Binding::Window)
        } else {
            self.bind_without_window()
        }
    }

    /// Makes the context current with no window surface (surfaceless or the pbuffer).
    ///
    /// # Errors
    ///
    /// The context can do neither, or `eglMakeCurrent` failed.
    pub fn bind_without_window(&mut self) -> Result<Binding, EglFailure> {
        if self.info.caps.surfaceless_context {
            return self.make_current(std::ptr::null_mut(), Binding::Surfaceless);
        }
        if self.info.pbuffer_capable {
            if self.pbuffer.is_null() {
                let attribs = [EGL_WIDTH, 1, EGL_HEIGHT, 1, EGL_NONE];
                // SAFETY: a valid config that has EGL_PBUFFER_BIT; the list ends with EGL_NONE.
                self.pbuffer = unsafe {
                    (self.api.create_pbuffer_surface)(self.display, self.config, attribs.as_ptr())
                };
                if self.pbuffer.is_null() {
                    return Err(EglFailure::new(
                        "make_current",
                        "eglCreatePbufferSurface",
                        self.api.error(),
                        "",
                    ));
                }
            }
            return self.make_current(self.pbuffer, Binding::Pbuffer);
        }
        Err(EglFailure::new(
            "make_current",
            "eglMakeCurrent",
            0,
            "no surfaceless context and no pbuffer: the context needs the window's surface",
        ))
    }

    fn make_current(&self, surface: EglSurface, binding: Binding) -> Result<Binding, EglFailure> {
        // SAFETY: valid display and context; the surface is null or this session's.
        let made =
            unsafe { (self.api.make_current)(self.display, surface, surface, self.context) } != 0;
        if made {
            Ok(binding)
        } else {
            Err(EglFailure::new(
                "make_current",
                "eglMakeCurrent",
                self.api.error(),
                format!("{binding:?}"),
            ))
        }
    }

    /// Releases the context from the calling thread (nothing current).
    pub fn unbind(&self) {
        // SAFETY: EGL_NO_SURFACE / EGL_NO_CONTEXT are null.
        let _ = unsafe {
            (self.api.make_current)(
                self.display,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
    }

    /// `eglSwapInterval`: 1 swaps on the display's vertical blank, which paces the frame loop.
    pub fn set_swap_interval(&self, interval: EglInt) -> bool {
        // SAFETY: valid display; the surface bound is the window's.
        unsafe { (self.api.swap_interval)(self.display, interval) != 0 }
    }

    /// The window surface's size in pixels, as EGL sees it (it follows the native window, after the next swap).
    #[must_use]
    pub fn surface_size(&self) -> Option<(u32, u32)> {
        if self.window_surface.is_null() {
            return None;
        }
        let (mut w, mut h) = (0, 0);
        // SAFETY: the surface is this session's; out pointers.
        let ok = unsafe {
            (self.api.query_surface)(self.display, self.window_surface, EGL_WIDTH, &mut w) != 0
                && (self.api.query_surface)(self.display, self.window_surface, EGL_HEIGHT, &mut h)
                    != 0
        };
        (ok && w > 0 && h > 0).then(|| (w as u32, h as u32))
    }

    /// The age of the back buffer in frames (0 when unknown, which makes WebRender draw everything).
    #[must_use]
    pub fn buffer_age(&self) -> usize {
        if !self.info.caps.buffer_age || self.window_surface.is_null() {
            return 0;
        }
        let mut age = 0;
        // SAFETY: the surface is this session's.
        let ok = unsafe {
            (self.api.query_surface)(self.display, self.window_surface, EGL_BUFFER_AGE_EXT, &mut age)
        } != 0;
        usable_buffer_age(true, ok.then_some(age))
    }

    /// Posts the frame: with the damage (`(x, y, w, h)` device pixels, origin top left) when the display can swap with
    /// damage and there is some, else the whole surface. `surface_height` flips the rectangles to EGL's origin.
    ///
    /// # Errors
    ///
    /// What the swap failed with, sorted by what the shell does about it.
    pub fn swap(
        &self,
        damage: Option<&[(u32, u32, u32, u32)]>,
        surface_height: u32,
    ) -> Result<(), SwapError> {
        if self.window_surface.is_null() {
            return Err(SwapError::SurfaceLost);
        }
        let swapped = match (self.swap_with_damage, damage) {
            (Some(with_damage), Some(rects)) if !rects.is_empty() => {
                let flipped = damage_rects_for_swap(rects, surface_height);
                let count = EglInt::try_from(rects.len()).unwrap_or(EglInt::MAX);
                // SAFETY: `flipped` has `count` rectangles of four ints, valid for the call.
                unsafe { with_damage(self.display, self.window_surface, flipped.as_ptr(), count) }
            }
            // SAFETY: the surface is this session's.
            _ => unsafe { (self.api.swap_buffers)(self.display, self.window_surface) },
        };
        if swapped != 0 {
            Ok(())
        } else {
            Err(classify_swap_error(self.api.error()))
        }
    }
}

impl EglApi {
    fn open_display(&self, kind: DisplayKind) -> EglDisplay {
        if kind == DisplayKind::SurfacelessMesa {
            if let Some(get_platform_display) = self.get_platform_display {
                // SAFETY: the platform constant, no native display, no attributes.
                let display = unsafe {
                    get_platform_display(
                        EGL_PLATFORM_SURFACELESS_MESA,
                        std::ptr::null_mut(),
                        std::ptr::null(),
                    )
                };
                if !display.is_null() {
                    return display;
                }
            }
        }
        // SAFETY: EGL_DEFAULT_DISPLAY is null.
        unsafe { (self.get_display)(std::ptr::null_mut()) }
    }

    /// The first config any (surface type, wish) pair gives, with the depth and stencil sizes asked and the surface
    /// type that matched.
    fn pick_config(
        &self,
        display: EglDisplay,
        surface_types: &[EglInt],
    ) -> Result<(EglConfig, (EglInt, EglInt), EglInt), EglFailure> {
        for &surface_type in surface_types {
            for wish in CONFIG_WISHES {
                let attribs = wish.attribs(surface_type);
                let mut config: EglConfig = std::ptr::null_mut();
                let mut count = 0;
                // SAFETY: the list ends with EGL_NONE; room for one config and its count.
                let chosen = unsafe {
                    (self.choose_config)(display, attribs.as_ptr(), &mut config, 1, &mut count)
                };
                if chosen != 0 && count > 0 && !config.is_null() {
                    return Ok((config, (wish.depth, wish.stencil), surface_type));
                }
            }
        }
        Err(EglFailure::new(
            "context",
            "eglChooseConfig",
            self.error(),
            format!("no RGBA8888 OpenGL ES 3 config for the surface types {surface_types:?}"),
        ))
    }
}

impl Drop for EglSession {
    fn drop(&mut self) {
        // The order that disconnects the window and frees the context; the display is the process's and stays.
        self.unbind();
        // SAFETY: the surfaces and the context are this session's.
        unsafe {
            if !self.window_surface.is_null() {
                let _ = (self.api.destroy_surface)(self.display, self.window_surface);
            }
            if !self.pbuffer.is_null() {
                let _ = (self.api.destroy_surface)(self.display, self.pbuffer);
            }
            let _ = (self.api.destroy_context)(self.display, self.context);
        }
    }
}

/// The GL function table of a context, and the library its functions were resolved from.
pub struct GlesFunctions {
    /// Kept loaded while the table's pointers may be called.
    _library: Option<libloading::Library>,
    /// The table WebRender and the engine's GL code call through.
    pub context: Rc<GenericGlContext>,
}

impl fmt::Debug for GlesFunctions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("GlesFunctions")
    }
}

/// The names that are the extension spelling of a function the table keeps under its core (desktop) name: the same
/// signature, so the table's slot can hold the other name's address. GLES has `glBufferStorageEXT` where GL 4.4 has
/// `glBufferStorage`, and WebRender's persistently mapped upload buffers call the latter.
const EXTENSION_SPELLINGS: [&str; 5] = [
    "glBufferStorageEXT",
    "glCopyImageSubDataEXT",
    "glGetQueryObjectui64vEXT",
    "glQueryCounterEXT",
    "glBindFragDataLocationIndexedEXT",
];

impl GlesFunctions {
    /// Resolves the GL functions of the current context: `eglGetProcAddress` first (extension functions are only
    /// there), then the GL ES library itself (`libGLESv3.so`, `libGLESv2.so`), whose symbols are the core ones.
    /// The context of `session` must be current on this thread.
    ///
    /// # Errors
    ///
    /// The names of the functions of [`REQUIRED_FUNCTIONS`] that no source had (stage `functions`).
    pub fn load(session: &EglSession, gles_libraries: &[&str]) -> Result<Self, String> {
        let library = gles_libraries.iter().find_map(|name| {
            // SAFETY: opening a system library runs its initialisers, as every dlopen does.
            unsafe { libloading::Library::new(name) }.ok()
        });
        let resolve = |name: &str| -> *mut gl_context_loader::c_void {
            let Ok(c_name) = CString::new(name) else {
                return std::ptr::null_mut();
            };
            let from_egl = session.api().proc_address(&c_name);
            if !from_egl.is_null() {
                return from_egl.cast();
            }
            if let Some(library) = &library {
                // SAFETY: the symbol's address is read as a pointer and never called here.
                if let Ok(symbol) =
                    unsafe { library.get::<*mut c_void>(c_name.as_bytes_with_nul()) }
                {
                    return (*symbol).cast();
                }
            }
            std::ptr::null_mut()
        };

        let missing = RefCell::new(Vec::<String>::new());
        let mut table = load_gl_context(|name| {
            let address = resolve(name);
            if address.is_null() && REQUIRED_FUNCTIONS.contains(&name) {
                missing.borrow_mut().push(name.to_owned());
            }
            address
        });
        // The extension spellings, for the slots the core name left empty.
        if table.glBufferStorage.is_null() {
            table.glBufferStorage = resolve(EXTENSION_SPELLINGS[0]);
        }
        if table.glCopyImageSubData.is_null() {
            table.glCopyImageSubData = resolve(EXTENSION_SPELLINGS[1]);
        }
        if table.glGetQueryObjectui64v.is_null() {
            table.glGetQueryObjectui64v = resolve(EXTENSION_SPELLINGS[2]);
        }
        if table.glQueryCounter.is_null() {
            table.glQueryCounter = resolve(EXTENSION_SPELLINGS[3]);
        }
        if table.glBindFragDataLocationIndexed.is_null() {
            table.glBindFragDataLocationIndexed = resolve(EXTENSION_SPELLINGS[4]);
        }

        let missing = missing.into_inner();
        if !missing.is_empty() {
            return Err(format!(
                "{} GL functions did not resolve: {}",
                missing.len(),
                missing.join(", ")
            ));
        }
        Ok(Self {
            _library: library,
            context: Rc::new(table),
        })
    }
}

#[cfg(test)]
#[path = "egl_gles_test.rs"]
mod tests;
