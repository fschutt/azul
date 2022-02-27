use crate::{
    app::{App, LazyFcCache},
    gl::{c_char, c_int},
    wr_translate::{
        rebuild_display_list,
        generate_frame,
        synchronize_gpu_values,
        scroll_all_nodes,
        wr_synchronize_updated_images,
        AsyncHitTester,
    }
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    sync::Arc
};
use azul_core::{
    FastBTreeSet, FastHashMap,
    app_resources::{
        ImageMask, ImageRef, Epoch,
        AppConfig, ImageCache, ResourceUpdate,
        RendererResources, GlTextureCache,
    },
    callbacks::{
        RefAny, UpdateImageType,
        DomNodeId, DocumentId
    },
    gl::OptionGlContextPtr,
    task::{Thread, ThreadId, Timer, TimerId},
    ui_solver::LayoutResult,
    styled_dom::DomId,
    dom::NodeId,
    display_list::RenderCallbacks,
    window::{
        LogicalSize, Menu, MenuCallback, MenuItem,
        MonitorVec, WindowCreateOptions, WindowInternal,
        WindowState, FullWindowState, ScrollResult,
        MouseCursorType, CallCallbacksResult
    },
    window_state::NodesToCheck,
};
use core::{
    fmt,
    convert::TryInto,
    cell::{BorrowError, BorrowMutError, RefCell},
    ffi::c_void,
    mem, ptr,
    sync::atomic::{AtomicUsize, Ordering as AtomicOrdering},
};
use gl_context_loader::GenericGlContext;
use webrender::{
    api::{
        units::{
            DeviceIntPoint as WrDeviceIntPoint,
            DeviceIntRect as WrDeviceIntRect,
            DeviceIntSize as WrDeviceIntSize,
            LayoutSize as WrLayoutSize,
        },
        HitTesterRequest as WrHitTesterRequest,
        ApiHitTester as WrApiHitTester, DocumentId as WrDocumentId,
        RenderNotifier as WrRenderNotifier,
    },
    render_api::RenderApi as WrRenderApi,
    PipelineInfo as WrPipelineInfo, Renderer as WrRenderer, RendererError as WrRendererError,
    RendererOptions as WrRendererOptions, ShaderPrecacheFlags as WrShaderPrecacheFlags,
    Shaders as WrShaders, Transaction as WrTransaction,
};
use std::ffi::{CString, OsStr};
use std::os::raw;
use gl_context_loader::gl;
use x11_dl::xlib::{Xlib, Display};

// TODO: Cache compiled shaders between renderers
const WR_SHADER_CACHE: Option<&Rc<RefCell<WrShaders>>> = None;

extern { // syscalls
    fn dlopen(filename: *const raw::c_char, flags: raw::c_int) -> *mut raw::c_void;
    fn dlsym(handle: *mut raw::c_void, symbol: *const raw::c_char) -> *mut raw::c_void;
    fn dlclose(handle: *mut raw::c_void) -> raw::c_int;
    fn dlerror() -> *mut raw::c_char;
}

#[derive(Debug)]
pub enum LinuxWindowCreateError {
    X(String),
    Egl(String),
    NoGlContext,
    Renderer(WrRendererError),
    BorrowMut(BorrowMutError),
}

#[derive(Debug, Copy, Clone)]
pub enum LinuxOpenGlError {
    OpenGL32DllNotFound(u32),
    FailedToGetDC(u32),
    FailedToCreateHiddenHWND(u32),
    FailedToGetPixelFormat(u32),
    NoMatchingPixelFormat(u32),
    OpenGLNotAvailable(u32),
    FailedToStoreContext(u32),
}

#[derive(Debug)]
pub enum LinuxStartupError {
    NoAppInstance(u32),
    WindowCreationFailed,
    Borrow(BorrowError),
    BorrowMut(BorrowMutError),
    Create(LinuxWindowCreateError),
    Gl(LinuxOpenGlError),
}

impl From<LinuxWindowCreateError> for LinuxStartupError {
    fn from(e: LinuxWindowCreateError) -> LinuxStartupError {
        LinuxStartupError::Create(e)
    }
}

pub fn get_monitors(app: &App) -> MonitorVec {
    MonitorVec::from_const_slice(&[]) // TODO
}

// Minimal typedefs from <EGL/egl.h>

type EGLDisplay = *mut c_void;
type EGLNativeDisplayType = *mut c_void;
type EGLNativeWindowType = *mut c_void;
type EGLint = i32;
type EGLBoolean = u32;
type EGLenum = u32;
type EGLConfig = *mut c_void;
type EGLContext = *mut c_void;
type EGLSurface = *mut c_void;

type eglGetDisplayFuncType = extern "C" fn(EGLNativeDisplayType) -> EGLDisplay;
type eglInitializeFuncType = extern "C" fn(EGLDisplay, *mut EGLint, *mut EGLint) -> EGLBoolean;
type eglBindAPIFuncType = extern "C" fn(EGLenum) -> EGLBoolean;
type eglChooseConfigFuncType = extern "C" fn(EGLDisplay, *const EGLint,*mut EGLConfig, EGLint, *mut EGLint) -> EGLBoolean;
type eglCreateWindowSurfaceFuncType = extern "C" fn(EGLDisplay, EGLConfig, EGLNativeWindowType, *const EGLint) -> EGLSurface;
type eglSwapIntervalFuncType = extern "C" fn(EGLDisplay, EGLint) -> EGLBoolean;
type eglCreateContextFuncType = extern "C" fn(EGLDisplay, EGLConfig, EGLContext, *const EGLint) -> EGLContext;
type eglMakeCurrentFuncType = extern "C" fn(EGLDisplay, EGLSurface, EGLSurface, EGLContext) -> EGLBoolean;
type eglSwapBuffersFuncType = extern "C" fn(EGLDisplay, EGLSurface) -> EGLBoolean;
type eglGetErrorFuncType = extern "C" fn () -> EGLint;

const EGL_NO_DISPLAY: EGLDisplay = 0 as *mut c_void;
const EGL_OPENGL_API: EGLenum = 0x30A2;
const EGL_SURFACE_TYPE: EGLint = 0x3033;
const EGL_WINDOW_BIT: EGLint = 0x0004;
const EGL_CONFORMANT: EGLint = 0x3042;
const EGL_OPENGL_BIT: EGLint = 0x0008;
const EGL_RENDERABLE_TYPE: EGLint = 0x3040;
const EGL_COLOR_BUFFER_TYPE: EGLint = 0x303F;
const EGL_RGB_BUFFER: EGLint = 0x308E;
const EGL_BLUE_SIZE: EGLint = 0x3022;
const EGL_GREEN_SIZE: EGLint = 0x3023;
const EGL_RED_SIZE: EGLint = 0x3024;
const EGL_DEPTH_SIZE: EGLint = 0x3025;
const EGL_STENCIL_SIZE: EGLint = 0x3026;
const EGL_NONE: EGLint = 0x3038;
const EGL_GL_COLORSPACE: EGLint = 0x3087;
const EGL_GL_COLORSPACE_LINEAR: EGLint = 0x308A;
const EGL_RENDER_BUFFER: EGLint = 0x3086;
const EGL_BACK_BUFFER: EGLint = 0x3084;
const EGL_NO_SURFACE: EGLSurface = 0 as *mut c_void;
const EGL_NO_CONTEXT: EGLContext = 0 as *mut c_void;
const EGL_FALSE: EGLBoolean = 0;
const EGL_TRUE: EGLBoolean = 1;

const EGL_CONTEXT_MAJOR_VERSION: EGLint = 0x00003098;
const EGL_CONTEXT_MINOR_VERSION: EGLint = 0x000030fb;
const EGL_CONTEXT_OPENGL_PROFILE_MASK: EGLint = 0x000030fd;
const EGL_CONTEXT_OPENGL_CORE_PROFILE_BIT: EGLint = 0x00000001;

const WM_PROTOCOLS: u64 = 0;

/// Main function that starts when app.run() is invoked
pub fn run(app: App, mut root_window: WindowCreateOptions) -> Result<isize, LinuxStartupError> {

    use self::LinuxStartupError::Create;
    use self::LinuxWindowCreateError::{X, Egl};
    use x11_dl::xlib::{
        self, Xlib, XEvent,
        InputOutput, CopyFromParent,
        False, XSetWindowAttributes,
        StructureNotifyMask,
        CWEventMask, XWindowAttributes
    };

    let App {
        data,
        config,
        mut windows,
        image_cache,
        fc_cache,
    } = app;

    let xlib = Xlib::open()
        .map_err(|e| X(format!("Could not load libX11: {}", e.detail())))?;

    let mut dpy = X11Display::open(&xlib)
        .ok_or(X(format!("X11: XOpenDisplay(0) failed")))?;

    let mut active_windows = BTreeMap::new();

    let app_data_inner = Rc::new(RefCell::new(ApplicationData {
        data,
        config,
        image_cache,
        fc_cache,
    }));

    for options in windows.iter_mut() {
        let window = X11Window::new(
            &mut dpy,
            options,
            SharedApplicationData { inner: app_data_inner.clone() }
        )?;
        window.show(&mut dpy);
        active_windows.insert(window.id, window);
    }

    let window = X11Window::new(
        &mut dpy,
        &mut root_window,
        SharedApplicationData { inner: app_data_inner.clone() }
    )?;
    window.show(&mut dpy);
    active_windows.insert(window.id, window);

    let mut cur_xevent = XEvent { pad: [0;24] };

    loop {

        let mut windows_to_close = Vec::new();

        // process all incoming X11 events
        if unsafe { (xlib.XPending)(dpy.get()) } == 0 {
            /// usleep(10 * 1000);
            continue;
        }

        unsafe { (xlib.XNextEvent)(dpy.get(), &mut cur_xevent) };

        for (window_id, window) in active_windows.iter() {

            let cur_event_type = cur_xevent.get_type();

            match cur_event_type {
                // window shown
                xlib::Expose => {
                    let expose_data = unsafe { cur_xevent.expose };
                    let width = expose_data.width;
                    let height = expose_data.height;

                    window.make_current();
                    window.gl_functions.functions.viewport(0, 0, width, height);
                    window.gl_functions.functions.clear_color(0.392, 0.584, 0.929, 1.0);
                    window.gl_functions.functions.clear(
                        gl::COLOR_BUFFER_BIT |
                        gl::DEPTH_BUFFER_BIT |
                        gl::STENCIL_BUFFER_BIT
                    );

                    let swap_result = (window.egl.eglSwapBuffers)(window.egl_display, window.egl_surface);
                    if swap_result != EGL_TRUE {
                        return Err(Create(Egl(format!("EGL: eglSwapBuffers(): Failed to swap OpenGL buffers: {}", swap_result))));
                    }
                },
                // window resized
                xlib::ResizeRequest => {
                    let resize_request_data = unsafe { cur_xevent.resize_request };
                    let width = resize_request_data.width;
                    let height = resize_request_data.height;

                    window.make_current();
                    window.gl_functions.functions.viewport(0, 0, width, height);
                    window.gl_functions.functions.clear_color(0.392, 0.584, 0.929, 1.0);
                    window.gl_functions.functions.clear(
                        gl::COLOR_BUFFER_BIT |
                        gl::DEPTH_BUFFER_BIT |
                        gl::STENCIL_BUFFER_BIT
                    );

                    let swap_result = (window.egl.eglSwapBuffers)(window.egl_display, window.egl_surface);
                    if swap_result != EGL_TRUE {
                        return Err(Create(Egl(format!("EGL: eglSwapBuffers(): Failed to swap OpenGL buffers: {}", swap_result))));
                    }
                },
                // window closed
                xlib::ClientMessage => {
                    let xclient_data = unsafe { cur_xevent.client_message };
                    if (xclient_data.data.as_longs().get(0).copied() == Some(window.wm_delete_window_atom as i64)) {
                        windows_to_close.push(*window_id);
                    }
                },
                _ => { },
            }

        }

        for w in windows_to_close {
            active_windows.remove(&w);
        }

        if active_windows.is_empty() {
            break;
        }
    }

    Ok(0)
}

#[derive(Debug, Clone)]
struct SharedApplicationData {
    inner: Rc<RefCell<ApplicationData>>,
}

// ApplicationData struct that is shared across windows
#[derive(Debug)]
struct ApplicationData {
    data: RefAny,
    config: AppConfig,
    image_cache: ImageCache,
    fc_cache: LazyFcCache,
}

fn display_egl_status(e: EGLint) -> &'static str {

    const BAD_ACCESS: EGLint = 0x3002;
    const BAD_ALLOC: EGLint = 0x3003;
    const BAD_ATTRIBUTE: EGLint = 0x3004;
    const BAD_CONFIG: EGLint = 0x3005;
    const BAD_CONTEXT: EGLint = 0x3006;
    const BAD_CURRENT_SURFACE: EGLint = 0x3007;
    const BAD_DISPLAY: EGLint = 0x3008;
    const BAD_MATCH: EGLint = 0x3009;
    const BAD_NATIVE_PIXMAP: EGLint = 0x300A;
    const BAD_NATIVE_WINDOW: EGLint = 0x300B;
    const BAD_PARAMETER: EGLint = 0x300C;
    const BAD_SURFACE: EGLint = 0x300D;

    match e {
        0x3001 => "not initialized",
        BAD_ACCESS => "bad access",
        BAD_ALLOC => "bad alloc",
        BAD_ATTRIBUTE => "bad attribute",
        BAD_CONTEXT => "bad context",
        BAD_CONFIG => "bad config",
        BAD_CURRENT_SURFACE => "bad current surface",
        BAD_DISPLAY => "bad display",
        BAD_SURFACE => "bad surface",
        BAD_MATCH => "bad match",
        BAD_PARAMETER => "bad parameter",
        BAD_NATIVE_PIXMAP => "bad native pixmap",
        BAD_NATIVE_WINDOW => "bad native window",
        0x300E => "context lost",
        _ => "unknown status code",
    }
}

struct Notifier {}

impl WrRenderNotifier for Notifier {
    fn clone(&self) -> Box<dyn WrRenderNotifier> {
        Box::new(Notifier {})
    }
    fn wake_up(&self, composite_needed: bool) {}
    fn new_frame_ready(
        &self,
        _: WrDocumentId,
        _scrolled: bool,
        composite_needed: bool,
        _render_time: Option<u64>,
    ) {
    }
}

struct X11Window {
    // X11 raw window handle
    pub id: u64,
    // EGL OpenGL 3.2 context
    pub egl_surface: EGLSurface,
    pub egl_display: EGLDisplay,
    pub egl_context: EGLContext,
    // XAtom fired when the window close button is hit
    pub wm_delete_window_atom: i64,
    // X11 library (dynamically loaded)
    pub xlib: Xlib,
    // libEGL.so library (dynamically loaded)
    pub egl: Egl,
    // OpenGL functions, loaded from libEGL.so
    pub gl_functions: GlFunctions,
    /// See azul-core, stores the entire UI (DOM, CSS styles, layout results, etc.)
    pub internal: WindowInternal,
    /// OpenGL context pointer with compiled SVG and FXAA shaders
    pub gl_context_ptr: OptionGlContextPtr,
    /// Main render API that can be used to register and un-register fonts and images
    pub render_api: WrRenderApi,
    /// WebRender renderer implementation (software or hardware)
    pub renderer: Option<WrRenderer>,
    /// Hit-tester, lazily initialized and updated every time the display list changes layout
    pub hit_tester: AsyncHitTester,
}

struct Egl {
    pub library: Library,
    pub eglMakeCurrent: eglMakeCurrentFuncType,
    pub eglSwapBuffers: eglSwapBuffersFuncType,
    pub eglGetDisplay: eglGetDisplayFuncType,
    pub eglInitialize: eglInitializeFuncType,
    pub eglBindAPI: eglBindAPIFuncType,
    pub eglChooseConfig: eglChooseConfigFuncType,
    pub eglCreateWindowSurface: eglCreateWindowSurfaceFuncType,
    pub eglSwapInterval: eglSwapIntervalFuncType,
    pub eglCreateContext: eglCreateContextFuncType,
    pub eglGetError: eglGetErrorFuncType,
}

impl Egl {
    fn new() -> Result<Self, LinuxStartupError> {

        use self::LinuxStartupError::Create;
        use self::LinuxWindowCreateError::{X, Egl};

        let egl = Library::load("libEGL.so")
            .map_err(|e| X(format!("Could not load libEGL: {}", e)))?;

        let eglMakeCurrent: eglMakeCurrentFuncType = egl.get("eglMakeCurrent")
            .and_then(|ptr| if ptr.is_null() { None } else { Some(unsafe { mem::transmute(ptr) }) })
            .ok_or(Create(Egl(format!("EGL: no function eglMakeCurrent"))))?;
        let eglSwapBuffers: eglSwapBuffersFuncType = egl.get("eglSwapBuffers")
            .and_then(|ptr| if ptr.is_null() { None } else { Some(unsafe { mem::transmute(ptr) }) })
            .ok_or(Create(Egl(format!("EGL: no function eglSwapBuffers"))))?;
        let eglGetDisplay: eglGetDisplayFuncType = egl.get("eglGetDisplay")
            .and_then(|ptr| if ptr.is_null() { None } else { Some(unsafe { mem::transmute(ptr) }) })
            .ok_or(Create(Egl(format!("EGL: no function eglGetDisplay"))))?;
        let eglInitialize: eglInitializeFuncType = egl.get("eglInitialize")
            .and_then(|ptr| if ptr.is_null() { None } else { Some(unsafe { mem::transmute(ptr) }) })
            .ok_or(Create(Egl(format!("EGL: no function eglInitialize"))))?;
        let eglBindAPI: eglBindAPIFuncType = egl.get("eglBindAPI")
            .and_then(|ptr| if ptr.is_null() { None } else { Some(unsafe { mem::transmute(ptr) }) })
            .ok_or(Create(Egl(format!("EGL: no function eglBindAPI"))))?;
        let eglChooseConfig: eglChooseConfigFuncType = egl.get("eglChooseConfig")
            .and_then(|ptr| if ptr.is_null() { None } else { Some(unsafe { mem::transmute(ptr) }) })
            .ok_or(Create(Egl(format!("EGL: no function eglChooseConfig"))))?;
        let eglCreateWindowSurface: eglCreateWindowSurfaceFuncType = egl.get("eglCreateWindowSurface")
            .and_then(|ptr| if ptr.is_null() { None } else { Some(unsafe { mem::transmute(ptr) }) })
            .ok_or(Create(Egl(format!("EGL: no function eglCreateWindowSurface"))))?;
        let eglSwapInterval: eglSwapIntervalFuncType = egl.get("eglSwapInterval")
            .and_then(|ptr| if ptr.is_null() { None } else { Some(unsafe { mem::transmute(ptr) }) })
            .ok_or(Create(Egl(format!("EGL: no function eglSwapInterval"))))?;
        let eglCreateContext: eglCreateContextFuncType = egl.get("eglCreateContext")
            .and_then(|ptr| if ptr.is_null() { None } else { Some(unsafe { mem::transmute(ptr) }) })
            .ok_or(Create(Egl(format!("EGL: no function eglCreateContext"))))?;
        let eglGetError: eglGetErrorFuncType = egl.get("eglGetError")
            .and_then(|ptr| if ptr.is_null() { None } else { Some(unsafe { mem::transmute(ptr) }) })
            .ok_or(Create(Egl(format!("EGL: no function eglGetError"))))?;

        Ok(Self {
            library: egl,
            eglMakeCurrent,
            eglSwapBuffers,
            eglGetDisplay,
            eglInitialize,
            eglBindAPI,
            eglChooseConfig,
            eglCreateWindowSurface,
            eglSwapInterval,
            eglCreateContext,
            eglGetError,
        })
    }
}

impl X11Window {
    fn new(
        dpy: &mut X11Display,
        options: &mut WindowCreateOptions,
        shared_application_data: SharedApplicationData
    ) -> Result<Self, LinuxStartupError> {

        use self::LinuxStartupError::Create;
        use self::LinuxWindowCreateError::{X, Egl as EglError};
        use x11_dl::xlib::{
            self, Xlib, XEvent,
            InputOutput, CopyFromParent,
            False, XSetWindowAttributes,
            StructureNotifyMask,
            CWEventMask, XWindowAttributes
        };
        use azul_core::window::{RendererType, HwAcceleration};
        use azul_core::gl::GlContextPtr;
        use webrender::api::ColorF as WrColorF;
        use webrender::ProgramCache as WrProgramCache;
        use crate::{
            compositor::Compositor,
            wr_translate::{
                translate_document_id_wr,
                translate_id_namespace_wr,
                wr_translate_debug_flags,
                wr_translate_document_id,
            },
        };
        use azul_core::callbacks::PipelineId;

        let xlib = Xlib::open()
            .map_err(|e| X(format!("Could not load libX11: {}", e.detail())))?;

        let egl = Egl::new()?;

        // DefaultRootWindow shim
        let scrnum = unsafe { (xlib.XDefaultScreen)(dpy.get()) };
        let root = unsafe { (xlib.XRootWindow)(dpy.get(), scrnum) };

        let mut xattr: XSetWindowAttributes = unsafe { mem::zeroed() };
        xattr.event_mask = StructureNotifyMask;

        let dpi_scale_factor = dpy.get_dpi_scale_factor();
        options.state.size.dpi = (dpi_scale_factor.max(0.0) * 96.0).round() as u32;
        options.state.size.hidpi_factor = dpi_scale_factor;
        options.state.size.system_hidpi_factor = dpi_scale_factor;

        let logical_size = options.state.size.dimensions;
        let physical_size = logical_size.to_physical(dpi_scale_factor);

        let window = unsafe { (xlib.XCreateWindow)(
            dpy.get(), root,
            0, 0,
            logical_size.width.round().max(0.0) as u32,
            logical_size.height.round().max(0.0) as u32,
            0,
            CopyFromParent,
            InputOutput as u32,
            ptr::null_mut(), // = CopyFromParent
            CWEventMask,
            &mut xattr,
        ) };

        if window == 0 {
            return Err(Create(X(format!("X11: XCreateWindow failed"))));
        }

        let window_title = encode_ascii(&options.state.title);
        unsafe { (xlib.XStoreName)(dpy.get(), window, window_title.as_ptr() as *const i8) };

        // subscribe to window close notification
        let wm_protocols_atom = unsafe { (xlib.XInternAtom)(
            dpy.get(),
            encode_ascii("WM_PROTOCOLS").as_ptr() as *const i8,
            False
        ) };

        let mut wm_delete_window_atom = unsafe { (xlib.XInternAtom)(
            dpy.get(),
            encode_ascii("WM_DELETE_WINDOW").as_ptr() as *const i8,
            False
        ) };

        unsafe { (xlib.XSetWMProtocols)(
            dpy.get(),
            window,
            &mut wm_delete_window_atom,
            1
        ) };

        let egl_display = (egl.eglGetDisplay)(dpy.display as *mut c_void);
        if egl_display == EGL_NO_DISPLAY {
            return Err(Create(EglError(format!("EGL: eglGetDisplay(): no display"))));
        }

        let mut major = 0;
        let mut minor = 0;

        let init_result = (egl.eglInitialize)(egl_display, &mut major, &mut minor);
        if init_result != EGL_TRUE {
            return Err(Create(EglError(format!("EGL: eglInitialize(): cannot initialize display: {}", init_result))));
        }

        // choose OpenGL API for EGL, by default it uses OpenGL ES
        let egl_bound = (egl.eglBindAPI)(EGL_OPENGL_API);
        if egl_bound != EGL_TRUE {
            return Err(Create(EglError(format!("EGL: eglBindAPI(): Failed to select OpenGL API for EGL: {}", egl_bound))));
        }

        let egl_attr = [

            EGL_SURFACE_TYPE,      EGL_WINDOW_BIT,
            EGL_CONFORMANT,        EGL_OPENGL_BIT,
            EGL_RENDERABLE_TYPE,   EGL_OPENGL_BIT,
            EGL_COLOR_BUFFER_TYPE, EGL_RGB_BUFFER,

            EGL_RED_SIZE,      8,
            EGL_GREEN_SIZE,    8,
            EGL_BLUE_SIZE,     8,
            EGL_DEPTH_SIZE,   24,
            EGL_STENCIL_SIZE,  8,

            EGL_NONE,
        ];

        let mut config: EGLConfig = unsafe { mem::zeroed() };
        let mut count = 0;
        let egl_config_chosen = (egl.eglChooseConfig)(egl_display, egl_attr.as_ptr(), &mut config, 1, &mut count);
        if egl_config_chosen != EGL_TRUE {
            return Err(Create(EglError(format!("EGL: eglChooseConfig(): Cannot choose EGL config: {}", egl_config_chosen))));
        }

        if count != 1 {
            return Err(Create(EglError(format!("EGL: eglChooseConfig(): Expected 1 EglConfig, got {}", count))));
        }

        let egl_surface_attr = [
            EGL_GL_COLORSPACE, EGL_GL_COLORSPACE_LINEAR,
            EGL_RENDER_BUFFER, EGL_BACK_BUFFER,
            EGL_NONE,
        ];

        let egl_surface = (egl.eglCreateWindowSurface)(
            egl_display,
            config,
            unsafe { mem::transmute(window as usize) },
            egl_surface_attr.as_ptr()
        );

        if egl_surface == EGL_NO_SURFACE {
            return Err(Create(EglError(format!("EGL: eglCreateWindowSurface(): no surface found"))));
        }

        let egl_context_attr = [
            EGL_CONTEXT_MAJOR_VERSION, 3,
            EGL_CONTEXT_MINOR_VERSION, 2,
            EGL_CONTEXT_OPENGL_PROFILE_MASK, EGL_CONTEXT_OPENGL_CORE_PROFILE_BIT,
            EGL_NONE,
        ];

        let egl_context = (egl.eglCreateContext)(egl_display, config, EGL_NO_CONTEXT, egl_context_attr.as_ptr());
        if egl_context == EGL_NO_CONTEXT {
            let err = (egl.eglGetError)();
            return Err(Create(EglError(format!("EGL: eglCreateContext() failed with status {} = {}", err, display_egl_status(err)))));
        }

        let egl_is_current = (egl.eglMakeCurrent)(egl_display, egl_surface, egl_surface, egl_context);
        if egl_is_current != EGL_TRUE {
            return Err(Create(EglError(format!("EGL: eglMakeCurrent(): failed to make context current: {}", egl_is_current))));
        }

        let mut gl_functions = GlFunctions::initialize();
        gl_functions.load();

        // Initialize WebRender
        let mut rt = RendererType::Software;

        let renderer_types = match options.renderer.into_option() {
            Some(s) => match s.hw_accel {
                HwAcceleration::DontCare => vec![RendererType::Hardware, RendererType::Software],
                HwAcceleration::Enabled => vec![RendererType::Hardware],
                HwAcceleration::Disabled => vec![RendererType::Software],
            },
            None => vec![RendererType::Hardware, RendererType::Software],
        };

        // TODO: allow fallback software rendering -
        // currently just takes the first option
        for r in renderer_types {
            rt = r;
            break;
        }

        // compiles SVG and FXAA shader programs...
        let gl_context_ptr = Some(GlContextPtr::new(
            rt,
            gl_functions.functions.clone()
        )).into();

        // Invoke callback to initialize UI for the first time
        let (mut renderer, sender) = WrRenderer::new(
            gl_functions.functions.clone(),
            Box::new(Notifier {}),
            WrRendererOptions {
                resource_override_path: None,
                use_optimized_shaders: true,
                enable_aa: true,
                enable_subpixel_aa: true,
                force_subpixel_aa: true,
                clear_color: WrColorF {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0,
                }, // transparent
                panic_on_gl_error: false,
                precache_flags: WrShaderPrecacheFlags::EMPTY,
                cached_programs: Some(WrProgramCache::new(None)),
                enable_multithreading: true,
                debug_flags: wr_translate_debug_flags(&options.state.debug_state),
                ..WrRendererOptions::default()
            },
            WR_SHADER_CACHE,
        ).map_err(|e| Create(EglError(format!("Could not init WebRender: {:?}", e))))?;

        renderer.set_external_image_handler(Box::new(Compositor::default()));

        let mut render_api = sender.create_api();

        let framebuffer_size = WrDeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);
        let document_id = translate_document_id_wr(render_api.add_document(framebuffer_size));
        let pipeline_id = PipelineId::new();
        let id_namespace = translate_id_namespace_wr(render_api.get_namespace_id());

        // hit tester will be empty on startup
        let hit_tester = render_api
            .request_hit_tester(wr_translate_document_id(document_id))
            .resolve();

        let hit_tester_ref = &*hit_tester;

        let mut appdata_lock = match shared_application_data.inner.try_borrow_mut() {
            Ok(o) => o,
            Err(e) => return Err(Create(EglError(format!("could not lock application data")))),
        };

        let appdata_lock = &mut *appdata_lock;
        let fc_cache = &mut appdata_lock.fc_cache;
        let image_cache = &appdata_lock.image_cache;
        let data = &mut appdata_lock.data;

        let mut initial_resource_updates = Vec::new();
        let mut internal = fc_cache.apply_closure(|fc_cache| {
            use azul_core::window::WindowInternalInit;

            WindowInternal::new(
                WindowInternalInit {
                    window_create_options: options.clone(),
                    document_id,
                    id_namespace,
                },
                data,
                image_cache,
                &gl_context_ptr,
                &mut initial_resource_updates,
                &crate::app::CALLBACKS,
                fc_cache,
                azul_layout::do_the_relayout,
                |window_state, scroll_states, layout_results| {
                    crate::wr_translate::fullhittest_new_webrender(
                        hit_tester_ref,
                        document_id,
                        window_state.focused_node,
                        layout_results,
                        &window_state.mouse_state.cursor_position,
                        window_state.size.hidpi_factor,
                    )
                },
            )
        });

        let mut txn = WrTransaction::new();

        // re-layout the window content for the first frame
        // (since the width / height might have changed)
        let size = internal.current_window_state.size.clone();
        let theme = internal.current_window_state.theme;
        let resize_result = fc_cache.apply_closure(|fc_cache| {
            internal.do_quick_resize(
                &image_cache,
                &crate::app::CALLBACKS,
                azul_layout::do_the_relayout,
                fc_cache,
                &gl_context_ptr,
                &size,
                theme,
            )
        });

        wr_synchronize_updated_images(resize_result.updated_images, &document_id, &mut txn);


        txn.set_document_view(
            WrDeviceIntRect::from_size(
                WrDeviceIntSize::new(physical_size.width as i32, physical_size.height as i32),
            )
        );

        render_api.send_transaction(wr_translate_document_id(internal.document_id), txn);

        render_api.flush_scene_builder();

        // Build the display list and send it to webrender for the first time
        rebuild_display_list(
            &mut internal,
            &mut render_api,
            &appdata_lock.image_cache,
            initial_resource_updates,
        );

        render_api.flush_scene_builder();

        generate_frame(
            &mut internal,
            &mut render_api,
            true,
        );

        render_api.flush_scene_builder();

        // Update the hit-tester to account for the new hit-testing functionality
        let hit_tester = render_api.request_hit_tester(wr_translate_document_id(document_id));

        Ok(Self {
            egl_surface,
            egl_display,
            egl_context,
            wm_delete_window_atom: wm_delete_window_atom as i64,
            id: window,
            xlib,
            egl,
            render_api,
            hit_tester: AsyncHitTester::Requested(hit_tester),
            internal,
            renderer: Some(renderer),
            gl_functions,
            gl_context_ptr,
        })
    }

    fn make_current(&self) {
        (self.egl.eglMakeCurrent)(
            self.egl_display,
            self.egl_surface,
            self.egl_surface,
            self.egl_context
        );
    }

    fn show(&self, dpy: &mut X11Display) {
        unsafe { (self.xlib.XMapWindow)(dpy.get(), self.id) };
    }
}

pub struct X11Display {
    pub display: *mut Display,
    pub xopen_display: unsafe extern "C" fn(_: *const c_char) -> *mut Display,
    pub xclose_display:  unsafe extern "C" fn(_: *mut Display) -> c_int,
}

impl X11Display {

    pub fn get<'a>(&'a mut self) -> &'a mut Display {
        unsafe { &mut *self.display }
    }

    pub fn open(xlib: &Xlib) -> Option<Self> {

        let dpy = unsafe { (xlib.XOpenDisplay)(&0) };

        if dpy.is_null() {
            return None;
        }

        Some(Self {
            display: dpy,
            xopen_display: xlib.XOpenDisplay,
            xclose_display: xlib.XCloseDisplay,
        })
    }

    /// Return the DPI on X11 systems
    ///
    /// Note: slow - cache output!
    pub fn get_dpi_scale_factor(&self) -> f32 {

        use std::env;
        use std::process::Command;

        // Execute "gsettings get org.gnome.desktop.interface text-scaling-factor"
        // and parse the output
        let gsettings_dpi_factor =
            Command::new("gsettings")
                .arg("get")
                .arg("org.gnome.desktop.interface")
                .arg("text-scaling-factor")
                .output().ok()
                .map(|output| output.stdout)
                .and_then(|stdout_bytes| String::from_utf8(stdout_bytes).ok())
                .map(|stdout_string| stdout_string.lines().collect::<String>())
                .and_then(|gsettings_output| gsettings_output.parse::<f32>().ok());

        if let Some(s) = gsettings_dpi_factor {
            return s;
        }

        // Failed, try parsing QT_FONT_DPI
        let qt_font_dpi = env::var("QT_FONT_DPI")
            .ok()
            .and_then(|font_dpi| font_dpi.parse::<f32>().ok());

        if let Some(s) = qt_font_dpi {
            return s;
        }

        // Failed again, try getting monitor info from XRandR
        // TODO
        return 1.0;

        /*
            XRRScreenResources *xrrr = XRRGetScreenResources(d, w);
            XRRCrtcInfo *xrrci;
            int i;
            int ncrtc = xrrr->ncrtc;
            for (i = 0; i < ncrtc; ++i) {
                xrrci = XRRGetCrtcInfo(d, xrrr, xrrr->crtcs[i]);
                printf("%dx%d+%d+%d\n", xrrci->width, xrrci->height, xrrci->x, xrrci->y);
                XRRFreeCrtcInfo(xrrci);
            }
            XRRFreeScreenResources(xrrr);
        */


    }
}

impl Drop for X11Display {
    fn drop(&mut self) {
        // TODO: error checking?
        let _ = unsafe { (self.xclose_display)(self.display) };
    }
}

/// A platform-specific equivalent of the cross-platform `Library`.
pub struct Library {
    name: &'static str,
    ptr: *mut raw::c_void
}

unsafe impl Send for Library {}
unsafe impl Sync for Library {}

impl Library {

    /// Dynamically load an arbitrary library by its name (dlopen)
    pub fn load(name: &'static str) -> Result<Self, String> {

        use alloc::borrow::Cow;
        use std::ffi::{CString, CStr};

        const RTLD_NOW: raw::c_int = 2;

        let cow = CString::new(name.as_bytes()).map_err(|e| String::new())?;
        let ptr = unsafe { dlopen(cow.as_ptr(), RTLD_NOW) };

        if ptr.is_null() {
            let dlerr = unsafe { CStr::from_ptr(dlerror()) };
            Err(dlerr.to_str().ok().map(|s| s.to_string()).unwrap_or_default())
        } else {
            Ok(Self { name, ptr })
        }
    }

    pub fn get(&self, symbol: &str) -> Option<*mut raw::c_void> {

        use std::ffi::CString;

        let symbol_name_new = CString::new(symbol.as_bytes()).ok()?;
        let symbol_new = unsafe { dlsym(self.ptr, symbol_name_new.as_ptr()) };
        let error = unsafe { dlerror() };
        if error.is_null() {
            Some(symbol_new)
        } else {
            None
        }
    }
}

impl fmt::Debug for Library {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.name.fmt(f)
    }
}

impl fmt::Display for Library {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.name.fmt(f)
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        unsafe { dlclose(self.ptr) };
    }
}

// OpenGL functions from wglGetProcAddress OR loaded from opengl32.dll
struct GlFunctions {
    _opengl32_dll_handle: Option<Library>,
    // implements Rc<dyn gleam::Gl>!
    functions: Rc<GenericGlContext>,
}

impl fmt::Debug for GlFunctions {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self._opengl32_dll_handle.as_ref().map(|f| f.ptr as usize).fmt(f)?;
        Ok(())
    }
}


fn encode_ascii(input: &str) -> Vec<u8> {
    input
    .chars()
    .filter(|c| c.is_ascii())
    .map(|c| c as u8)
    .chain(Some(0).into_iter())
    .collect::<Vec<_>>()
}

impl GlFunctions {

    // Initializes the DLL, but does not load the functions yet
    fn initialize() -> Self {

        // zero-initialize all function pointers
        let context: GenericGlContext = unsafe { mem::zeroed() };
        let opengl32_dll = Library::load("GL").ok();

        Self {
            _opengl32_dll_handle: opengl32_dll,
            functions: Rc::new(context),
        }
    }

    // Assuming the OpenGL context is current, loads the OpenGL function pointers
    fn load(&mut self) {

        fn get_func(s: &'static str, opengl32_dll: &Option<Library>) -> *mut gl_context_loader::c_void {
            opengl32_dll
            .as_ref()
            .and_then(|l| l.get(s))
            .unwrap_or(core::ptr::null_mut())
            as *mut gl_context_loader::c_void
        }

        self.functions = Rc::new(GenericGlContext {
            glAccum: get_func("glAccum", &self._opengl32_dll_handle),
            glActiveTexture: get_func("glActiveTexture", &self._opengl32_dll_handle),
            glAlphaFunc: get_func("glAlphaFunc", &self._opengl32_dll_handle),
            glAreTexturesResident: get_func("glAreTexturesResident", &self._opengl32_dll_handle),
            glArrayElement: get_func("glArrayElement", &self._opengl32_dll_handle),
            glAttachShader: get_func("glAttachShader", &self._opengl32_dll_handle),
            glBegin: get_func("glBegin", &self._opengl32_dll_handle),
            glBeginConditionalRender: get_func(
                "glBeginConditionalRender",
                &self._opengl32_dll_handle,
            ),
            glBeginQuery: get_func("glBeginQuery", &self._opengl32_dll_handle),
            glBeginTransformFeedback: get_func(
                "glBeginTransformFeedback",
                &self._opengl32_dll_handle,
            ),
            glBindAttribLocation: get_func("glBindAttribLocation", &self._opengl32_dll_handle),
            glBindBuffer: get_func("glBindBuffer", &self._opengl32_dll_handle),
            glBindBufferBase: get_func("glBindBufferBase", &self._opengl32_dll_handle),
            glBindBufferRange: get_func("glBindBufferRange", &self._opengl32_dll_handle),
            glBindFragDataLocation: get_func("glBindFragDataLocation", &self._opengl32_dll_handle),
            glBindFragDataLocationIndexed: get_func(
                "glBindFragDataLocationIndexed",
                &self._opengl32_dll_handle,
            ),
            glBindFramebuffer: get_func("glBindFramebuffer", &self._opengl32_dll_handle),
            glBindRenderbuffer: get_func("glBindRenderbuffer", &self._opengl32_dll_handle),
            glBindSampler: get_func("glBindSampler", &self._opengl32_dll_handle),
            glBindTexture: get_func("glBindTexture", &self._opengl32_dll_handle),
            glBindVertexArray: get_func("glBindVertexArray", &self._opengl32_dll_handle),
            glBindVertexArrayAPPLE: get_func("glBindVertexArrayAPPLE", &self._opengl32_dll_handle),
            glBitmap: get_func("glBitmap", &self._opengl32_dll_handle),
            glBlendBarrierKHR: get_func("glBlendBarrierKHR", &self._opengl32_dll_handle),
            glBlendColor: get_func("glBlendColor", &self._opengl32_dll_handle),
            glBlendEquation: get_func("glBlendEquation", &self._opengl32_dll_handle),
            glBlendEquationSeparate: get_func("glBlendEquationSeparate", &self._opengl32_dll_handle),
            glBlendFunc: get_func("glBlendFunc", &self._opengl32_dll_handle),
            glBlendFuncSeparate: get_func("glBlendFuncSeparate", &self._opengl32_dll_handle),
            glBlitFramebuffer: get_func("glBlitFramebuffer", &self._opengl32_dll_handle),
            glBufferData: get_func("glBufferData", &self._opengl32_dll_handle),
            glBufferStorage: get_func("glBufferStorage", &self._opengl32_dll_handle),
            glBufferSubData: get_func("glBufferSubData", &self._opengl32_dll_handle),
            glCallList: get_func("glCallList", &self._opengl32_dll_handle),
            glCallLists: get_func("glCallLists", &self._opengl32_dll_handle),
            glCheckFramebufferStatus: get_func(
                "glCheckFramebufferStatus",
                &self._opengl32_dll_handle,
            ),
            glClampColor: get_func("glClampColor", &self._opengl32_dll_handle),
            glClear: get_func("glClear", &self._opengl32_dll_handle),
            glClearAccum: get_func("glClearAccum", &self._opengl32_dll_handle),
            glClearBufferfi: get_func("glClearBufferfi", &self._opengl32_dll_handle),
            glClearBufferfv: get_func("glClearBufferfv", &self._opengl32_dll_handle),
            glClearBufferiv: get_func("glClearBufferiv", &self._opengl32_dll_handle),
            glClearBufferuiv: get_func("glClearBufferuiv", &self._opengl32_dll_handle),
            glClearColor: get_func("glClearColor", &self._opengl32_dll_handle),
            glClearDepth: get_func("glClearDepth", &self._opengl32_dll_handle),
            glClearIndex: get_func("glClearIndex", &self._opengl32_dll_handle),
            glClearStencil: get_func("glClearStencil", &self._opengl32_dll_handle),
            glClientActiveTexture: get_func("glClientActiveTexture", &self._opengl32_dll_handle),
            glClientWaitSync: get_func("glClientWaitSync", &self._opengl32_dll_handle),
            glClipPlane: get_func("glClipPlane", &self._opengl32_dll_handle),
            glColor3b: get_func("glColor3b", &self._opengl32_dll_handle),
            glColor3bv: get_func("glColor3bv", &self._opengl32_dll_handle),
            glColor3d: get_func("glColor3d", &self._opengl32_dll_handle),
            glColor3dv: get_func("glColor3dv", &self._opengl32_dll_handle),
            glColor3f: get_func("glColor3f", &self._opengl32_dll_handle),
            glColor3fv: get_func("glColor3fv", &self._opengl32_dll_handle),
            glColor3i: get_func("glColor3i", &self._opengl32_dll_handle),
            glColor3iv: get_func("glColor3iv", &self._opengl32_dll_handle),
            glColor3s: get_func("glColor3s", &self._opengl32_dll_handle),
            glColor3sv: get_func("glColor3sv", &self._opengl32_dll_handle),
            glColor3ub: get_func("glColor3ub", &self._opengl32_dll_handle),
            glColor3ubv: get_func("glColor3ubv", &self._opengl32_dll_handle),
            glColor3ui: get_func("glColor3ui", &self._opengl32_dll_handle),
            glColor3uiv: get_func("glColor3uiv", &self._opengl32_dll_handle),
            glColor3us: get_func("glColor3us", &self._opengl32_dll_handle),
            glColor3usv: get_func("glColor3usv", &self._opengl32_dll_handle),
            glColor4b: get_func("glColor4b", &self._opengl32_dll_handle),
            glColor4bv: get_func("glColor4bv", &self._opengl32_dll_handle),
            glColor4d: get_func("glColor4d", &self._opengl32_dll_handle),
            glColor4dv: get_func("glColor4dv", &self._opengl32_dll_handle),
            glColor4f: get_func("glColor4f", &self._opengl32_dll_handle),
            glColor4fv: get_func("glColor4fv", &self._opengl32_dll_handle),
            glColor4i: get_func("glColor4i", &self._opengl32_dll_handle),
            glColor4iv: get_func("glColor4iv", &self._opengl32_dll_handle),
            glColor4s: get_func("glColor4s", &self._opengl32_dll_handle),
            glColor4sv: get_func("glColor4sv", &self._opengl32_dll_handle),
            glColor4ub: get_func("glColor4ub", &self._opengl32_dll_handle),
            glColor4ubv: get_func("glColor4ubv", &self._opengl32_dll_handle),
            glColor4ui: get_func("glColor4ui", &self._opengl32_dll_handle),
            glColor4uiv: get_func("glColor4uiv", &self._opengl32_dll_handle),
            glColor4us: get_func("glColor4us", &self._opengl32_dll_handle),
            glColor4usv: get_func("glColor4usv", &self._opengl32_dll_handle),
            glColorMask: get_func("glColorMask", &self._opengl32_dll_handle),
            glColorMaski: get_func("glColorMaski", &self._opengl32_dll_handle),
            glColorMaterial: get_func("glColorMaterial", &self._opengl32_dll_handle),
            glColorP3ui: get_func("glColorP3ui", &self._opengl32_dll_handle),
            glColorP3uiv: get_func("glColorP3uiv", &self._opengl32_dll_handle),
            glColorP4ui: get_func("glColorP4ui", &self._opengl32_dll_handle),
            glColorP4uiv: get_func("glColorP4uiv", &self._opengl32_dll_handle),
            glColorPointer: get_func("glColorPointer", &self._opengl32_dll_handle),
            glCompileShader: get_func("glCompileShader", &self._opengl32_dll_handle),
            glCompressedTexImage1D: get_func("glCompressedTexImage1D", &self._opengl32_dll_handle),
            glCompressedTexImage2D: get_func("glCompressedTexImage2D", &self._opengl32_dll_handle),
            glCompressedTexImage3D: get_func("glCompressedTexImage3D", &self._opengl32_dll_handle),
            glCompressedTexSubImage1D: get_func(
                "glCompressedTexSubImage1D",
                &self._opengl32_dll_handle,
            ),
            glCompressedTexSubImage2D: get_func(
                "glCompressedTexSubImage2D",
                &self._opengl32_dll_handle,
            ),
            glCompressedTexSubImage3D: get_func(
                "glCompressedTexSubImage3D",
                &self._opengl32_dll_handle,
            ),
            glCopyBufferSubData: get_func("glCopyBufferSubData", &self._opengl32_dll_handle),
            glCopyImageSubData: get_func("glCopyImageSubData", &self._opengl32_dll_handle),
            glCopyPixels: get_func("glCopyPixels", &self._opengl32_dll_handle),
            glCopyTexImage1D: get_func("glCopyTexImage1D", &self._opengl32_dll_handle),
            glCopyTexImage2D: get_func("glCopyTexImage2D", &self._opengl32_dll_handle),
            glCopyTexSubImage1D: get_func("glCopyTexSubImage1D", &self._opengl32_dll_handle),
            glCopyTexSubImage2D: get_func("glCopyTexSubImage2D", &self._opengl32_dll_handle),
            glCopyTexSubImage3D: get_func("glCopyTexSubImage3D", &self._opengl32_dll_handle),
            glCreateProgram: get_func("glCreateProgram", &self._opengl32_dll_handle),
            glCreateShader: get_func("glCreateShader", &self._opengl32_dll_handle),
            glCullFace: get_func("glCullFace", &self._opengl32_dll_handle),
            glDebugMessageCallback: get_func("glDebugMessageCallback", &self._opengl32_dll_handle),
            glDebugMessageCallbackKHR: get_func(
                "glDebugMessageCallbackKHR",
                &self._opengl32_dll_handle,
            ),
            glDebugMessageControl: get_func("glDebugMessageControl", &self._opengl32_dll_handle),
            glDebugMessageControlKHR: get_func(
                "glDebugMessageControlKHR",
                &self._opengl32_dll_handle,
            ),
            glDebugMessageInsert: get_func("glDebugMessageInsert", &self._opengl32_dll_handle),
            glDebugMessageInsertKHR: get_func("glDebugMessageInsertKHR", &self._opengl32_dll_handle),
            glDeleteBuffers: get_func("glDeleteBuffers", &self._opengl32_dll_handle),
            glDeleteFencesAPPLE: get_func("glDeleteFencesAPPLE", &self._opengl32_dll_handle),
            glDeleteFramebuffers: get_func("glDeleteFramebuffers", &self._opengl32_dll_handle),
            glDeleteLists: get_func("glDeleteLists", &self._opengl32_dll_handle),
            glDeleteProgram: get_func("glDeleteProgram", &self._opengl32_dll_handle),
            glDeleteQueries: get_func("glDeleteQueries", &self._opengl32_dll_handle),
            glDeleteRenderbuffers: get_func("glDeleteRenderbuffers", &self._opengl32_dll_handle),
            glDeleteSamplers: get_func("glDeleteSamplers", &self._opengl32_dll_handle),
            glDeleteShader: get_func("glDeleteShader", &self._opengl32_dll_handle),
            glDeleteSync: get_func("glDeleteSync", &self._opengl32_dll_handle),
            glDeleteTextures: get_func("glDeleteTextures", &self._opengl32_dll_handle),
            glDeleteVertexArrays: get_func("glDeleteVertexArrays", &self._opengl32_dll_handle),
            glDeleteVertexArraysAPPLE: get_func(
                "glDeleteVertexArraysAPPLE",
                &self._opengl32_dll_handle,
            ),
            glDepthFunc: get_func("glDepthFunc", &self._opengl32_dll_handle),
            glDepthMask: get_func("glDepthMask", &self._opengl32_dll_handle),
            glDepthRange: get_func("glDepthRange", &self._opengl32_dll_handle),
            glDetachShader: get_func("glDetachShader", &self._opengl32_dll_handle),
            glDisable: get_func("glDisable", &self._opengl32_dll_handle),
            glDisableClientState: get_func("glDisableClientState", &self._opengl32_dll_handle),
            glDisableVertexAttribArray: get_func(
                "glDisableVertexAttribArray",
                &self._opengl32_dll_handle,
            ),
            glDisablei: get_func("glDisablei", &self._opengl32_dll_handle),
            glDrawArrays: get_func("glDrawArrays", &self._opengl32_dll_handle),
            glDrawArraysInstanced: get_func("glDrawArraysInstanced", &self._opengl32_dll_handle),
            glDrawBuffer: get_func("glDrawBuffer", &self._opengl32_dll_handle),
            glDrawBuffers: get_func("glDrawBuffers", &self._opengl32_dll_handle),
            glDrawElements: get_func("glDrawElements", &self._opengl32_dll_handle),
            glDrawElementsBaseVertex: get_func(
                "glDrawElementsBaseVertex",
                &self._opengl32_dll_handle,
            ),
            glDrawElementsInstanced: get_func("glDrawElementsInstanced", &self._opengl32_dll_handle),
            glDrawElementsInstancedBaseVertex: get_func(
                "glDrawElementsInstancedBaseVertex",
                &self._opengl32_dll_handle,
            ),
            glDrawPixels: get_func("glDrawPixels", &self._opengl32_dll_handle),
            glDrawRangeElements: get_func("glDrawRangeElements", &self._opengl32_dll_handle),
            glDrawRangeElementsBaseVertex: get_func(
                "glDrawRangeElementsBaseVertex",
                &self._opengl32_dll_handle,
            ),
            glEdgeFlag: get_func("glEdgeFlag", &self._opengl32_dll_handle),
            glEdgeFlagPointer: get_func("glEdgeFlagPointer", &self._opengl32_dll_handle),
            glEdgeFlagv: get_func("glEdgeFlagv", &self._opengl32_dll_handle),
            glEnable: get_func("glEnable", &self._opengl32_dll_handle),
            glEnableClientState: get_func("glEnableClientState", &self._opengl32_dll_handle),
            glEnableVertexAttribArray: get_func(
                "glEnableVertexAttribArray",
                &self._opengl32_dll_handle,
            ),
            glEnablei: get_func("glEnablei", &self._opengl32_dll_handle),
            glEnd: get_func("glEnd", &self._opengl32_dll_handle),
            glEndConditionalRender: get_func("glEndConditionalRender", &self._opengl32_dll_handle),
            glEndList: get_func("glEndList", &self._opengl32_dll_handle),
            glEndQuery: get_func("glEndQuery", &self._opengl32_dll_handle),
            glEndTransformFeedback: get_func("glEndTransformFeedback", &self._opengl32_dll_handle),
            glEvalCoord1d: get_func("glEvalCoord1d", &self._opengl32_dll_handle),
            glEvalCoord1dv: get_func("glEvalCoord1dv", &self._opengl32_dll_handle),
            glEvalCoord1f: get_func("glEvalCoord1f", &self._opengl32_dll_handle),
            glEvalCoord1fv: get_func("glEvalCoord1fv", &self._opengl32_dll_handle),
            glEvalCoord2d: get_func("glEvalCoord2d", &self._opengl32_dll_handle),
            glEvalCoord2dv: get_func("glEvalCoord2dv", &self._opengl32_dll_handle),
            glEvalCoord2f: get_func("glEvalCoord2f", &self._opengl32_dll_handle),
            glEvalCoord2fv: get_func("glEvalCoord2fv", &self._opengl32_dll_handle),
            glEvalMesh1: get_func("glEvalMesh1", &self._opengl32_dll_handle),
            glEvalMesh2: get_func("glEvalMesh2", &self._opengl32_dll_handle),
            glEvalPoint1: get_func("glEvalPoint1", &self._opengl32_dll_handle),
            glEvalPoint2: get_func("glEvalPoint2", &self._opengl32_dll_handle),
            glFeedbackBuffer: get_func("glFeedbackBuffer", &self._opengl32_dll_handle),
            glFenceSync: get_func("glFenceSync", &self._opengl32_dll_handle),
            glFinish: get_func("glFinish", &self._opengl32_dll_handle),
            glFinishFenceAPPLE: get_func("glFinishFenceAPPLE", &self._opengl32_dll_handle),
            glFinishObjectAPPLE: get_func("glFinishObjectAPPLE", &self._opengl32_dll_handle),
            glFlush: get_func("glFlush", &self._opengl32_dll_handle),
            glFlushMappedBufferRange: get_func(
                "glFlushMappedBufferRange",
                &self._opengl32_dll_handle,
            ),
            glFogCoordPointer: get_func("glFogCoordPointer", &self._opengl32_dll_handle),
            glFogCoordd: get_func("glFogCoordd", &self._opengl32_dll_handle),
            glFogCoorddv: get_func("glFogCoorddv", &self._opengl32_dll_handle),
            glFogCoordf: get_func("glFogCoordf", &self._opengl32_dll_handle),
            glFogCoordfv: get_func("glFogCoordfv", &self._opengl32_dll_handle),
            glFogf: get_func("glFogf", &self._opengl32_dll_handle),
            glFogfv: get_func("glFogfv", &self._opengl32_dll_handle),
            glFogi: get_func("glFogi", &self._opengl32_dll_handle),
            glFogiv: get_func("glFogiv", &self._opengl32_dll_handle),
            glFramebufferRenderbuffer: get_func(
                "glFramebufferRenderbuffer",
                &self._opengl32_dll_handle,
            ),
            glFramebufferTexture: get_func("glFramebufferTexture", &self._opengl32_dll_handle),
            glFramebufferTexture1D: get_func("glFramebufferTexture1D", &self._opengl32_dll_handle),
            glFramebufferTexture2D: get_func("glFramebufferTexture2D", &self._opengl32_dll_handle),
            glFramebufferTexture3D: get_func("glFramebufferTexture3D", &self._opengl32_dll_handle),
            glFramebufferTextureLayer: get_func(
                "glFramebufferTextureLayer",
                &self._opengl32_dll_handle,
            ),
            glFrontFace: get_func("glFrontFace", &self._opengl32_dll_handle),
            glFrustum: get_func("glFrustum", &self._opengl32_dll_handle),
            glGenBuffers: get_func("glGenBuffers", &self._opengl32_dll_handle),
            glGenFencesAPPLE: get_func("glGenFencesAPPLE", &self._opengl32_dll_handle),
            glGenFramebuffers: get_func("glGenFramebuffers", &self._opengl32_dll_handle),
            glGenLists: get_func("glGenLists", &self._opengl32_dll_handle),
            glGenQueries: get_func("glGenQueries", &self._opengl32_dll_handle),
            glGenRenderbuffers: get_func("glGenRenderbuffers", &self._opengl32_dll_handle),
            glGenSamplers: get_func("glGenSamplers", &self._opengl32_dll_handle),
            glGenTextures: get_func("glGenTextures", &self._opengl32_dll_handle),
            glGenVertexArrays: get_func("glGenVertexArrays", &self._opengl32_dll_handle),
            glGenVertexArraysAPPLE: get_func("glGenVertexArraysAPPLE", &self._opengl32_dll_handle),
            glGenerateMipmap: get_func("glGenerateMipmap", &self._opengl32_dll_handle),
            glGetActiveAttrib: get_func("glGetActiveAttrib", &self._opengl32_dll_handle),
            glGetActiveUniform: get_func("glGetActiveUniform", &self._opengl32_dll_handle),
            glGetActiveUniformBlockName: get_func(
                "glGetActiveUniformBlockName",
                &self._opengl32_dll_handle,
            ),
            glGetActiveUniformBlockiv: get_func(
                "glGetActiveUniformBlockiv",
                &self._opengl32_dll_handle,
            ),
            glGetActiveUniformName: get_func("glGetActiveUniformName", &self._opengl32_dll_handle),
            glGetActiveUniformsiv: get_func("glGetActiveUniformsiv", &self._opengl32_dll_handle),
            glGetAttachedShaders: get_func("glGetAttachedShaders", &self._opengl32_dll_handle),
            glGetAttribLocation: get_func("glGetAttribLocation", &self._opengl32_dll_handle),
            glGetBooleani_v: get_func("glGetBooleani_v", &self._opengl32_dll_handle),
            glGetBooleanv: get_func("glGetBooleanv", &self._opengl32_dll_handle),
            glGetBufferParameteri64v: get_func(
                "glGetBufferParameteri64v",
                &self._opengl32_dll_handle,
            ),
            glGetBufferParameteriv: get_func("glGetBufferParameteriv", &self._opengl32_dll_handle),
            glGetBufferPointerv: get_func("glGetBufferPointerv", &self._opengl32_dll_handle),
            glGetBufferSubData: get_func("glGetBufferSubData", &self._opengl32_dll_handle),
            glGetClipPlane: get_func("glGetClipPlane", &self._opengl32_dll_handle),
            glGetCompressedTexImage: get_func("glGetCompressedTexImage", &self._opengl32_dll_handle),
            glGetDebugMessageLog: get_func("glGetDebugMessageLog", &self._opengl32_dll_handle),
            glGetDebugMessageLogKHR: get_func("glGetDebugMessageLogKHR", &self._opengl32_dll_handle),
            glGetDoublev: get_func("glGetDoublev", &self._opengl32_dll_handle),
            glGetError: get_func("glGetError", &self._opengl32_dll_handle),
            glGetFloatv: get_func("glGetFloatv", &self._opengl32_dll_handle),
            glGetFragDataIndex: get_func("glGetFragDataIndex", &self._opengl32_dll_handle),
            glGetFragDataLocation: get_func("glGetFragDataLocation", &self._opengl32_dll_handle),
            glGetFramebufferAttachmentParameteriv: get_func(
                "glGetFramebufferAttachmentParameteriv",
                &self._opengl32_dll_handle,
            ),
            glGetInteger64i_v: get_func("glGetInteger64i_v", &self._opengl32_dll_handle),
            glGetInteger64v: get_func("glGetInteger64v", &self._opengl32_dll_handle),
            glGetIntegeri_v: get_func("glGetIntegeri_v", &self._opengl32_dll_handle),
            glGetIntegerv: get_func("glGetIntegerv", &self._opengl32_dll_handle),
            glGetLightfv: get_func("glGetLightfv", &self._opengl32_dll_handle),
            glGetLightiv: get_func("glGetLightiv", &self._opengl32_dll_handle),
            glGetMapdv: get_func("glGetMapdv", &self._opengl32_dll_handle),
            glGetMapfv: get_func("glGetMapfv", &self._opengl32_dll_handle),
            glGetMapiv: get_func("glGetMapiv", &self._opengl32_dll_handle),
            glGetMaterialfv: get_func("glGetMaterialfv", &self._opengl32_dll_handle),
            glGetMaterialiv: get_func("glGetMaterialiv", &self._opengl32_dll_handle),
            glGetMultisamplefv: get_func("glGetMultisamplefv", &self._opengl32_dll_handle),
            glGetObjectLabel: get_func("glGetObjectLabel", &self._opengl32_dll_handle),
            glGetObjectLabelKHR: get_func("glGetObjectLabelKHR", &self._opengl32_dll_handle),
            glGetObjectPtrLabel: get_func("glGetObjectPtrLabel", &self._opengl32_dll_handle),
            glGetObjectPtrLabelKHR: get_func("glGetObjectPtrLabelKHR", &self._opengl32_dll_handle),
            glGetPixelMapfv: get_func("glGetPixelMapfv", &self._opengl32_dll_handle),
            glGetPixelMapuiv: get_func("glGetPixelMapuiv", &self._opengl32_dll_handle),
            glGetPixelMapusv: get_func("glGetPixelMapusv", &self._opengl32_dll_handle),
            glGetPointerv: get_func("glGetPointerv", &self._opengl32_dll_handle),
            glGetPointervKHR: get_func("glGetPointervKHR", &self._opengl32_dll_handle),
            glGetPolygonStipple: get_func("glGetPolygonStipple", &self._opengl32_dll_handle),
            glGetProgramBinary: get_func("glGetProgramBinary", &self._opengl32_dll_handle),
            glGetProgramInfoLog: get_func("glGetProgramInfoLog", &self._opengl32_dll_handle),
            glGetProgramiv: get_func("glGetProgramiv", &self._opengl32_dll_handle),
            glGetQueryObjecti64v: get_func("glGetQueryObjecti64v", &self._opengl32_dll_handle),
            glGetQueryObjectiv: get_func("glGetQueryObjectiv", &self._opengl32_dll_handle),
            glGetQueryObjectui64v: get_func("glGetQueryObjectui64v", &self._opengl32_dll_handle),
            glGetQueryObjectuiv: get_func("glGetQueryObjectuiv", &self._opengl32_dll_handle),
            glGetQueryiv: get_func("glGetQueryiv", &self._opengl32_dll_handle),
            glGetRenderbufferParameteriv: get_func(
                "glGetRenderbufferParameteriv",
                &self._opengl32_dll_handle,
            ),
            glGetSamplerParameterIiv: get_func(
                "glGetSamplerParameterIiv",
                &self._opengl32_dll_handle,
            ),
            glGetSamplerParameterIuiv: get_func(
                "glGetSamplerParameterIuiv",
                &self._opengl32_dll_handle,
            ),
            glGetSamplerParameterfv: get_func("glGetSamplerParameterfv", &self._opengl32_dll_handle),
            glGetSamplerParameteriv: get_func("glGetSamplerParameteriv", &self._opengl32_dll_handle),
            glGetShaderInfoLog: get_func("glGetShaderInfoLog", &self._opengl32_dll_handle),
            glGetShaderSource: get_func("glGetShaderSource", &self._opengl32_dll_handle),
            glGetShaderiv: get_func("glGetShaderiv", &self._opengl32_dll_handle),
            glGetString: get_func("glGetString", &self._opengl32_dll_handle),
            glGetStringi: get_func("glGetStringi", &self._opengl32_dll_handle),
            glGetSynciv: get_func("glGetSynciv", &self._opengl32_dll_handle),
            glGetTexEnvfv: get_func("glGetTexEnvfv", &self._opengl32_dll_handle),
            glGetTexEnviv: get_func("glGetTexEnviv", &self._opengl32_dll_handle),
            glGetTexGendv: get_func("glGetTexGendv", &self._opengl32_dll_handle),
            glGetTexGenfv: get_func("glGetTexGenfv", &self._opengl32_dll_handle),
            glGetTexGeniv: get_func("glGetTexGeniv", &self._opengl32_dll_handle),
            glGetTexImage: get_func("glGetTexImage", &self._opengl32_dll_handle),
            glGetTexLevelParameterfv: get_func(
                "glGetTexLevelParameterfv",
                &self._opengl32_dll_handle,
            ),
            glGetTexLevelParameteriv: get_func(
                "glGetTexLevelParameteriv",
                &self._opengl32_dll_handle,
            ),
            glGetTexParameterIiv: get_func("glGetTexParameterIiv", &self._opengl32_dll_handle),
            glGetTexParameterIuiv: get_func("glGetTexParameterIuiv", &self._opengl32_dll_handle),
            glGetTexParameterPointervAPPLE: get_func(
                "glGetTexParameterPointervAPPLE",
                &self._opengl32_dll_handle,
            ),
            glGetTexParameterfv: get_func("glGetTexParameterfv", &self._opengl32_dll_handle),
            glGetTexParameteriv: get_func("glGetTexParameteriv", &self._opengl32_dll_handle),
            glGetTransformFeedbackVarying: get_func(
                "glGetTransformFeedbackVarying",
                &self._opengl32_dll_handle,
            ),
            glGetUniformBlockIndex: get_func("glGetUniformBlockIndex", &self._opengl32_dll_handle),
            glGetUniformIndices: get_func("glGetUniformIndices", &self._opengl32_dll_handle),
            glGetUniformLocation: get_func("glGetUniformLocation", &self._opengl32_dll_handle),
            glGetUniformfv: get_func("glGetUniformfv", &self._opengl32_dll_handle),
            glGetUniformiv: get_func("glGetUniformiv", &self._opengl32_dll_handle),
            glGetUniformuiv: get_func("glGetUniformuiv", &self._opengl32_dll_handle),
            glGetVertexAttribIiv: get_func("glGetVertexAttribIiv", &self._opengl32_dll_handle),
            glGetVertexAttribIuiv: get_func("glGetVertexAttribIuiv", &self._opengl32_dll_handle),
            glGetVertexAttribPointerv: get_func(
                "glGetVertexAttribPointerv",
                &self._opengl32_dll_handle,
            ),
            glGetVertexAttribdv: get_func("glGetVertexAttribdv", &self._opengl32_dll_handle),
            glGetVertexAttribfv: get_func("glGetVertexAttribfv", &self._opengl32_dll_handle),
            glGetVertexAttribiv: get_func("glGetVertexAttribiv", &self._opengl32_dll_handle),
            glHint: get_func("glHint", &self._opengl32_dll_handle),
            glIndexMask: get_func("glIndexMask", &self._opengl32_dll_handle),
            glIndexPointer: get_func("glIndexPointer", &self._opengl32_dll_handle),
            glIndexd: get_func("glIndexd", &self._opengl32_dll_handle),
            glIndexdv: get_func("glIndexdv", &self._opengl32_dll_handle),
            glIndexf: get_func("glIndexf", &self._opengl32_dll_handle),
            glIndexfv: get_func("glIndexfv", &self._opengl32_dll_handle),
            glIndexi: get_func("glIndexi", &self._opengl32_dll_handle),
            glIndexiv: get_func("glIndexiv", &self._opengl32_dll_handle),
            glIndexs: get_func("glIndexs", &self._opengl32_dll_handle),
            glIndexsv: get_func("glIndexsv", &self._opengl32_dll_handle),
            glIndexub: get_func("glIndexub", &self._opengl32_dll_handle),
            glIndexubv: get_func("glIndexubv", &self._opengl32_dll_handle),
            glInitNames: get_func("glInitNames", &self._opengl32_dll_handle),
            glInsertEventMarkerEXT: get_func("glInsertEventMarkerEXT", &self._opengl32_dll_handle),
            glInterleavedArrays: get_func("glInterleavedArrays", &self._opengl32_dll_handle),
            glInvalidateBufferData: get_func("glInvalidateBufferData", &self._opengl32_dll_handle),
            glInvalidateBufferSubData: get_func(
                "glInvalidateBufferSubData",
                &self._opengl32_dll_handle,
            ),
            glInvalidateFramebuffer: get_func("glInvalidateFramebuffer", &self._opengl32_dll_handle),
            glInvalidateSubFramebuffer: get_func(
                "glInvalidateSubFramebuffer",
                &self._opengl32_dll_handle,
            ),
            glInvalidateTexImage: get_func("glInvalidateTexImage", &self._opengl32_dll_handle),
            glInvalidateTexSubImage: get_func("glInvalidateTexSubImage", &self._opengl32_dll_handle),
            glIsBuffer: get_func("glIsBuffer", &self._opengl32_dll_handle),
            glIsEnabled: get_func("glIsEnabled", &self._opengl32_dll_handle),
            glIsEnabledi: get_func("glIsEnabledi", &self._opengl32_dll_handle),
            glIsFenceAPPLE: get_func("glIsFenceAPPLE", &self._opengl32_dll_handle),
            glIsFramebuffer: get_func("glIsFramebuffer", &self._opengl32_dll_handle),
            glIsList: get_func("glIsList", &self._opengl32_dll_handle),
            glIsProgram: get_func("glIsProgram", &self._opengl32_dll_handle),
            glIsQuery: get_func("glIsQuery", &self._opengl32_dll_handle),
            glIsRenderbuffer: get_func("glIsRenderbuffer", &self._opengl32_dll_handle),
            glIsSampler: get_func("glIsSampler", &self._opengl32_dll_handle),
            glIsShader: get_func("glIsShader", &self._opengl32_dll_handle),
            glIsSync: get_func("glIsSync", &self._opengl32_dll_handle),
            glIsTexture: get_func("glIsTexture", &self._opengl32_dll_handle),
            glIsVertexArray: get_func("glIsVertexArray", &self._opengl32_dll_handle),
            glIsVertexArrayAPPLE: get_func("glIsVertexArrayAPPLE", &self._opengl32_dll_handle),
            glLightModelf: get_func("glLightModelf", &self._opengl32_dll_handle),
            glLightModelfv: get_func("glLightModelfv", &self._opengl32_dll_handle),
            glLightModeli: get_func("glLightModeli", &self._opengl32_dll_handle),
            glLightModeliv: get_func("glLightModeliv", &self._opengl32_dll_handle),
            glLightf: get_func("glLightf", &self._opengl32_dll_handle),
            glLightfv: get_func("glLightfv", &self._opengl32_dll_handle),
            glLighti: get_func("glLighti", &self._opengl32_dll_handle),
            glLightiv: get_func("glLightiv", &self._opengl32_dll_handle),
            glLineStipple: get_func("glLineStipple", &self._opengl32_dll_handle),
            glLineWidth: get_func("glLineWidth", &self._opengl32_dll_handle),
            glLinkProgram: get_func("glLinkProgram", &self._opengl32_dll_handle),
            glListBase: get_func("glListBase", &self._opengl32_dll_handle),
            glLoadIdentity: get_func("glLoadIdentity", &self._opengl32_dll_handle),
            glLoadMatrixd: get_func("glLoadMatrixd", &self._opengl32_dll_handle),
            glLoadMatrixf: get_func("glLoadMatrixf", &self._opengl32_dll_handle),
            glLoadName: get_func("glLoadName", &self._opengl32_dll_handle),
            glLoadTransposeMatrixd: get_func("glLoadTransposeMatrixd", &self._opengl32_dll_handle),
            glLoadTransposeMatrixf: get_func("glLoadTransposeMatrixf", &self._opengl32_dll_handle),
            glLogicOp: get_func("glLogicOp", &self._opengl32_dll_handle),
            glMap1d: get_func("glMap1d", &self._opengl32_dll_handle),
            glMap1f: get_func("glMap1f", &self._opengl32_dll_handle),
            glMap2d: get_func("glMap2d", &self._opengl32_dll_handle),
            glMap2f: get_func("glMap2f", &self._opengl32_dll_handle),
            glMapBuffer: get_func("glMapBuffer", &self._opengl32_dll_handle),
            glMapBufferRange: get_func("glMapBufferRange", &self._opengl32_dll_handle),
            glMapGrid1d: get_func("glMapGrid1d", &self._opengl32_dll_handle),
            glMapGrid1f: get_func("glMapGrid1f", &self._opengl32_dll_handle),
            glMapGrid2d: get_func("glMapGrid2d", &self._opengl32_dll_handle),
            glMapGrid2f: get_func("glMapGrid2f", &self._opengl32_dll_handle),
            glMaterialf: get_func("glMaterialf", &self._opengl32_dll_handle),
            glMaterialfv: get_func("glMaterialfv", &self._opengl32_dll_handle),
            glMateriali: get_func("glMateriali", &self._opengl32_dll_handle),
            glMaterialiv: get_func("glMaterialiv", &self._opengl32_dll_handle),
            glMatrixMode: get_func("glMatrixMode", &self._opengl32_dll_handle),
            glMultMatrixd: get_func("glMultMatrixd", &self._opengl32_dll_handle),
            glMultMatrixf: get_func("glMultMatrixf", &self._opengl32_dll_handle),
            glMultTransposeMatrixd: get_func("glMultTransposeMatrixd", &self._opengl32_dll_handle),
            glMultTransposeMatrixf: get_func("glMultTransposeMatrixf", &self._opengl32_dll_handle),
            glMultiDrawArrays: get_func("glMultiDrawArrays", &self._opengl32_dll_handle),
            glMultiDrawElements: get_func("glMultiDrawElements", &self._opengl32_dll_handle),
            glMultiDrawElementsBaseVertex: get_func(
                "glMultiDrawElementsBaseVertex",
                &self._opengl32_dll_handle,
            ),
            glMultiTexCoord1d: get_func("glMultiTexCoord1d", &self._opengl32_dll_handle),
            glMultiTexCoord1dv: get_func("glMultiTexCoord1dv", &self._opengl32_dll_handle),
            glMultiTexCoord1f: get_func("glMultiTexCoord1f", &self._opengl32_dll_handle),
            glMultiTexCoord1fv: get_func("glMultiTexCoord1fv", &self._opengl32_dll_handle),
            glMultiTexCoord1i: get_func("glMultiTexCoord1i", &self._opengl32_dll_handle),
            glMultiTexCoord1iv: get_func("glMultiTexCoord1iv", &self._opengl32_dll_handle),
            glMultiTexCoord1s: get_func("glMultiTexCoord1s", &self._opengl32_dll_handle),
            glMultiTexCoord1sv: get_func("glMultiTexCoord1sv", &self._opengl32_dll_handle),
            glMultiTexCoord2d: get_func("glMultiTexCoord2d", &self._opengl32_dll_handle),
            glMultiTexCoord2dv: get_func("glMultiTexCoord2dv", &self._opengl32_dll_handle),
            glMultiTexCoord2f: get_func("glMultiTexCoord2f", &self._opengl32_dll_handle),
            glMultiTexCoord2fv: get_func("glMultiTexCoord2fv", &self._opengl32_dll_handle),
            glMultiTexCoord2i: get_func("glMultiTexCoord2i", &self._opengl32_dll_handle),
            glMultiTexCoord2iv: get_func("glMultiTexCoord2iv", &self._opengl32_dll_handle),
            glMultiTexCoord2s: get_func("glMultiTexCoord2s", &self._opengl32_dll_handle),
            glMultiTexCoord2sv: get_func("glMultiTexCoord2sv", &self._opengl32_dll_handle),
            glMultiTexCoord3d: get_func("glMultiTexCoord3d", &self._opengl32_dll_handle),
            glMultiTexCoord3dv: get_func("glMultiTexCoord3dv", &self._opengl32_dll_handle),
            glMultiTexCoord3f: get_func("glMultiTexCoord3f", &self._opengl32_dll_handle),
            glMultiTexCoord3fv: get_func("glMultiTexCoord3fv", &self._opengl32_dll_handle),
            glMultiTexCoord3i: get_func("glMultiTexCoord3i", &self._opengl32_dll_handle),
            glMultiTexCoord3iv: get_func("glMultiTexCoord3iv", &self._opengl32_dll_handle),
            glMultiTexCoord3s: get_func("glMultiTexCoord3s", &self._opengl32_dll_handle),
            glMultiTexCoord3sv: get_func("glMultiTexCoord3sv", &self._opengl32_dll_handle),
            glMultiTexCoord4d: get_func("glMultiTexCoord4d", &self._opengl32_dll_handle),
            glMultiTexCoord4dv: get_func("glMultiTexCoord4dv", &self._opengl32_dll_handle),
            glMultiTexCoord4f: get_func("glMultiTexCoord4f", &self._opengl32_dll_handle),
            glMultiTexCoord4fv: get_func("glMultiTexCoord4fv", &self._opengl32_dll_handle),
            glMultiTexCoord4i: get_func("glMultiTexCoord4i", &self._opengl32_dll_handle),
            glMultiTexCoord4iv: get_func("glMultiTexCoord4iv", &self._opengl32_dll_handle),
            glMultiTexCoord4s: get_func("glMultiTexCoord4s", &self._opengl32_dll_handle),
            glMultiTexCoord4sv: get_func("glMultiTexCoord4sv", &self._opengl32_dll_handle),
            glMultiTexCoordP1ui: get_func("glMultiTexCoordP1ui", &self._opengl32_dll_handle),
            glMultiTexCoordP1uiv: get_func("glMultiTexCoordP1uiv", &self._opengl32_dll_handle),
            glMultiTexCoordP2ui: get_func("glMultiTexCoordP2ui", &self._opengl32_dll_handle),
            glMultiTexCoordP2uiv: get_func("glMultiTexCoordP2uiv", &self._opengl32_dll_handle),
            glMultiTexCoordP3ui: get_func("glMultiTexCoordP3ui", &self._opengl32_dll_handle),
            glMultiTexCoordP3uiv: get_func("glMultiTexCoordP3uiv", &self._opengl32_dll_handle),
            glMultiTexCoordP4ui: get_func("glMultiTexCoordP4ui", &self._opengl32_dll_handle),
            glMultiTexCoordP4uiv: get_func("glMultiTexCoordP4uiv", &self._opengl32_dll_handle),
            glNewList: get_func("glNewList", &self._opengl32_dll_handle),
            glNormal3b: get_func("glNormal3b", &self._opengl32_dll_handle),
            glNormal3bv: get_func("glNormal3bv", &self._opengl32_dll_handle),
            glNormal3d: get_func("glNormal3d", &self._opengl32_dll_handle),
            glNormal3dv: get_func("glNormal3dv", &self._opengl32_dll_handle),
            glNormal3f: get_func("glNormal3f", &self._opengl32_dll_handle),
            glNormal3fv: get_func("glNormal3fv", &self._opengl32_dll_handle),
            glNormal3i: get_func("glNormal3i", &self._opengl32_dll_handle),
            glNormal3iv: get_func("glNormal3iv", &self._opengl32_dll_handle),
            glNormal3s: get_func("glNormal3s", &self._opengl32_dll_handle),
            glNormal3sv: get_func("glNormal3sv", &self._opengl32_dll_handle),
            glNormalP3ui: get_func("glNormalP3ui", &self._opengl32_dll_handle),
            glNormalP3uiv: get_func("glNormalP3uiv", &self._opengl32_dll_handle),
            glNormalPointer: get_func("glNormalPointer", &self._opengl32_dll_handle),
            glObjectLabel: get_func("glObjectLabel", &self._opengl32_dll_handle),
            glObjectLabelKHR: get_func("glObjectLabelKHR", &self._opengl32_dll_handle),
            glObjectPtrLabel: get_func("glObjectPtrLabel", &self._opengl32_dll_handle),
            glObjectPtrLabelKHR: get_func("glObjectPtrLabelKHR", &self._opengl32_dll_handle),
            glOrtho: get_func("glOrtho", &self._opengl32_dll_handle),
            glPassThrough: get_func("glPassThrough", &self._opengl32_dll_handle),
            glPixelMapfv: get_func("glPixelMapfv", &self._opengl32_dll_handle),
            glPixelMapuiv: get_func("glPixelMapuiv", &self._opengl32_dll_handle),
            glPixelMapusv: get_func("glPixelMapusv", &self._opengl32_dll_handle),
            glPixelStoref: get_func("glPixelStoref", &self._opengl32_dll_handle),
            glPixelStorei: get_func("glPixelStorei", &self._opengl32_dll_handle),
            glPixelTransferf: get_func("glPixelTransferf", &self._opengl32_dll_handle),
            glPixelTransferi: get_func("glPixelTransferi", &self._opengl32_dll_handle),
            glPixelZoom: get_func("glPixelZoom", &self._opengl32_dll_handle),
            glPointParameterf: get_func("glPointParameterf", &self._opengl32_dll_handle),
            glPointParameterfv: get_func("glPointParameterfv", &self._opengl32_dll_handle),
            glPointParameteri: get_func("glPointParameteri", &self._opengl32_dll_handle),
            glPointParameteriv: get_func("glPointParameteriv", &self._opengl32_dll_handle),
            glPointSize: get_func("glPointSize", &self._opengl32_dll_handle),
            glPolygonMode: get_func("glPolygonMode", &self._opengl32_dll_handle),
            glPolygonOffset: get_func("glPolygonOffset", &self._opengl32_dll_handle),
            glPolygonStipple: get_func("glPolygonStipple", &self._opengl32_dll_handle),
            glPopAttrib: get_func("glPopAttrib", &self._opengl32_dll_handle),
            glPopClientAttrib: get_func("glPopClientAttrib", &self._opengl32_dll_handle),
            glPopDebugGroup: get_func("glPopDebugGroup", &self._opengl32_dll_handle),
            glPopDebugGroupKHR: get_func("glPopDebugGroupKHR", &self._opengl32_dll_handle),
            glPopGroupMarkerEXT: get_func("glPopGroupMarkerEXT", &self._opengl32_dll_handle),
            glPopMatrix: get_func("glPopMatrix", &self._opengl32_dll_handle),
            glPopName: get_func("glPopName", &self._opengl32_dll_handle),
            glPrimitiveRestartIndex: get_func("glPrimitiveRestartIndex", &self._opengl32_dll_handle),
            glPrioritizeTextures: get_func("glPrioritizeTextures", &self._opengl32_dll_handle),
            glProgramBinary: get_func("glProgramBinary", &self._opengl32_dll_handle),
            glProgramParameteri: get_func("glProgramParameteri", &self._opengl32_dll_handle),
            glProvokingVertex: get_func("glProvokingVertex", &self._opengl32_dll_handle),
            glPushAttrib: get_func("glPushAttrib", &self._opengl32_dll_handle),
            glPushClientAttrib: get_func("glPushClientAttrib", &self._opengl32_dll_handle),
            glPushDebugGroup: get_func("glPushDebugGroup", &self._opengl32_dll_handle),
            glPushDebugGroupKHR: get_func("glPushDebugGroupKHR", &self._opengl32_dll_handle),
            glPushGroupMarkerEXT: get_func("glPushGroupMarkerEXT", &self._opengl32_dll_handle),
            glPushMatrix: get_func("glPushMatrix", &self._opengl32_dll_handle),
            glPushName: get_func("glPushName", &self._opengl32_dll_handle),
            glQueryCounter: get_func("glQueryCounter", &self._opengl32_dll_handle),
            glRasterPos2d: get_func("glRasterPos2d", &self._opengl32_dll_handle),
            glRasterPos2dv: get_func("glRasterPos2dv", &self._opengl32_dll_handle),
            glRasterPos2f: get_func("glRasterPos2f", &self._opengl32_dll_handle),
            glRasterPos2fv: get_func("glRasterPos2fv", &self._opengl32_dll_handle),
            glRasterPos2i: get_func("glRasterPos2i", &self._opengl32_dll_handle),
            glRasterPos2iv: get_func("glRasterPos2iv", &self._opengl32_dll_handle),
            glRasterPos2s: get_func("glRasterPos2s", &self._opengl32_dll_handle),
            glRasterPos2sv: get_func("glRasterPos2sv", &self._opengl32_dll_handle),
            glRasterPos3d: get_func("glRasterPos3d", &self._opengl32_dll_handle),
            glRasterPos3dv: get_func("glRasterPos3dv", &self._opengl32_dll_handle),
            glRasterPos3f: get_func("glRasterPos3f", &self._opengl32_dll_handle),
            glRasterPos3fv: get_func("glRasterPos3fv", &self._opengl32_dll_handle),
            glRasterPos3i: get_func("glRasterPos3i", &self._opengl32_dll_handle),
            glRasterPos3iv: get_func("glRasterPos3iv", &self._opengl32_dll_handle),
            glRasterPos3s: get_func("glRasterPos3s", &self._opengl32_dll_handle),
            glRasterPos3sv: get_func("glRasterPos3sv", &self._opengl32_dll_handle),
            glRasterPos4d: get_func("glRasterPos4d", &self._opengl32_dll_handle),
            glRasterPos4dv: get_func("glRasterPos4dv", &self._opengl32_dll_handle),
            glRasterPos4f: get_func("glRasterPos4f", &self._opengl32_dll_handle),
            glRasterPos4fv: get_func("glRasterPos4fv", &self._opengl32_dll_handle),
            glRasterPos4i: get_func("glRasterPos4i", &self._opengl32_dll_handle),
            glRasterPos4iv: get_func("glRasterPos4iv", &self._opengl32_dll_handle),
            glRasterPos4s: get_func("glRasterPos4s", &self._opengl32_dll_handle),
            glRasterPos4sv: get_func("glRasterPos4sv", &self._opengl32_dll_handle),
            glReadBuffer: get_func("glReadBuffer", &self._opengl32_dll_handle),
            glReadPixels: get_func("glReadPixels", &self._opengl32_dll_handle),
            glRectd: get_func("glRectd", &self._opengl32_dll_handle),
            glRectdv: get_func("glRectdv", &self._opengl32_dll_handle),
            glRectf: get_func("glRectf", &self._opengl32_dll_handle),
            glRectfv: get_func("glRectfv", &self._opengl32_dll_handle),
            glRecti: get_func("glRecti", &self._opengl32_dll_handle),
            glRectiv: get_func("glRectiv", &self._opengl32_dll_handle),
            glRects: get_func("glRects", &self._opengl32_dll_handle),
            glRectsv: get_func("glRectsv", &self._opengl32_dll_handle),
            glRenderMode: get_func("glRenderMode", &self._opengl32_dll_handle),
            glRenderbufferStorage: get_func("glRenderbufferStorage", &self._opengl32_dll_handle),
            glRenderbufferStorageMultisample: get_func(
                "glRenderbufferStorageMultisample",
                &self._opengl32_dll_handle,
            ),
            glRotated: get_func("glRotated", &self._opengl32_dll_handle),
            glRotatef: get_func("glRotatef", &self._opengl32_dll_handle),
            glSampleCoverage: get_func("glSampleCoverage", &self._opengl32_dll_handle),
            glSampleMaski: get_func("glSampleMaski", &self._opengl32_dll_handle),
            glSamplerParameterIiv: get_func("glSamplerParameterIiv", &self._opengl32_dll_handle),
            glSamplerParameterIuiv: get_func("glSamplerParameterIuiv", &self._opengl32_dll_handle),
            glSamplerParameterf: get_func("glSamplerParameterf", &self._opengl32_dll_handle),
            glSamplerParameterfv: get_func("glSamplerParameterfv", &self._opengl32_dll_handle),
            glSamplerParameteri: get_func("glSamplerParameteri", &self._opengl32_dll_handle),
            glSamplerParameteriv: get_func("glSamplerParameteriv", &self._opengl32_dll_handle),
            glScaled: get_func("glScaled", &self._opengl32_dll_handle),
            glScalef: get_func("glScalef", &self._opengl32_dll_handle),
            glScissor: get_func("glScissor", &self._opengl32_dll_handle),
            glSecondaryColor3b: get_func("glSecondaryColor3b", &self._opengl32_dll_handle),
            glSecondaryColor3bv: get_func("glSecondaryColor3bv", &self._opengl32_dll_handle),
            glSecondaryColor3d: get_func("glSecondaryColor3d", &self._opengl32_dll_handle),
            glSecondaryColor3dv: get_func("glSecondaryColor3dv", &self._opengl32_dll_handle),
            glSecondaryColor3f: get_func("glSecondaryColor3f", &self._opengl32_dll_handle),
            glSecondaryColor3fv: get_func("glSecondaryColor3fv", &self._opengl32_dll_handle),
            glSecondaryColor3i: get_func("glSecondaryColor3i", &self._opengl32_dll_handle),
            glSecondaryColor3iv: get_func("glSecondaryColor3iv", &self._opengl32_dll_handle),
            glSecondaryColor3s: get_func("glSecondaryColor3s", &self._opengl32_dll_handle),
            glSecondaryColor3sv: get_func("glSecondaryColor3sv", &self._opengl32_dll_handle),
            glSecondaryColor3ub: get_func("glSecondaryColor3ub", &self._opengl32_dll_handle),
            glSecondaryColor3ubv: get_func("glSecondaryColor3ubv", &self._opengl32_dll_handle),
            glSecondaryColor3ui: get_func("glSecondaryColor3ui", &self._opengl32_dll_handle),
            glSecondaryColor3uiv: get_func("glSecondaryColor3uiv", &self._opengl32_dll_handle),
            glSecondaryColor3us: get_func("glSecondaryColor3us", &self._opengl32_dll_handle),
            glSecondaryColor3usv: get_func("glSecondaryColor3usv", &self._opengl32_dll_handle),
            glSecondaryColorP3ui: get_func("glSecondaryColorP3ui", &self._opengl32_dll_handle),
            glSecondaryColorP3uiv: get_func("glSecondaryColorP3uiv", &self._opengl32_dll_handle),
            glSecondaryColorPointer: get_func("glSecondaryColorPointer", &self._opengl32_dll_handle),
            glSelectBuffer: get_func("glSelectBuffer", &self._opengl32_dll_handle),
            glSetFenceAPPLE: get_func("glSetFenceAPPLE", &self._opengl32_dll_handle),
            glShadeModel: get_func("glShadeModel", &self._opengl32_dll_handle),
            glShaderSource: get_func("glShaderSource", &self._opengl32_dll_handle),
            glShaderStorageBlockBinding: get_func(
                "glShaderStorageBlockBinding",
                &self._opengl32_dll_handle,
            ),
            glStencilFunc: get_func("glStencilFunc", &self._opengl32_dll_handle),
            glStencilFuncSeparate: get_func("glStencilFuncSeparate", &self._opengl32_dll_handle),
            glStencilMask: get_func("glStencilMask", &self._opengl32_dll_handle),
            glStencilMaskSeparate: get_func("glStencilMaskSeparate", &self._opengl32_dll_handle),
            glStencilOp: get_func("glStencilOp", &self._opengl32_dll_handle),
            glStencilOpSeparate: get_func("glStencilOpSeparate", &self._opengl32_dll_handle),
            glTestFenceAPPLE: get_func("glTestFenceAPPLE", &self._opengl32_dll_handle),
            glTestObjectAPPLE: get_func("glTestObjectAPPLE", &self._opengl32_dll_handle),
            glTexBuffer: get_func("glTexBuffer", &self._opengl32_dll_handle),
            glTexCoord1d: get_func("glTexCoord1d", &self._opengl32_dll_handle),
            glTexCoord1dv: get_func("glTexCoord1dv", &self._opengl32_dll_handle),
            glTexCoord1f: get_func("glTexCoord1f", &self._opengl32_dll_handle),
            glTexCoord1fv: get_func("glTexCoord1fv", &self._opengl32_dll_handle),
            glTexCoord1i: get_func("glTexCoord1i", &self._opengl32_dll_handle),
            glTexCoord1iv: get_func("glTexCoord1iv", &self._opengl32_dll_handle),
            glTexCoord1s: get_func("glTexCoord1s", &self._opengl32_dll_handle),
            glTexCoord1sv: get_func("glTexCoord1sv", &self._opengl32_dll_handle),
            glTexCoord2d: get_func("glTexCoord2d", &self._opengl32_dll_handle),
            glTexCoord2dv: get_func("glTexCoord2dv", &self._opengl32_dll_handle),
            glTexCoord2f: get_func("glTexCoord2f", &self._opengl32_dll_handle),
            glTexCoord2fv: get_func("glTexCoord2fv", &self._opengl32_dll_handle),
            glTexCoord2i: get_func("glTexCoord2i", &self._opengl32_dll_handle),
            glTexCoord2iv: get_func("glTexCoord2iv", &self._opengl32_dll_handle),
            glTexCoord2s: get_func("glTexCoord2s", &self._opengl32_dll_handle),
            glTexCoord2sv: get_func("glTexCoord2sv", &self._opengl32_dll_handle),
            glTexCoord3d: get_func("glTexCoord3d", &self._opengl32_dll_handle),
            glTexCoord3dv: get_func("glTexCoord3dv", &self._opengl32_dll_handle),
            glTexCoord3f: get_func("glTexCoord3f", &self._opengl32_dll_handle),
            glTexCoord3fv: get_func("glTexCoord3fv", &self._opengl32_dll_handle),
            glTexCoord3i: get_func("glTexCoord3i", &self._opengl32_dll_handle),
            glTexCoord3iv: get_func("glTexCoord3iv", &self._opengl32_dll_handle),
            glTexCoord3s: get_func("glTexCoord3s", &self._opengl32_dll_handle),
            glTexCoord3sv: get_func("glTexCoord3sv", &self._opengl32_dll_handle),
            glTexCoord4d: get_func("glTexCoord4d", &self._opengl32_dll_handle),
            glTexCoord4dv: get_func("glTexCoord4dv", &self._opengl32_dll_handle),
            glTexCoord4f: get_func("glTexCoord4f", &self._opengl32_dll_handle),
            glTexCoord4fv: get_func("glTexCoord4fv", &self._opengl32_dll_handle),
            glTexCoord4i: get_func("glTexCoord4i", &self._opengl32_dll_handle),
            glTexCoord4iv: get_func("glTexCoord4iv", &self._opengl32_dll_handle),
            glTexCoord4s: get_func("glTexCoord4s", &self._opengl32_dll_handle),
            glTexCoord4sv: get_func("glTexCoord4sv", &self._opengl32_dll_handle),
            glTexCoordP1ui: get_func("glTexCoordP1ui", &self._opengl32_dll_handle),
            glTexCoordP1uiv: get_func("glTexCoordP1uiv", &self._opengl32_dll_handle),
            glTexCoordP2ui: get_func("glTexCoordP2ui", &self._opengl32_dll_handle),
            glTexCoordP2uiv: get_func("glTexCoordP2uiv", &self._opengl32_dll_handle),
            glTexCoordP3ui: get_func("glTexCoordP3ui", &self._opengl32_dll_handle),
            glTexCoordP3uiv: get_func("glTexCoordP3uiv", &self._opengl32_dll_handle),
            glTexCoordP4ui: get_func("glTexCoordP4ui", &self._opengl32_dll_handle),
            glTexCoordP4uiv: get_func("glTexCoordP4uiv", &self._opengl32_dll_handle),
            glTexCoordPointer: get_func("glTexCoordPointer", &self._opengl32_dll_handle),
            glTexEnvf: get_func("glTexEnvf", &self._opengl32_dll_handle),
            glTexEnvfv: get_func("glTexEnvfv", &self._opengl32_dll_handle),
            glTexEnvi: get_func("glTexEnvi", &self._opengl32_dll_handle),
            glTexEnviv: get_func("glTexEnviv", &self._opengl32_dll_handle),
            glTexGend: get_func("glTexGend", &self._opengl32_dll_handle),
            glTexGendv: get_func("glTexGendv", &self._opengl32_dll_handle),
            glTexGenf: get_func("glTexGenf", &self._opengl32_dll_handle),
            glTexGenfv: get_func("glTexGenfv", &self._opengl32_dll_handle),
            glTexGeni: get_func("glTexGeni", &self._opengl32_dll_handle),
            glTexGeniv: get_func("glTexGeniv", &self._opengl32_dll_handle),
            glTexImage1D: get_func("glTexImage1D", &self._opengl32_dll_handle),
            glTexImage2D: get_func("glTexImage2D", &self._opengl32_dll_handle),
            glTexImage2DMultisample: get_func("glTexImage2DMultisample", &self._opengl32_dll_handle),
            glTexImage3D: get_func("glTexImage3D", &self._opengl32_dll_handle),
            glTexImage3DMultisample: get_func("glTexImage3DMultisample", &self._opengl32_dll_handle),
            glTexParameterIiv: get_func("glTexParameterIiv", &self._opengl32_dll_handle),
            glTexParameterIuiv: get_func("glTexParameterIuiv", &self._opengl32_dll_handle),
            glTexParameterf: get_func("glTexParameterf", &self._opengl32_dll_handle),
            glTexParameterfv: get_func("glTexParameterfv", &self._opengl32_dll_handle),
            glTexParameteri: get_func("glTexParameteri", &self._opengl32_dll_handle),
            glTexParameteriv: get_func("glTexParameteriv", &self._opengl32_dll_handle),
            glTexStorage1D: get_func("glTexStorage1D", &self._opengl32_dll_handle),
            glTexStorage2D: get_func("glTexStorage2D", &self._opengl32_dll_handle),
            glTexStorage3D: get_func("glTexStorage3D", &self._opengl32_dll_handle),
            glTexSubImage1D: get_func("glTexSubImage1D", &self._opengl32_dll_handle),
            glTexSubImage2D: get_func("glTexSubImage2D", &self._opengl32_dll_handle),
            glTexSubImage3D: get_func("glTexSubImage3D", &self._opengl32_dll_handle),
            glTextureRangeAPPLE: get_func("glTextureRangeAPPLE", &self._opengl32_dll_handle),
            glTransformFeedbackVaryings: get_func(
                "glTransformFeedbackVaryings",
                &self._opengl32_dll_handle,
            ),
            glTranslated: get_func("glTranslated", &self._opengl32_dll_handle),
            glTranslatef: get_func("glTranslatef", &self._opengl32_dll_handle),
            glUniform1f: get_func("glUniform1f", &self._opengl32_dll_handle),
            glUniform1fv: get_func("glUniform1fv", &self._opengl32_dll_handle),
            glUniform1i: get_func("glUniform1i", &self._opengl32_dll_handle),
            glUniform1iv: get_func("glUniform1iv", &self._opengl32_dll_handle),
            glUniform1ui: get_func("glUniform1ui", &self._opengl32_dll_handle),
            glUniform1uiv: get_func("glUniform1uiv", &self._opengl32_dll_handle),
            glUniform2f: get_func("glUniform2f", &self._opengl32_dll_handle),
            glUniform2fv: get_func("glUniform2fv", &self._opengl32_dll_handle),
            glUniform2i: get_func("glUniform2i", &self._opengl32_dll_handle),
            glUniform2iv: get_func("glUniform2iv", &self._opengl32_dll_handle),
            glUniform2ui: get_func("glUniform2ui", &self._opengl32_dll_handle),
            glUniform2uiv: get_func("glUniform2uiv", &self._opengl32_dll_handle),
            glUniform3f: get_func("glUniform3f", &self._opengl32_dll_handle),
            glUniform3fv: get_func("glUniform3fv", &self._opengl32_dll_handle),
            glUniform3i: get_func("glUniform3i", &self._opengl32_dll_handle),
            glUniform3iv: get_func("glUniform3iv", &self._opengl32_dll_handle),
            glUniform3ui: get_func("glUniform3ui", &self._opengl32_dll_handle),
            glUniform3uiv: get_func("glUniform3uiv", &self._opengl32_dll_handle),
            glUniform4f: get_func("glUniform4f", &self._opengl32_dll_handle),
            glUniform4fv: get_func("glUniform4fv", &self._opengl32_dll_handle),
            glUniform4i: get_func("glUniform4i", &self._opengl32_dll_handle),
            glUniform4iv: get_func("glUniform4iv", &self._opengl32_dll_handle),
            glUniform4ui: get_func("glUniform4ui", &self._opengl32_dll_handle),
            glUniform4uiv: get_func("glUniform4uiv", &self._opengl32_dll_handle),
            glUniformBlockBinding: get_func("glUniformBlockBinding", &self._opengl32_dll_handle),
            glUniformMatrix2fv: get_func("glUniformMatrix2fv", &self._opengl32_dll_handle),
            glUniformMatrix2x3fv: get_func("glUniformMatrix2x3fv", &self._opengl32_dll_handle),
            glUniformMatrix2x4fv: get_func("glUniformMatrix2x4fv", &self._opengl32_dll_handle),
            glUniformMatrix3fv: get_func("glUniformMatrix3fv", &self._opengl32_dll_handle),
            glUniformMatrix3x2fv: get_func("glUniformMatrix3x2fv", &self._opengl32_dll_handle),
            glUniformMatrix3x4fv: get_func("glUniformMatrix3x4fv", &self._opengl32_dll_handle),
            glUniformMatrix4fv: get_func("glUniformMatrix4fv", &self._opengl32_dll_handle),
            glUniformMatrix4x2fv: get_func("glUniformMatrix4x2fv", &self._opengl32_dll_handle),
            glUniformMatrix4x3fv: get_func("glUniformMatrix4x3fv", &self._opengl32_dll_handle),
            glUnmapBuffer: get_func("glUnmapBuffer", &self._opengl32_dll_handle),
            glUseProgram: get_func("glUseProgram", &self._opengl32_dll_handle),
            glValidateProgram: get_func("glValidateProgram", &self._opengl32_dll_handle),
            glVertex2d: get_func("glVertex2d", &self._opengl32_dll_handle),
            glVertex2dv: get_func("glVertex2dv", &self._opengl32_dll_handle),
            glVertex2f: get_func("glVertex2f", &self._opengl32_dll_handle),
            glVertex2fv: get_func("glVertex2fv", &self._opengl32_dll_handle),
            glVertex2i: get_func("glVertex2i", &self._opengl32_dll_handle),
            glVertex2iv: get_func("glVertex2iv", &self._opengl32_dll_handle),
            glVertex2s: get_func("glVertex2s", &self._opengl32_dll_handle),
            glVertex2sv: get_func("glVertex2sv", &self._opengl32_dll_handle),
            glVertex3d: get_func("glVertex3d", &self._opengl32_dll_handle),
            glVertex3dv: get_func("glVertex3dv", &self._opengl32_dll_handle),
            glVertex3f: get_func("glVertex3f", &self._opengl32_dll_handle),
            glVertex3fv: get_func("glVertex3fv", &self._opengl32_dll_handle),
            glVertex3i: get_func("glVertex3i", &self._opengl32_dll_handle),
            glVertex3iv: get_func("glVertex3iv", &self._opengl32_dll_handle),
            glVertex3s: get_func("glVertex3s", &self._opengl32_dll_handle),
            glVertex3sv: get_func("glVertex3sv", &self._opengl32_dll_handle),
            glVertex4d: get_func("glVertex4d", &self._opengl32_dll_handle),
            glVertex4dv: get_func("glVertex4dv", &self._opengl32_dll_handle),
            glVertex4f: get_func("glVertex4f", &self._opengl32_dll_handle),
            glVertex4fv: get_func("glVertex4fv", &self._opengl32_dll_handle),
            glVertex4i: get_func("glVertex4i", &self._opengl32_dll_handle),
            glVertex4iv: get_func("glVertex4iv", &self._opengl32_dll_handle),
            glVertex4s: get_func("glVertex4s", &self._opengl32_dll_handle),
            glVertex4sv: get_func("glVertex4sv", &self._opengl32_dll_handle),
            glVertexAttrib1d: get_func("glVertexAttrib1d", &self._opengl32_dll_handle),
            glVertexAttrib1dv: get_func("glVertexAttrib1dv", &self._opengl32_dll_handle),
            glVertexAttrib1f: get_func("glVertexAttrib1f", &self._opengl32_dll_handle),
            glVertexAttrib1fv: get_func("glVertexAttrib1fv", &self._opengl32_dll_handle),
            glVertexAttrib1s: get_func("glVertexAttrib1s", &self._opengl32_dll_handle),
            glVertexAttrib1sv: get_func("glVertexAttrib1sv", &self._opengl32_dll_handle),
            glVertexAttrib2d: get_func("glVertexAttrib2d", &self._opengl32_dll_handle),
            glVertexAttrib2dv: get_func("glVertexAttrib2dv", &self._opengl32_dll_handle),
            glVertexAttrib2f: get_func("glVertexAttrib2f", &self._opengl32_dll_handle),
            glVertexAttrib2fv: get_func("glVertexAttrib2fv", &self._opengl32_dll_handle),
            glVertexAttrib2s: get_func("glVertexAttrib2s", &self._opengl32_dll_handle),
            glVertexAttrib2sv: get_func("glVertexAttrib2sv", &self._opengl32_dll_handle),
            glVertexAttrib3d: get_func("glVertexAttrib3d", &self._opengl32_dll_handle),
            glVertexAttrib3dv: get_func("glVertexAttrib3dv", &self._opengl32_dll_handle),
            glVertexAttrib3f: get_func("glVertexAttrib3f", &self._opengl32_dll_handle),
            glVertexAttrib3fv: get_func("glVertexAttrib3fv", &self._opengl32_dll_handle),
            glVertexAttrib3s: get_func("glVertexAttrib3s", &self._opengl32_dll_handle),
            glVertexAttrib3sv: get_func("glVertexAttrib3sv", &self._opengl32_dll_handle),
            glVertexAttrib4Nbv: get_func("glVertexAttrib4Nbv", &self._opengl32_dll_handle),
            glVertexAttrib4Niv: get_func("glVertexAttrib4Niv", &self._opengl32_dll_handle),
            glVertexAttrib4Nsv: get_func("glVertexAttrib4Nsv", &self._opengl32_dll_handle),
            glVertexAttrib4Nub: get_func("glVertexAttrib4Nub", &self._opengl32_dll_handle),
            glVertexAttrib4Nubv: get_func("glVertexAttrib4Nubv", &self._opengl32_dll_handle),
            glVertexAttrib4Nuiv: get_func("glVertexAttrib4Nuiv", &self._opengl32_dll_handle),
            glVertexAttrib4Nusv: get_func("glVertexAttrib4Nusv", &self._opengl32_dll_handle),
            glVertexAttrib4bv: get_func("glVertexAttrib4bv", &self._opengl32_dll_handle),
            glVertexAttrib4d: get_func("glVertexAttrib4d", &self._opengl32_dll_handle),
            glVertexAttrib4dv: get_func("glVertexAttrib4dv", &self._opengl32_dll_handle),
            glVertexAttrib4f: get_func("glVertexAttrib4f", &self._opengl32_dll_handle),
            glVertexAttrib4fv: get_func("glVertexAttrib4fv", &self._opengl32_dll_handle),
            glVertexAttrib4iv: get_func("glVertexAttrib4iv", &self._opengl32_dll_handle),
            glVertexAttrib4s: get_func("glVertexAttrib4s", &self._opengl32_dll_handle),
            glVertexAttrib4sv: get_func("glVertexAttrib4sv", &self._opengl32_dll_handle),
            glVertexAttrib4ubv: get_func("glVertexAttrib4ubv", &self._opengl32_dll_handle),
            glVertexAttrib4uiv: get_func("glVertexAttrib4uiv", &self._opengl32_dll_handle),
            glVertexAttrib4usv: get_func("glVertexAttrib4usv", &self._opengl32_dll_handle),
            glVertexAttribDivisor: get_func("glVertexAttribDivisor", &self._opengl32_dll_handle),
            glVertexAttribI1i: get_func("glVertexAttribI1i", &self._opengl32_dll_handle),
            glVertexAttribI1iv: get_func("glVertexAttribI1iv", &self._opengl32_dll_handle),
            glVertexAttribI1ui: get_func("glVertexAttribI1ui", &self._opengl32_dll_handle),
            glVertexAttribI1uiv: get_func("glVertexAttribI1uiv", &self._opengl32_dll_handle),
            glVertexAttribI2i: get_func("glVertexAttribI2i", &self._opengl32_dll_handle),
            glVertexAttribI2iv: get_func("glVertexAttribI2iv", &self._opengl32_dll_handle),
            glVertexAttribI2ui: get_func("glVertexAttribI2ui", &self._opengl32_dll_handle),
            glVertexAttribI2uiv: get_func("glVertexAttribI2uiv", &self._opengl32_dll_handle),
            glVertexAttribI3i: get_func("glVertexAttribI3i", &self._opengl32_dll_handle),
            glVertexAttribI3iv: get_func("glVertexAttribI3iv", &self._opengl32_dll_handle),
            glVertexAttribI3ui: get_func("glVertexAttribI3ui", &self._opengl32_dll_handle),
            glVertexAttribI3uiv: get_func("glVertexAttribI3uiv", &self._opengl32_dll_handle),
            glVertexAttribI4bv: get_func("glVertexAttribI4bv", &self._opengl32_dll_handle),
            glVertexAttribI4i: get_func("glVertexAttribI4i", &self._opengl32_dll_handle),
            glVertexAttribI4iv: get_func("glVertexAttribI4iv", &self._opengl32_dll_handle),
            glVertexAttribI4sv: get_func("glVertexAttribI4sv", &self._opengl32_dll_handle),
            glVertexAttribI4ubv: get_func("glVertexAttribI4ubv", &self._opengl32_dll_handle),
            glVertexAttribI4ui: get_func("glVertexAttribI4ui", &self._opengl32_dll_handle),
            glVertexAttribI4uiv: get_func("glVertexAttribI4uiv", &self._opengl32_dll_handle),
            glVertexAttribI4usv: get_func("glVertexAttribI4usv", &self._opengl32_dll_handle),
            glVertexAttribIPointer: get_func("glVertexAttribIPointer", &self._opengl32_dll_handle),
            glVertexAttribP1ui: get_func("glVertexAttribP1ui", &self._opengl32_dll_handle),
            glVertexAttribP1uiv: get_func("glVertexAttribP1uiv", &self._opengl32_dll_handle),
            glVertexAttribP2ui: get_func("glVertexAttribP2ui", &self._opengl32_dll_handle),
            glVertexAttribP2uiv: get_func("glVertexAttribP2uiv", &self._opengl32_dll_handle),
            glVertexAttribP3ui: get_func("glVertexAttribP3ui", &self._opengl32_dll_handle),
            glVertexAttribP3uiv: get_func("glVertexAttribP3uiv", &self._opengl32_dll_handle),
            glVertexAttribP4ui: get_func("glVertexAttribP4ui", &self._opengl32_dll_handle),
            glVertexAttribP4uiv: get_func("glVertexAttribP4uiv", &self._opengl32_dll_handle),
            glVertexAttribPointer: get_func("glVertexAttribPointer", &self._opengl32_dll_handle),
            glVertexP2ui: get_func("glVertexP2ui", &self._opengl32_dll_handle),
            glVertexP2uiv: get_func("glVertexP2uiv", &self._opengl32_dll_handle),
            glVertexP3ui: get_func("glVertexP3ui", &self._opengl32_dll_handle),
            glVertexP3uiv: get_func("glVertexP3uiv", &self._opengl32_dll_handle),
            glVertexP4ui: get_func("glVertexP4ui", &self._opengl32_dll_handle),
            glVertexP4uiv: get_func("glVertexP4uiv", &self._opengl32_dll_handle),
            glVertexPointer: get_func("glVertexPointer", &self._opengl32_dll_handle),
            glViewport: get_func("glViewport", &self._opengl32_dll_handle),
            glWaitSync: get_func("glWaitSync", &self._opengl32_dll_handle),
            glWindowPos2d: get_func("glWindowPos2d", &self._opengl32_dll_handle),
            glWindowPos2dv: get_func("glWindowPos2dv", &self._opengl32_dll_handle),
            glWindowPos2f: get_func("glWindowPos2f", &self._opengl32_dll_handle),
            glWindowPos2fv: get_func("glWindowPos2fv", &self._opengl32_dll_handle),
            glWindowPos2i: get_func("glWindowPos2i", &self._opengl32_dll_handle),
            glWindowPos2iv: get_func("glWindowPos2iv", &self._opengl32_dll_handle),
            glWindowPos2s: get_func("glWindowPos2s", &self._opengl32_dll_handle),
            glWindowPos2sv: get_func("glWindowPos2sv", &self._opengl32_dll_handle),
            glWindowPos3d: get_func("glWindowPos3d", &self._opengl32_dll_handle),
            glWindowPos3dv: get_func("glWindowPos3dv", &self._opengl32_dll_handle),
            glWindowPos3f: get_func("glWindowPos3f", &self._opengl32_dll_handle),
            glWindowPos3fv: get_func("glWindowPos3fv", &self._opengl32_dll_handle),
            glWindowPos3i: get_func("glWindowPos3i", &self._opengl32_dll_handle),
            glWindowPos3iv: get_func("glWindowPos3iv", &self._opengl32_dll_handle),
            glWindowPos3s: get_func("glWindowPos3s", &self._opengl32_dll_handle),
            glWindowPos3sv: get_func("glWindowPos3sv", &self._opengl32_dll_handle),
        });
    }
}
