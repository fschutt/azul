use alloc::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    sync::Arc,
};
use core::{
    cell::{BorrowError, BorrowMutError, RefCell},
    convert::TryInto,
    ffi::c_void,
    fmt, mem, ptr,
    sync::atomic::{AtomicUsize, Ordering as AtomicOrdering},
};
use std::{
    ffi::{CString, OsStr},
    os::raw::{self, c_char, c_int, c_long, c_uchar, c_uint, c_ulong, c_ushort},
};

use azul_core::{
    app_resources::{
        AppConfig, Epoch, GlTextureCache, ImageCache, ImageMask, ImageRef, RendererResources,
        ResourceUpdate,
    },
    callbacks::{DocumentId, DomNodeId, RefAny, UpdateImageType},
    display_list::RenderCallbacks,
    dom::NodeId,
    events::NodesToCheck,
    gl::OptionGlContextPtr,
    styled_dom::DomId,
    task::{Thread, ThreadId, Timer, TimerId},
    ui_solver::LayoutResult,
    window::{
        CallCallbacksResult, FullWindowState, LayoutWindow, LogicalSize, Menu, MenuCallback,
        MenuItem, MonitorVec, MouseCursorType, ScrollResult, WindowCreateOptions, WindowState,
    },
    FastBTreeSet, FastHashMap,
};
use gl_context_loader::{gl, GenericGlContext};
use webrender::{
    api::{
        units::{
            DeviceIntPoint as WrDeviceIntPoint, DeviceIntRect as WrDeviceIntRect,
            DeviceIntSize as WrDeviceIntSize, LayoutSize as WrLayoutSize,
        },
        ApiHitTester as WrApiHitTester, DocumentId as WrDocumentId,
        HitTesterRequest as WrHitTesterRequest, RenderNotifier as WrRenderNotifier,
    },
    render_api::RenderApi as WrRenderApi,
    PipelineInfo as WrPipelineInfo, Renderer as WrRenderer, RendererError as WrRendererError,
    RendererOptions as WrRendererOptions, ShaderPrecacheFlags as WrShaderPrecacheFlags,
    Shaders as WrShaders, Transaction as WrTransaction,
};

use crate::desktop::{
    app::{App, LazyFcCache},
    wr_translate::{
        generate_frame, rebuild_display_list, scroll_all_nodes, synchronize_gpu_values,
        wr_synchronize_updated_images, AsyncHitTester,
    },
};

// TODO: Cache compiled shaders between renderers
const WR_SHADER_CACHE: Option<&Rc<RefCell<WrShaders>>> = None;

extern "C" {
    // syscalls
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
type eglChooseConfigFuncType =
    extern "C" fn(EGLDisplay, *const EGLint, *mut EGLConfig, EGLint, *mut EGLint) -> EGLBoolean;
type eglCreateWindowSurfaceFuncType =
    extern "C" fn(EGLDisplay, EGLConfig, EGLNativeWindowType, *const EGLint) -> EGLSurface;
type eglSwapIntervalFuncType = extern "C" fn(EGLDisplay, EGLint) -> EGLBoolean;
type eglCreateContextFuncType =
    extern "C" fn(EGLDisplay, EGLConfig, EGLContext, *const EGLint) -> EGLContext;
type eglMakeCurrentFuncType =
    extern "C" fn(EGLDisplay, EGLSurface, EGLSurface, EGLContext) -> EGLBoolean;
type eglSwapBuffersFuncType = extern "C" fn(EGLDisplay, EGLSurface) -> EGLBoolean;
type eglGetErrorFuncType = extern "C" fn() -> EGLint;
type eglGetProcAddressFuncType = extern "C" fn(*const c_char) -> *mut raw::c_void;

type XDefaultScreenFuncType = extern "C" fn(*mut Display) -> c_int;
type XRootWindowFuncType = extern "C" fn(*mut Display, c_int) -> c_ulong;
type XCreateWindowFuncType = extern "C" fn(
    *mut Display,
    c_ulong,
    c_int,
    c_int,
    c_uint,
    c_uint,
    c_uint,
    c_int,
    c_uint,
    *mut Visual,
    c_ulong,
    *mut XSetWindowAttributes,
) -> c_ulong;
type XStoreNameFuncType = extern "C" fn(*mut Display, c_ulong, *const c_char) -> c_int;
type XInternAtomFuncType = extern "C" fn(*mut Display, *const c_char, c_int) -> c_ulong;
type XSetWMProtocolsFuncType = extern "C" fn(*mut Display, c_ulong, *mut c_ulong, c_int) -> c_int;
type XMapWindowFuncType = extern "C" fn(*mut Display, c_ulong) -> c_int;
type XOpenDisplayFuncType = extern "C" fn(*const c_char) -> *mut Display;
type XCloseDisplayFuncType = extern "C" fn(*mut Display) -> c_int;
type XPendingFuncType = extern "C" fn(*mut Display) -> c_int;
type XNextEventFuncType = extern "C" fn(*mut Display, *mut XEvent) -> c_int;
type XSelectInputFuncType = extern "C" fn(_: *mut Display, _: c_ulong, _: c_long) -> c_int;

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

// minimal typedefs from X11.h

#[derive(Copy, Clone)]
pub enum _XDisplay {}
pub type Display = _XDisplay;

#[repr(C)]
struct XExtData {
    number: c_int,
    next: *mut XExtData,
    free_private: Option<unsafe extern "C" fn() -> c_int>,
    private_data: *mut c_char,
}

#[repr(C)]
struct Visual {
    ext_data: *mut XExtData,
    visualid: XID,
    class: c_int,
    red_mask: c_ulong,
    green_mask: c_ulong,
    blue_mask: c_ulong,
    bits_per_rgb: c_int,
    map_entries: c_int,
}

type Atom = XID;
type Time = c_ulong;
type Drawable = XID;
type Colormap = XID;
type Bool = c_int;
type Window = XID;
type RRProvider = XID;
type RROutput = XID;
type Rotation = c_ushort;
type SizeID = c_ushort;
type SubpixelOrder = c_ushort;
type RRCrtc = XID;
type RRMode = XID;
type Connection = c_ushort;

#[derive(Copy, Clone)]
#[repr(C)]
struct XF86VidModeNotifyEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: Bool,
    display: *mut Display,
    root: Window,
    state: c_int,
    kind: c_int,
    forced: Bool,
    time: Time,
}

#[derive(Copy, Clone)]
#[repr(C)]
struct XRRScreenChangeNotifyEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: Bool,
    pub display: *mut Display,
    pub window: Window,
    pub root: Window,
    pub timestamp: Time,
    pub config_timestamp: Time,
    pub size_index: SizeID,
    pub subpixel_order: SubpixelOrder,
    pub rotation: Rotation,
    pub width: c_int,
    pub height: c_int,
    pub mwidth: c_int,
    pub mheight: c_int,
}

#[derive(Copy, Clone)]
#[repr(C)]
struct XRRNotifyEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: Bool,
    pub display: *mut Display,
    pub window: Window,
    pub subtype: c_int,
}

#[derive(Copy, Clone)]
#[repr(C)]
struct XRROutputChangeNotifyEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: Bool,
    pub display: *mut Display,
    pub window: Window,
    pub subtype: c_int,
    pub output: RROutput,
    pub crtc: RRCrtc,
    pub mode: RRMode,
    pub rotation: Rotation,
    pub connection: Connection,
    pub subpixel_order: SubpixelOrder,
}

#[derive(Copy, Clone)]
#[repr(C)]
struct XRRCrtcChangeNotifyEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: Bool,
    pub display: *mut Display,
    pub window: Window,
    pub subtype: c_int,
    pub crtc: RRCrtc,
    pub mode: RRMode,
    pub rotation: Rotation,
    pub x: c_int,
    pub y: c_int,
    pub width: c_uint,
    pub height: c_uint,
}

#[derive(Copy, Clone)]
#[repr(C)]
struct XRROutputPropertyNotifyEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: Bool,
    pub display: *mut Display,
    pub window: Window,
    pub subtype: c_int,
    pub output: RROutput,
    pub property: Atom,
    pub timestamp: Time,
    pub state: c_int,
}

#[derive(Copy, Clone)]
#[repr(C)]
struct XRRProviderChangeNotifyEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: Bool,
    pub display: *mut Display,
    pub window: Window,
    pub subtype: c_int,
    pub provider: RRProvider,
    pub timestamp: Time,
    pub current_role: c_uint,
}

#[derive(Copy, Clone)]
#[repr(C)]
struct XRRProviderPropertyNotifyEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: Bool,
    pub display: *mut Display,
    pub window: Window,
    pub subtype: c_int,
    pub provider: RRProvider,
    pub property: Atom,
    pub timestamp: Time,
    pub state: c_int,
}

#[derive(Copy, Clone)]
#[repr(C)]
struct XRRResourceChangeNotifyEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: Bool,
    pub display: *mut Display,
    pub window: Window,
    pub subtype: c_int,
    pub timestamp: Time,
}

#[derive(Copy, Clone)]
#[repr(C)]
struct XScreenSaverNotifyEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: Bool,
    pub display: *mut Display,
    pub window: Window,
    pub root: Window,
    pub state: c_int,
    pub kind: c_int,
    pub forced: Bool,
    pub time: Time,
}

#[repr(C)]
#[derive(Copy, Clone)]
union XEvent {
    type_: c_int,
    any: XAnyEvent,
    button: XButtonEvent,
    circulate: XCirculateEvent,
    circulate_request: XCirculateRequestEvent,
    client_message: XClientMessageEvent,
    colormap: XColormapEvent,
    configure: XConfigureEvent,
    configure_request: XConfigureRequestEvent,
    create_window: XCreateWindowEvent,
    crossing: XCrossingEvent,
    destroy_window: XDestroyWindowEvent,
    error: XErrorEvent,
    expose: XExposeEvent,
    focus_change: XFocusChangeEvent,
    generic_event_cookie: XGenericEventCookie,
    graphics_expose: XGraphicsExposeEvent,
    gravity: XGravityEvent,
    key: XKeyEvent,
    keymap: XKeymapEvent,
    map: XMapEvent,
    mapping: XMappingEvent,
    map_request: XMapRequestEvent,
    motion: XMotionEvent,
    no_expose: XNoExposeEvent,
    property: XPropertyEvent,
    reparent: XReparentEvent,
    resize_request: XResizeRequestEvent,
    selection_clear: XSelectionClearEvent,
    selection: XSelectionEvent,
    selection_request: XSelectionRequestEvent,
    unmap: XUnmapEvent,
    visibility: XVisibilityEvent,
    pad: [c_long; 24],
    // xf86vidmode
    xf86vm_notify: XF86VidModeNotifyEvent,
    // xrandr
    xrr_screen_change_notify: XRRScreenChangeNotifyEvent,
    xrr_notify: XRRNotifyEvent,
    xrr_output_change_notify: XRROutputChangeNotifyEvent,
    xrr_crtc_change_notify: XRRCrtcChangeNotifyEvent,
    xrr_output_property_notify: XRROutputPropertyNotifyEvent,
    xrr_provider_change_notify: XRRProviderChangeNotifyEvent,
    xrr_provider_property_notify: XRRProviderPropertyNotifyEvent,
    xrr_resource_change_notify: XRRResourceChangeNotifyEvent,
    // xscreensaver
    xss_notify: XScreenSaverNotifyEvent,
}

impl XEvent {
    pub fn get_type(&self) -> c_int {
        unsafe { self.type_ }
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XAnyEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    window: XID,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XButtonEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    window: XID,
    root: XID,
    subwindow: XID,
    time: Time,
    x: c_int,
    y: c_int,
    x_root: c_int,
    y_root: c_int,
    state: c_uint,
    button: c_uint,
    same_screen: X11Bool,
}
type XButtonPressedEvent = XButtonEvent;
type XButtonReleasedEvent = XButtonEvent;

#[repr(C)]
#[derive(Copy, Clone)]
struct XCirculateEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    event: XID,
    window: XID,
    place: c_int,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XCirculateRequestEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    parent: XID,
    window: XID,
    place: c_int,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XClientMessageEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    window: XID,
    message_type: Atom,
    format: c_int,
    data: ClientMessageData,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[repr(C)]
pub struct ClientMessageData {
    longs: [c_long; 5],
}

impl ClientMessageData {
    pub fn as_longs(&self) -> &[c_long] {
        self.longs.as_ref()
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct XGenericEventCookie {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: X11Bool,
    pub display: *mut Display,
    pub extension: c_int,
    pub evtype: c_int,
    pub cookie: c_uint,
    pub data: *mut c_void,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XColormapEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    window: XID,
    colormap: Colormap,
    new: X11Bool,
    state: c_int,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XConfigureEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    event: XID,
    window: XID,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
    border_width: c_int,
    above: XID,
    override_redirect: X11Bool,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XConfigureRequestEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    parent: XID,
    window: XID,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
    border_width: c_int,
    above: XID,
    detail: c_int,
    value_mask: c_ulong,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XCreateWindowEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    parent: XID,
    window: XID,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
    border_width: c_int,
    override_redirect: X11Bool,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XCrossingEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    window: XID,
    root: XID,
    subwindow: XID,
    time: Time,
    x: c_int,
    y: c_int,
    x_root: c_int,
    y_root: c_int,
    mode: c_int,
    detail: c_int,
    same_screen: X11Bool,
    focus: X11Bool,
    state: c_uint,
}
type XEnterWindowEvent = XCrossingEvent;
type XLeaveWindowEvent = XCrossingEvent;

#[repr(C)]
#[derive(Copy, Clone)]
struct XDestroyWindowEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    event: XID,
    window: XID,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XErrorEvent {
    type_: c_int,
    display: *mut Display,
    resourceid: XID,
    serial: c_ulong,
    error_code: c_uchar,
    request_code: c_uchar,
    minor_code: c_uchar,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XExposeEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    window: XID,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
    count: c_int,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XFocusChangeEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    window: XID,
    mode: c_int,
    detail: c_int,
}
type XFocusInEvent = XFocusChangeEvent;
type XFocusOutEvent = XFocusChangeEvent;

#[repr(C)]
#[derive(Copy, Clone)]
struct XGraphicsExposeEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    drawable: Drawable,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
    count: c_int,
    major_code: c_int,
    minor_code: c_int,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XGravityEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    event: XID,
    window: XID,
    x: c_int,
    y: c_int,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XKeyEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    window: XID,
    root: XID,
    subwindow: XID,
    time: Time,
    x: c_int,
    y: c_int,
    x_root: c_int,
    y_root: c_int,
    state: c_uint,
    keycode: c_uint,
    same_screen: X11Bool,
}
type XKeyPressedEvent = XKeyEvent;
type XKeyReleasedEvent = XKeyEvent;

#[repr(C)]
#[derive(Copy, Clone)]
struct XKeymapEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    window: XID,
    key_vector: [c_char; 32],
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XMapEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    event: XID,
    window: XID,
    override_redirect: X11Bool,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XMappingEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    event: XID,
    request: c_int,
    first_keycode: c_int,
    count: c_int,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XMapRequestEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    parent: XID,
    window: XID,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XMotionEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    window: XID,
    root: XID,
    subwindow: XID,
    time: Time,
    x: c_int,
    y: c_int,
    x_root: c_int,
    y_root: c_int,
    state: c_uint,
    is_hint: c_char,
    same_screen: X11Bool,
}
type XPointerMovedEvent = XMotionEvent;

#[repr(C)]
#[derive(Copy, Clone)]
struct XNoExposeEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    drawable: Drawable,
    major_code: c_int,
    minor_code: c_int,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XPropertyEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    window: XID,
    atom: Atom,
    time: Time,
    state: c_int,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XReparentEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    event: XID,
    window: XID,
    parent: XID,
    x: c_int,
    y: c_int,
    override_redirect: X11Bool,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XResizeRequestEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    window: XID,
    width: c_int,
    height: c_int,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XSelectionClearEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    window: XID,
    selection: Atom,
    time: Time,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XSelectionEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    requestor: XID,
    selection: Atom,
    target: Atom,
    property: Atom,
    time: Time,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XSelectionRequestEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    owner: XID,
    requestor: XID,
    selection: Atom,
    target: Atom,
    property: Atom,
    time: Time,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XUnmapEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    event: XID,
    window: XID,
    from_configure: X11Bool,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XVisibilityEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: X11Bool,
    display: *mut Display,
    window: XID,
    state: c_int,
}

const X11_INPUT_OUTPUT: c_int = 1;
const X11_COPY_FROM_PARENT: c_int = 0;
const X11_CW_EVENT_MASK: c_ulong = 0x0800;
const X11_STRUCTURE_NOTIFY_MASK: c_long = 0x0002_0000;
const X11_EXPOSURE_MASK: c_long = 0x0000_8000;
const X11_RESIZE_REDIRECT_MASK: c_long = 0x0004_0000;
const X11_KEY_PRESS_MASK: c_long = 0x0000_0001;
const X11_KEY_RELEASE_MASK: c_long = 0x0000_0002;
const X11_POINTER_MOTION_MASK: c_long = 0x0000_0040;
const X11_BUTTON_PRESS_MASK: c_long = 0x0000_0004;
const X11_BUTTON_RELEASE_MASK: c_long = 0x0000_0008;

const X11_FALSE: X11Bool = 0;

const X11_EXPOSE: c_int = 12;
const X11_RESIZE_REQUEST: c_int = 25;
const X11_CLIENT_MESSAGE: c_int = 33;

type X11Bool = c_int;
type XID = c_ulong;
type X11Pixmap = XID;
type X11Colormap = XID;
type X11Cursor = XID;

#[repr(C)]
struct XSetWindowAttributes {
    pub background_pixmap: X11Pixmap,
    pub background_pixel: c_ulong,
    pub border_pixmap: X11Pixmap,
    pub border_pixel: c_ulong,
    pub bit_gravity: c_int,
    pub win_gravity: c_int,
    pub backing_store: c_int,
    pub backing_planes: c_ulong,
    pub backing_pixel: c_ulong,
    pub save_under: X11Bool,
    pub event_mask: c_long,
    pub do_not_propagate_mask: c_long,
    pub override_redirect: X11Bool,
    pub colormap: X11Colormap,
    pub cursor: X11Cursor,
}

/// Main function that starts when app.run() is invoked
pub fn run(app: App, mut root_window: WindowCreateOptions) -> Result<isize, LinuxStartupError> {
    use self::{
        LinuxStartupError::Create,
        LinuxWindowCreateError::{Egl as EglError, X},
    };

    let App {
        data,
        config,
        mut windows,
        image_cache,
        fc_cache,
    } = app;

    let xlib = Rc::new(Xlib::new()?);
    let egl = Rc::new(Egl::new()?);

    let mut active_windows = BTreeMap::new();

    let app_data_inner = Rc::new(RefCell::new(AppData {
        data,
        config,
        image_cache,
        fc_cache,
    }));

    for options in windows.iter_mut() {
        let mut window = X11Window::new(
            xlib.clone(),
            egl.clone(),
            options,
            SharedApplicationData {
                inner: app_data_inner.clone(),
            },
        )?;
        window.show();
        active_windows.insert(window.id, window);
    }

    let mut window = X11Window::new(
        xlib.clone(),
        egl.clone(),
        &mut root_window,
        SharedApplicationData {
            inner: app_data_inner.clone(),
        },
    )?;
    window.show();
    active_windows.insert(window.id, window);

    let mut cur_xevent = XEvent { pad: [0; 24] };

    loop {
        let mut windows_to_close = Vec::new();

        for (window_id, window) in active_windows.iter_mut() {
            // blocks until next event
            unsafe { (xlib.XNextEvent)(window.dpy.get(), &mut cur_xevent) };

            let cur_event_type = cur_xevent.get_type();

            match cur_event_type {
                // window shown
                X11_EXPOSE => {
                    let expose_data = unsafe { cur_xevent.expose };
                    let width = expose_data.width;
                    let height = expose_data.height;

                    window.make_current();
                    window.render_api.flush_scene_builder();

                    window
                        .gl_functions
                        .functions
                        .bind_framebuffer(gl_context_loader::gl::FRAMEBUFFER, 0);
                    window
                        .gl_functions
                        .functions
                        .disable(gl_context_loader::gl::FRAMEBUFFER_SRGB);
                    window
                        .gl_functions
                        .functions
                        .disable(gl_context_loader::gl::MULTISAMPLE);

                    window.gl_functions.functions.viewport(0, 0, width, height);
                    window
                        .gl_functions
                        .functions
                        .clear_color(0.0, 0.0, 0.0, 1.0);
                    window.gl_functions.functions.clear(
                        gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT | gl::STENCIL_BUFFER_BIT,
                    );

                    let mut current_program = [0_i32];
                    unsafe {
                        window.gl_functions.functions.get_integer_v(
                            gl_context_loader::gl::CURRENT_PROGRAM,
                            (&mut current_program[..]).into(),
                        );
                    }

                    if let Some(r) = window.renderer.as_mut() {
                        let framebuffer_size = WrDeviceIntSize::new(width, height);
                        r.update();
                        let _ = r.render(framebuffer_size, 0);
                    }

                    let swap_result =
                        (window.egl.eglSwapBuffers)(window.egl_display, window.egl_surface);
                    if swap_result != EGL_TRUE {
                        return Err(Create(EglError(format!(
                            "EGL: eglSwapBuffers(): Failed to swap OpenGL buffers: {}",
                            swap_result
                        ))));
                    }

                    window
                        .gl_functions
                        .functions
                        .bind_framebuffer(gl_context_loader::gl::FRAMEBUFFER, 0);
                    window
                        .gl_functions
                        .functions
                        .bind_texture(gl_context_loader::gl::TEXTURE_2D, 0);
                    window
                        .gl_functions
                        .functions
                        .use_program(current_program[0] as u32);
                }
                // window resized
                X11_RESIZE_REQUEST => {
                    let resize_request_data = unsafe { cur_xevent.resize_request };
                    let width = resize_request_data.width;
                    let height = resize_request_data.height;

                    window.make_current();
                    window.render_api.flush_scene_builder();

                    window
                        .gl_functions
                        .functions
                        .bind_framebuffer(gl_context_loader::gl::FRAMEBUFFER, 0);
                    window
                        .gl_functions
                        .functions
                        .disable(gl_context_loader::gl::FRAMEBUFFER_SRGB);
                    window
                        .gl_functions
                        .functions
                        .disable(gl_context_loader::gl::MULTISAMPLE);

                    window.gl_functions.functions.viewport(0, 0, width, height);
                    window
                        .gl_functions
                        .functions
                        .clear_color(0.0, 0.0, 0.0, 1.0);
                    window.gl_functions.functions.clear(
                        gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT | gl::STENCIL_BUFFER_BIT,
                    );

                    let mut current_program = [0_i32];
                    unsafe {
                        window.gl_functions.functions.get_integer_v(
                            gl_context_loader::gl::CURRENT_PROGRAM,
                            (&mut current_program[..]).into(),
                        );
                    }

                    if let Some(r) = window.renderer.as_mut() {
                        let framebuffer_size = WrDeviceIntSize::new(width, height);
                        r.update();
                        let _ = r.render(framebuffer_size, 0);
                    }

                    let swap_result =
                        (window.egl.eglSwapBuffers)(window.egl_display, window.egl_surface);
                    if swap_result != EGL_TRUE {
                        return Err(Create(EglError(format!(
                            "EGL: eglSwapBuffers(): Failed to swap OpenGL buffers: {}",
                            swap_result
                        ))));
                    }
                }
                // window closed
                X11_CLIENT_MESSAGE => {
                    let xclient_data = unsafe { cur_xevent.client_message };
                    if (xclient_data.data.as_longs().get(0).copied()
                        == Some(window.wm_delete_window_atom))
                    {
                        windows_to_close.push(*window_id);
                    }
                }
                _ => {}
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
pub struct SharedApplicationData {
    inner: Rc<RefCell<AppData>>,
}

// AppData struct that is shared across windows
#[derive(Debug)]
pub struct AppData {
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
    pub dpy: X11Display,
    // EGL OpenGL 3.2 context
    pub egl_surface: EGLSurface,
    pub egl_display: EGLDisplay,
    pub egl_context: EGLContext,
    // XAtom fired when the window close button is hit
    pub wm_delete_window_atom: c_long,
    // X11 library (dynamically loaded)
    pub xlib: Rc<Xlib>,
    // libEGL.so library (dynamically loaded)
    pub egl: Rc<Egl>,
    // OpenGL functions, loaded from libEGL.so
    pub gl_functions: GlFunctions,
    /// See azul-core, stores the entire UI (DOM, CSS styles, layout results, etc.)
    pub internal: LayoutWindow,
    /// OpenGL context pointer with compiled SVG and FXAA shaders
    pub gl_context_ptr: OptionGlContextPtr,
    /// Main render API that can be used to register and un-register fonts and images
    pub render_api: WrRenderApi,
    /// WebRender renderer implementation (software or hardware)
    pub renderer: Option<WrRenderer>,
    /// Hit-tester, lazily initialized and updated every time the display list changes layout
    pub hit_tester: AsyncHitTester,
}

struct Xlib {
    pub library: Library,
    pub XDefaultScreen: XDefaultScreenFuncType,
    pub XRootWindow: XRootWindowFuncType,
    pub XCreateWindow: XCreateWindowFuncType,
    pub XStoreName: XStoreNameFuncType,
    pub XInternAtom: XInternAtomFuncType,
    pub XSetWMProtocols: XSetWMProtocolsFuncType,
    pub XMapWindow: XMapWindowFuncType,
    pub XOpenDisplay: XOpenDisplayFuncType,
    pub XCloseDisplay: XCloseDisplayFuncType,
    pub XPending: XPendingFuncType,
    pub XNextEvent: XNextEventFuncType,
    pub XSelectInput: XSelectInputFuncType,
}

impl Xlib {
    fn new() -> Result<Self, LinuxStartupError> {
        use self::{
            LinuxStartupError::Create,
            LinuxWindowCreateError::{Egl, X},
        };

        let x11 =
            Library::load("libX11.so").map_err(|e| X(format!("Could not load libX11: {}", e)))?;

        let XDefaultScreen: XDefaultScreenFuncType = x11
            .get("XDefaultScreen")
            .and_then(|ptr| {
                if ptr.is_null() {
                    None
                } else {
                    Some(unsafe { mem::transmute(ptr) })
                }
            })
            .ok_or(Create(Egl(format!("X11: no function XDefaultScreen"))))?;

        let XRootWindow: XRootWindowFuncType = x11
            .get("XRootWindow")
            .and_then(|ptr| {
                if ptr.is_null() {
                    None
                } else {
                    Some(unsafe { mem::transmute(ptr) })
                }
            })
            .ok_or(Create(Egl(format!("X11: no function XRootWindow"))))?;

        let XCreateWindow: XCreateWindowFuncType = x11
            .get("XCreateWindow")
            .and_then(|ptr| {
                if ptr.is_null() {
                    None
                } else {
                    Some(unsafe { mem::transmute(ptr) })
                }
            })
            .ok_or(Create(Egl(format!("X11: no function XCreateWindow"))))?;

        let XStoreName: XStoreNameFuncType = x11
            .get("XStoreName")
            .and_then(|ptr| {
                if ptr.is_null() {
                    None
                } else {
                    Some(unsafe { mem::transmute(ptr) })
                }
            })
            .ok_or(Create(Egl(format!("X11: no function XStoreName"))))?;

        let XInternAtom: XInternAtomFuncType = x11
            .get("XInternAtom")
            .and_then(|ptr| {
                if ptr.is_null() {
                    None
                } else {
                    Some(unsafe { mem::transmute(ptr) })
                }
            })
            .ok_or(Create(Egl(format!("X11: no function XInternAtom"))))?;

        let XSetWMProtocols: XSetWMProtocolsFuncType = x11
            .get("XSetWMProtocols")
            .and_then(|ptr| {
                if ptr.is_null() {
                    None
                } else {
                    Some(unsafe { mem::transmute(ptr) })
                }
            })
            .ok_or(Create(Egl(format!("X11: no function XSetWMProtocols"))))?;

        let XMapWindow: XMapWindowFuncType = x11
            .get("XMapWindow")
            .and_then(|ptr| {
                if ptr.is_null() {
                    None
                } else {
                    Some(unsafe { mem::transmute(ptr) })
                }
            })
            .ok_or(Create(Egl(format!("X11: no function XMapWindow"))))?;

        let XOpenDisplay: XOpenDisplayFuncType = x11
            .get("XOpenDisplay")
            .and_then(|ptr| {
                if ptr.is_null() {
                    None
                } else {
                    Some(unsafe { mem::transmute(ptr) })
                }
            })
            .ok_or(Create(Egl(format!("X11: no function XOpenDisplay"))))?;

        let XCloseDisplay: XCloseDisplayFuncType = x11
            .get("XCloseDisplay")
            .and_then(|ptr| {
                if ptr.is_null() {
                    None
                } else {
                    Some(unsafe { mem::transmute(ptr) })
                }
            })
            .ok_or(Create(Egl(format!("X11: no function XCloseDisplay"))))?;

        let XPending: XPendingFuncType = x11
            .get("XPending")
            .and_then(|ptr| {
                if ptr.is_null() {
                    None
                } else {
                    Some(unsafe { mem::transmute(ptr) })
                }
            })
            .ok_or(Create(Egl(format!("X11: no function XPending"))))?;

        let XNextEvent: XNextEventFuncType = x11
            .get("XNextEvent")
            .and_then(|ptr| {
                if ptr.is_null() {
                    None
                } else {
                    Some(unsafe { mem::transmute(ptr) })
                }
            })
            .ok_or(Create(Egl(format!("X11: no function XNextEvent"))))?;

        let XSelectInput: XSelectInputFuncType = x11
            .get("XSelectInput")
            .and_then(|ptr| {
                if ptr.is_null() {
                    None
                } else {
                    Some(unsafe { mem::transmute(ptr) })
                }
            })
            .ok_or(Create(Egl(format!("X11: no function XSelectInput"))))?;

        Ok(Xlib {
            library: x11,
            XDefaultScreen,
            XRootWindow,
            XCreateWindow,
            XStoreName,
            XInternAtom,
            XSetWMProtocols,
            XMapWindow,
            XOpenDisplay,
            XCloseDisplay,
            XPending,
            XNextEvent,
            XSelectInput,
        })
    }
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
    pub eglGetProcAddress: eglGetProcAddressFuncType,
}

impl Egl {
    fn new() -> Result<Self, LinuxStartupError> {
        use self::{
            LinuxStartupError::Create,
            LinuxWindowCreateError::{Egl, X},
        };

        let egl =
            Library::load("libEGL.so").map_err(|e| X(format!("Could not load libEGL: {}", e)))?;

        let eglMakeCurrent: eglMakeCurrentFuncType = egl
            .get("eglMakeCurrent")
            .and_then(|ptr| {
                if ptr.is_null() {
                    None
                } else {
                    Some(unsafe { mem::transmute(ptr) })
                }
            })
            .ok_or(Create(Egl(format!("EGL: no function eglMakeCurrent"))))?;
        let eglSwapBuffers: eglSwapBuffersFuncType = egl
            .get("eglSwapBuffers")
            .and_then(|ptr| {
                if ptr.is_null() {
                    None
                } else {
                    Some(unsafe { mem::transmute(ptr) })
                }
            })
            .ok_or(Create(Egl(format!("EGL: no function eglSwapBuffers"))))?;
        let eglGetDisplay: eglGetDisplayFuncType = egl
            .get("eglGetDisplay")
            .and_then(|ptr| {
                if ptr.is_null() {
                    None
                } else {
                    Some(unsafe { mem::transmute(ptr) })
                }
            })
            .ok_or(Create(Egl(format!("EGL: no function eglGetDisplay"))))?;
        let eglInitialize: eglInitializeFuncType = egl
            .get("eglInitialize")
            .and_then(|ptr| {
                if ptr.is_null() {
                    None
                } else {
                    Some(unsafe { mem::transmute(ptr) })
                }
            })
            .ok_or(Create(Egl(format!("EGL: no function eglInitialize"))))?;
        let eglBindAPI: eglBindAPIFuncType = egl
            .get("eglBindAPI")
            .and_then(|ptr| {
                if ptr.is_null() {
                    None
                } else {
                    Some(unsafe { mem::transmute(ptr) })
                }
            })
            .ok_or(Create(Egl(format!("EGL: no function eglBindAPI"))))?;
        let eglChooseConfig: eglChooseConfigFuncType = egl
            .get("eglChooseConfig")
            .and_then(|ptr| {
                if ptr.is_null() {
                    None
                } else {
                    Some(unsafe { mem::transmute(ptr) })
                }
            })
            .ok_or(Create(Egl(format!("EGL: no function eglChooseConfig"))))?;
        let eglCreateWindowSurface: eglCreateWindowSurfaceFuncType = egl
            .get("eglCreateWindowSurface")
            .and_then(|ptr| {
                if ptr.is_null() {
                    None
                } else {
                    Some(unsafe { mem::transmute(ptr) })
                }
            })
            .ok_or(Create(Egl(format!(
                "EGL: no function eglCreateWindowSurface"
            ))))?;
        let eglSwapInterval: eglSwapIntervalFuncType = egl
            .get("eglSwapInterval")
            .and_then(|ptr| {
                if ptr.is_null() {
                    None
                } else {
                    Some(unsafe { mem::transmute(ptr) })
                }
            })
            .ok_or(Create(Egl(format!("EGL: no function eglSwapInterval"))))?;
        let eglCreateContext: eglCreateContextFuncType = egl
            .get("eglCreateContext")
            .and_then(|ptr| {
                if ptr.is_null() {
                    None
                } else {
                    Some(unsafe { mem::transmute(ptr) })
                }
            })
            .ok_or(Create(Egl(format!("EGL: no function eglCreateContext"))))?;
        let eglGetError: eglGetErrorFuncType = egl
            .get("eglGetError")
            .and_then(|ptr| {
                if ptr.is_null() {
                    None
                } else {
                    Some(unsafe { mem::transmute(ptr) })
                }
            })
            .ok_or(Create(Egl(format!("EGL: no function eglGetError"))))?;
        let eglGetProcAddress: eglGetProcAddressFuncType = egl
            .get("eglGetProcAddress")
            .and_then(|ptr| {
                if ptr.is_null() {
                    None
                } else {
                    Some(unsafe { mem::transmute(ptr) })
                }
            })
            .ok_or(Create(Egl(format!("EGL: no function eglGetProcAddress"))))?;

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
            eglGetProcAddress,
        })
    }
}

impl X11Window {
    fn new(
        xlib: Rc<Xlib>,
        egl: Rc<Egl>,
        options: &mut WindowCreateOptions,
        shared_application_data: SharedApplicationData,
    ) -> Result<Self, LinuxStartupError> {
        use azul_core::{
            callbacks::PipelineId,
            gl::GlContextPtr,
            window::{HwAcceleration, RendererType},
        };
        use webrender::{api::ColorF as WrColorF, ProgramCache as WrProgramCache};

        use self::{
            LinuxStartupError::Create,
            LinuxWindowCreateError::{Egl as EglError, X},
        };
        use crate::desktop::{
            compositor::Compositor,
            wr_translate::{
                translate_document_id_wr, translate_id_namespace_wr, wr_translate_debug_flags,
                wr_translate_document_id,
            },
        };

        let mut dpy =
            X11Display::open(xlib.clone()).ok_or(X(format!("X11: XOpenDisplay(0) failed")))?;

        // DefaultRootWindow shim
        let scrnum = unsafe { (xlib.XDefaultScreen)(dpy.get()) };
        let root = unsafe { (xlib.XRootWindow)(dpy.get(), scrnum) };

        let mask = X11_EXPOSURE_MASK
            | X11_KEY_PRESS_MASK
            | X11_KEY_RELEASE_MASK
            | X11_POINTER_MOTION_MASK
            | X11_BUTTON_PRESS_MASK
            | X11_BUTTON_RELEASE_MASK
            | X11_STRUCTURE_NOTIFY_MASK;

        let mut xattr: XSetWindowAttributes = unsafe { mem::zeroed() };
        xattr.event_mask = mask;

        let dpi_scale_factor = dpy.get_dpi_scale_factor();
        options.state.size.dpi = (dpi_scale_factor.max(0.0) * 96.0).round() as u32;
        options.state.size.hidpi_factor = dpi_scale_factor;
        options.state.size.system_hidpi_factor = dpi_scale_factor;

        let logical_size = options.state.size.dimensions;
        let physical_size = logical_size.to_physical(dpi_scale_factor);

        let window = unsafe {
            (xlib.XCreateWindow)(
                dpy.get(),
                root,
                0,
                0,
                logical_size.width.round().max(0.0) as u32,
                logical_size.height.round().max(0.0) as u32,
                0,
                X11_COPY_FROM_PARENT,
                X11_INPUT_OUTPUT as u32,
                ptr::null_mut(), // = CopyFromParent
                X11_CW_EVENT_MASK,
                &mut xattr,
            )
        };

        if window == 0 {
            return Err(Create(X(format!("X11: XCreateWindow failed"))));
        }

        unsafe {
            (xlib.XSelectInput)(dpy.get(), window, mask);
        }

        let window_title = encode_ascii(&options.state.title);
        unsafe { (xlib.XStoreName)(dpy.get(), window, window_title.as_ptr() as *const i8) };

        // subscribe to window close notification
        let wm_protocols_atom = unsafe {
            (xlib.XInternAtom)(
                dpy.get(),
                encode_ascii("WM_PROTOCOLS").as_ptr() as *const i8,
                X11_FALSE,
            )
        };

        let mut wm_delete_window_atom = unsafe {
            (xlib.XInternAtom)(
                dpy.get(),
                encode_ascii("WM_DELETE_WINDOW").as_ptr() as *const i8,
                X11_FALSE,
            )
        };

        unsafe { (xlib.XSetWMProtocols)(dpy.get(), window, &mut wm_delete_window_atom, 1) };

        let egl_display = (egl.eglGetDisplay)(dpy.display as *mut c_void);
        if egl_display == EGL_NO_DISPLAY {
            return Err(Create(EglError(format!(
                "EGL: eglGetDisplay(): no display"
            ))));
        }

        let mut major = 0;
        let mut minor = 0;

        let init_result = (egl.eglInitialize)(egl_display, &mut major, &mut minor);
        if init_result != EGL_TRUE {
            return Err(Create(EglError(format!(
                "EGL: eglInitialize(): cannot initialize display: {}",
                init_result
            ))));
        }

        // choose OpenGL API for EGL, by default it uses OpenGL ES
        let egl_bound = (egl.eglBindAPI)(EGL_OPENGL_API);
        if egl_bound != EGL_TRUE {
            return Err(Create(EglError(format!(
                "EGL: eglBindAPI(): Failed to select OpenGL API for EGL: {}",
                egl_bound
            ))));
        }

        let egl_attr = [
            EGL_SURFACE_TYPE,
            EGL_WINDOW_BIT,
            EGL_CONFORMANT,
            EGL_OPENGL_BIT,
            EGL_RENDERABLE_TYPE,
            EGL_OPENGL_BIT,
            EGL_COLOR_BUFFER_TYPE,
            EGL_RGB_BUFFER,
            EGL_RED_SIZE,
            8,
            EGL_GREEN_SIZE,
            8,
            EGL_BLUE_SIZE,
            8,
            EGL_DEPTH_SIZE,
            24,
            EGL_STENCIL_SIZE,
            8,
            EGL_NONE,
        ];

        let mut config: EGLConfig = unsafe { mem::zeroed() };
        let mut count = 0;
        let egl_config_chosen =
            (egl.eglChooseConfig)(egl_display, egl_attr.as_ptr(), &mut config, 1, &mut count);
        if egl_config_chosen != EGL_TRUE {
            return Err(Create(EglError(format!(
                "EGL: eglChooseConfig(): Cannot choose EGL config: {}",
                egl_config_chosen
            ))));
        }

        if count != 1 {
            return Err(Create(EglError(format!(
                "EGL: eglChooseConfig(): Expected 1 EglConfig, got {}",
                count
            ))));
        }

        let egl_surface_attr = [
            EGL_GL_COLORSPACE,
            EGL_GL_COLORSPACE_LINEAR,
            EGL_RENDER_BUFFER,
            EGL_BACK_BUFFER,
            EGL_NONE,
        ];

        let egl_surface = (egl.eglCreateWindowSurface)(
            egl_display,
            config,
            unsafe { mem::transmute(window as usize) },
            egl_surface_attr.as_ptr(),
        );

        if egl_surface == EGL_NO_SURFACE {
            return Err(Create(EglError(format!(
                "EGL: eglCreateWindowSurface(): no surface found"
            ))));
        }

        let egl_context_attr = [
            EGL_CONTEXT_MAJOR_VERSION,
            3,
            EGL_CONTEXT_MINOR_VERSION,
            2,
            EGL_CONTEXT_OPENGL_PROFILE_MASK,
            EGL_CONTEXT_OPENGL_CORE_PROFILE_BIT,
            EGL_NONE,
        ];

        let egl_context = (egl.eglCreateContext)(
            egl_display,
            config,
            EGL_NO_CONTEXT,
            egl_context_attr.as_ptr(),
        );
        if egl_context == EGL_NO_CONTEXT {
            let err = (egl.eglGetError)();
            return Err(Create(EglError(format!(
                "EGL: eglCreateContext() failed with status {} = {}",
                err,
                display_egl_status(err)
            ))));
        }

        let egl_is_current =
            (egl.eglMakeCurrent)(egl_display, egl_surface, egl_surface, egl_context);
        if egl_is_current != EGL_TRUE {
            return Err(Create(EglError(format!(
                "EGL: eglMakeCurrent(): failed to make context current: {}",
                egl_is_current
            ))));
        }

        let mut gl_functions = GlFunctions::initialize(egl.clone());
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
        let gl_context_ptr = Some(GlContextPtr::new(rt, gl_functions.functions.clone())).into();

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
                enable_multithreading: false,
                debug_flags: wr_translate_debug_flags(&options.state.debug_state),
                ..WrRendererOptions::default()
            },
            WR_SHADER_CACHE,
        )
        .map_err(|e| Create(EglError(format!("Could not init WebRender: {:?}", e))))?;

        renderer.set_external_image_handler(Box::new(Compositor::default()));

        let mut render_api = sender.create_api();

        let framebuffer_size =
            WrDeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);
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
            use azul_layout::window::LayoutWindowInit;

            LayoutWindow::new(
                LayoutWindowInit {
                    window_create_options: options.clone(),
                    document_id,
                    id_namespace,
                },
                data,
                image_cache,
                &gl_context_ptr,
                &mut initial_resource_updates,
                &crate::desktop::app::CALLBACKS,
                fc_cache,
                azul_layout::do_the_relayout,
                |window_state, scroll_states, layout_results| {
                    crate::desktop::wr_translate::fullhittest_new_webrender(
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
                &crate::desktop::app::CALLBACKS,
                azul_layout::do_the_relayout,
                fc_cache,
                &gl_context_ptr,
                &size,
                theme,
            )
        });

        wr_synchronize_updated_images(resize_result.updated_images, &document_id, &mut txn);

        txn.set_document_view(WrDeviceIntRect::from_size(WrDeviceIntSize::new(
            physical_size.width as i32,
            physical_size.height as i32,
        )));

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

        generate_frame(&mut internal, &mut render_api, true);

        render_api.flush_scene_builder();

        // Update the hit-tester to account for the new hit-testing functionality
        let hit_tester = render_api.request_hit_tester(wr_translate_document_id(document_id));

        Ok(Self {
            egl_surface,
            egl_display,
            egl_context,
            wm_delete_window_atom: wm_delete_window_atom as i64,
            id: window,
            dpy,
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
            self.egl_context,
        );
    }

    fn show(&mut self) {
        unsafe { (self.xlib.XMapWindow)(self.dpy.get(), self.id) };
    }
}

struct X11Display {
    display: *mut Display,
    xlib: Rc<Xlib>,
}

impl X11Display {
    fn get<'a>(&'a mut self) -> &'a mut Display {
        unsafe { &mut *self.display }
    }

    fn open(xlib: Rc<Xlib>) -> Option<Self> {
        let dpy = unsafe { (xlib.XOpenDisplay)(&0) };

        if dpy.is_null() {
            return None;
        }

        Some(Self { display: dpy, xlib })
    }

    /// Return the DPI on X11 systems
    ///
    /// Note: slow - cache output!
    pub fn get_dpi_scale_factor(&self) -> f32 {
        use std::{env, process::Command};

        // Execute "gsettings get org.gnome.desktop.interface text-scaling-factor"
        // and parse the output
        let gsettings_dpi_factor = Command::new("gsettings")
            .arg("get")
            .arg("org.gnome.desktop.interface")
            .arg("text-scaling-factor")
            .output()
            .ok()
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
        let _ = unsafe { (self.xlib.XCloseDisplay)(self.display) };
    }
}

/// A platform-specific equivalent of the cross-platform `Library`.
pub struct Library {
    name: &'static str,
    ptr: *mut raw::c_void,
}

unsafe impl Send for Library {}
unsafe impl Sync for Library {}

impl Library {
    /// Dynamically load an arbitrary library by its name (dlopen)
    pub fn load(name: &'static str) -> Result<Self, String> {
        use alloc::borrow::Cow;
        use std::ffi::{CStr, CString};

        const RTLD_NOW: raw::c_int = 2;

        let cow = CString::new(name.as_bytes()).map_err(|e| String::new())?;
        let ptr = unsafe { dlopen(cow.as_ptr(), RTLD_NOW) };

        if ptr.is_null() {
            let dlerr = unsafe { CStr::from_ptr(dlerror()) };
            Err(dlerr
                .to_str()
                .ok()
                .map(|s| s.to_string())
                .unwrap_or_default())
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
    egl: Rc<Egl>,
    // implements Rc<dyn gleam::Gl>!
    functions: Rc<GenericGlContext>,
}

impl fmt::Debug for GlFunctions {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self._opengl32_dll_handle
            .as_ref()
            .map(|f| f.ptr as usize)
            .fmt(f)?;
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
    fn initialize(egl: Rc<Egl>) -> Self {
        // zero-initialize all function pointers
        let context: GenericGlContext = unsafe { mem::zeroed() };
        let opengl32_dll = Library::load("GL").ok();

        Self {
            _opengl32_dll_handle: opengl32_dll,
            egl,
            functions: Rc::new(context),
        }
    }

    // Assuming the OpenGL context is current, loads the OpenGL function pointers
    fn load(&mut self) {
        fn get_func(
            egl: &Egl,
            s: &'static str,
            opengl32_dll: &Option<Library>,
        ) -> *mut gl_context_loader::c_void {
            use std::ffi::CString;

            let symbol_name_new = match CString::new(s.as_bytes()).ok() {
                Some(s) => s,
                None => return ptr::null_mut(),
            };

            opengl32_dll
                .as_ref()
                .and_then(|l| l.get(s))
                .unwrap_or((egl.eglGetProcAddress)(symbol_name_new.as_ptr()))
                as *mut gl_context_loader::c_void
        }

        let egl = &*self.egl;
        self.functions = Rc::new(GenericGlContext {
            glAccum: get_func(&egl, "glAccum", &self._opengl32_dll_handle),
            glActiveTexture: get_func(&egl, "glActiveTexture", &self._opengl32_dll_handle),
            glAlphaFunc: get_func(&egl, "glAlphaFunc", &self._opengl32_dll_handle),
            glAreTexturesResident: get_func(
                &egl,
                "glAreTexturesResident",
                &self._opengl32_dll_handle,
            ),
            glArrayElement: get_func(&egl, "glArrayElement", &self._opengl32_dll_handle),
            glAttachShader: get_func(&egl, "glAttachShader", &self._opengl32_dll_handle),
            glBegin: get_func(&egl, "glBegin", &self._opengl32_dll_handle),
            glBeginConditionalRender: get_func(
                &egl,
                "glBeginConditionalRender",
                &self._opengl32_dll_handle,
            ),
            glBeginQuery: get_func(&egl, "glBeginQuery", &self._opengl32_dll_handle),
            glBeginTransformFeedback: get_func(
                &egl,
                "glBeginTransformFeedback",
                &self._opengl32_dll_handle,
            ),
            glBindAttribLocation: get_func(
                &egl,
                "glBindAttribLocation",
                &self._opengl32_dll_handle,
            ),
            glBindBuffer: get_func(&egl, "glBindBuffer", &self._opengl32_dll_handle),
            glBindBufferBase: get_func(&egl, "glBindBufferBase", &self._opengl32_dll_handle),
            glBindBufferRange: get_func(&egl, "glBindBufferRange", &self._opengl32_dll_handle),
            glBindFragDataLocation: get_func(
                &egl,
                "glBindFragDataLocation",
                &self._opengl32_dll_handle,
            ),
            glBindFragDataLocationIndexed: get_func(
                &egl,
                "glBindFragDataLocationIndexed",
                &self._opengl32_dll_handle,
            ),
            glBindFramebuffer: get_func(&egl, "glBindFramebuffer", &self._opengl32_dll_handle),
            glBindRenderbuffer: get_func(&egl, "glBindRenderbuffer", &self._opengl32_dll_handle),
            glBindSampler: get_func(&egl, "glBindSampler", &self._opengl32_dll_handle),
            glBindTexture: get_func(&egl, "glBindTexture", &self._opengl32_dll_handle),
            glBindVertexArray: get_func(&egl, "glBindVertexArray", &self._opengl32_dll_handle),
            glBindVertexArrayAPPLE: get_func(
                &egl,
                "glBindVertexArrayAPPLE",
                &self._opengl32_dll_handle,
            ),
            glBitmap: get_func(&egl, "glBitmap", &self._opengl32_dll_handle),
            glBlendBarrierKHR: get_func(&egl, "glBlendBarrierKHR", &self._opengl32_dll_handle),
            glBlendColor: get_func(&egl, "glBlendColor", &self._opengl32_dll_handle),
            glBlendEquation: get_func(&egl, "glBlendEquation", &self._opengl32_dll_handle),
            glBlendEquationSeparate: get_func(
                &egl,
                "glBlendEquationSeparate",
                &self._opengl32_dll_handle,
            ),
            glBlendFunc: get_func(&egl, "glBlendFunc", &self._opengl32_dll_handle),
            glBlendFuncSeparate: get_func(&egl, "glBlendFuncSeparate", &self._opengl32_dll_handle),
            glBlitFramebuffer: get_func(&egl, "glBlitFramebuffer", &self._opengl32_dll_handle),
            glBufferData: get_func(&egl, "glBufferData", &self._opengl32_dll_handle),
            glBufferStorage: get_func(&egl, "glBufferStorage", &self._opengl32_dll_handle),
            glBufferSubData: get_func(&egl, "glBufferSubData", &self._opengl32_dll_handle),
            glCallList: get_func(&egl, "glCallList", &self._opengl32_dll_handle),
            glCallLists: get_func(&egl, "glCallLists", &self._opengl32_dll_handle),
            glCheckFramebufferStatus: get_func(
                &egl,
                "glCheckFramebufferStatus",
                &self._opengl32_dll_handle,
            ),
            glClampColor: get_func(&egl, "glClampColor", &self._opengl32_dll_handle),
            glClear: get_func(&egl, "glClear", &self._opengl32_dll_handle),
            glClearAccum: get_func(&egl, "glClearAccum", &self._opengl32_dll_handle),
            glClearBufferfi: get_func(&egl, "glClearBufferfi", &self._opengl32_dll_handle),
            glClearBufferfv: get_func(&egl, "glClearBufferfv", &self._opengl32_dll_handle),
            glClearBufferiv: get_func(&egl, "glClearBufferiv", &self._opengl32_dll_handle),
            glClearBufferuiv: get_func(&egl, "glClearBufferuiv", &self._opengl32_dll_handle),
            glClearColor: get_func(&egl, "glClearColor", &self._opengl32_dll_handle),
            glClearDepth: get_func(&egl, "glClearDepth", &self._opengl32_dll_handle),
            glClearIndex: get_func(&egl, "glClearIndex", &self._opengl32_dll_handle),
            glClearStencil: get_func(&egl, "glClearStencil", &self._opengl32_dll_handle),
            glClientActiveTexture: get_func(
                &egl,
                "glClientActiveTexture",
                &self._opengl32_dll_handle,
            ),
            glClientWaitSync: get_func(&egl, "glClientWaitSync", &self._opengl32_dll_handle),
            glClipPlane: get_func(&egl, "glClipPlane", &self._opengl32_dll_handle),
            glColor3b: get_func(&egl, "glColor3b", &self._opengl32_dll_handle),
            glColor3bv: get_func(&egl, "glColor3bv", &self._opengl32_dll_handle),
            glColor3d: get_func(&egl, "glColor3d", &self._opengl32_dll_handle),
            glColor3dv: get_func(&egl, "glColor3dv", &self._opengl32_dll_handle),
            glColor3f: get_func(&egl, "glColor3f", &self._opengl32_dll_handle),
            glColor3fv: get_func(&egl, "glColor3fv", &self._opengl32_dll_handle),
            glColor3i: get_func(&egl, "glColor3i", &self._opengl32_dll_handle),
            glColor3iv: get_func(&egl, "glColor3iv", &self._opengl32_dll_handle),
            glColor3s: get_func(&egl, "glColor3s", &self._opengl32_dll_handle),
            glColor3sv: get_func(&egl, "glColor3sv", &self._opengl32_dll_handle),
            glColor3ub: get_func(&egl, "glColor3ub", &self._opengl32_dll_handle),
            glColor3ubv: get_func(&egl, "glColor3ubv", &self._opengl32_dll_handle),
            glColor3ui: get_func(&egl, "glColor3ui", &self._opengl32_dll_handle),
            glColor3uiv: get_func(&egl, "glColor3uiv", &self._opengl32_dll_handle),
            glColor3us: get_func(&egl, "glColor3us", &self._opengl32_dll_handle),
            glColor3usv: get_func(&egl, "glColor3usv", &self._opengl32_dll_handle),
            glColor4b: get_func(&egl, "glColor4b", &self._opengl32_dll_handle),
            glColor4bv: get_func(&egl, "glColor4bv", &self._opengl32_dll_handle),
            glColor4d: get_func(&egl, "glColor4d", &self._opengl32_dll_handle),
            glColor4dv: get_func(&egl, "glColor4dv", &self._opengl32_dll_handle),
            glColor4f: get_func(&egl, "glColor4f", &self._opengl32_dll_handle),
            glColor4fv: get_func(&egl, "glColor4fv", &self._opengl32_dll_handle),
            glColor4i: get_func(&egl, "glColor4i", &self._opengl32_dll_handle),
            glColor4iv: get_func(&egl, "glColor4iv", &self._opengl32_dll_handle),
            glColor4s: get_func(&egl, "glColor4s", &self._opengl32_dll_handle),
            glColor4sv: get_func(&egl, "glColor4sv", &self._opengl32_dll_handle),
            glColor4ub: get_func(&egl, "glColor4ub", &self._opengl32_dll_handle),
            glColor4ubv: get_func(&egl, "glColor4ubv", &self._opengl32_dll_handle),
            glColor4ui: get_func(&egl, "glColor4ui", &self._opengl32_dll_handle),
            glColor4uiv: get_func(&egl, "glColor4uiv", &self._opengl32_dll_handle),
            glColor4us: get_func(&egl, "glColor4us", &self._opengl32_dll_handle),
            glColor4usv: get_func(&egl, "glColor4usv", &self._opengl32_dll_handle),
            glColorMask: get_func(&egl, "glColorMask", &self._opengl32_dll_handle),
            glColorMaski: get_func(&egl, "glColorMaski", &self._opengl32_dll_handle),
            glColorMaterial: get_func(&egl, "glColorMaterial", &self._opengl32_dll_handle),
            glColorP3ui: get_func(&egl, "glColorP3ui", &self._opengl32_dll_handle),
            glColorP3uiv: get_func(&egl, "glColorP3uiv", &self._opengl32_dll_handle),
            glColorP4ui: get_func(&egl, "glColorP4ui", &self._opengl32_dll_handle),
            glColorP4uiv: get_func(&egl, "glColorP4uiv", &self._opengl32_dll_handle),
            glColorPointer: get_func(&egl, "glColorPointer", &self._opengl32_dll_handle),
            glCompileShader: get_func(&egl, "glCompileShader", &self._opengl32_dll_handle),
            glCompressedTexImage1D: get_func(
                &egl,
                "glCompressedTexImage1D",
                &self._opengl32_dll_handle,
            ),
            glCompressedTexImage2D: get_func(
                &egl,
                "glCompressedTexImage2D",
                &self._opengl32_dll_handle,
            ),
            glCompressedTexImage3D: get_func(
                &egl,
                "glCompressedTexImage3D",
                &self._opengl32_dll_handle,
            ),
            glCompressedTexSubImage1D: get_func(
                &egl,
                "glCompressedTexSubImage1D",
                &self._opengl32_dll_handle,
            ),
            glCompressedTexSubImage2D: get_func(
                &egl,
                "glCompressedTexSubImage2D",
                &self._opengl32_dll_handle,
            ),
            glCompressedTexSubImage3D: get_func(
                &egl,
                "glCompressedTexSubImage3D",
                &self._opengl32_dll_handle,
            ),
            glCopyBufferSubData: get_func(&egl, "glCopyBufferSubData", &self._opengl32_dll_handle),
            glCopyImageSubData: get_func(&egl, "glCopyImageSubData", &self._opengl32_dll_handle),
            glCopyPixels: get_func(&egl, "glCopyPixels", &self._opengl32_dll_handle),
            glCopyTexImage1D: get_func(&egl, "glCopyTexImage1D", &self._opengl32_dll_handle),
            glCopyTexImage2D: get_func(&egl, "glCopyTexImage2D", &self._opengl32_dll_handle),
            glCopyTexSubImage1D: get_func(&egl, "glCopyTexSubImage1D", &self._opengl32_dll_handle),
            glCopyTexSubImage2D: get_func(&egl, "glCopyTexSubImage2D", &self._opengl32_dll_handle),
            glCopyTexSubImage3D: get_func(&egl, "glCopyTexSubImage3D", &self._opengl32_dll_handle),
            glCreateProgram: get_func(&egl, "glCreateProgram", &self._opengl32_dll_handle),
            glCreateShader: get_func(&egl, "glCreateShader", &self._opengl32_dll_handle),
            glCullFace: get_func(&egl, "glCullFace", &self._opengl32_dll_handle),
            glDebugMessageCallback: get_func(
                &egl,
                "glDebugMessageCallback",
                &self._opengl32_dll_handle,
            ),
            glDebugMessageCallbackKHR: get_func(
                &egl,
                "glDebugMessageCallbackKHR",
                &self._opengl32_dll_handle,
            ),
            glDebugMessageControl: get_func(
                &egl,
                "glDebugMessageControl",
                &self._opengl32_dll_handle,
            ),
            glDebugMessageControlKHR: get_func(
                &egl,
                "glDebugMessageControlKHR",
                &self._opengl32_dll_handle,
            ),
            glDebugMessageInsert: get_func(
                &egl,
                "glDebugMessageInsert",
                &self._opengl32_dll_handle,
            ),
            glDebugMessageInsertKHR: get_func(
                &egl,
                "glDebugMessageInsertKHR",
                &self._opengl32_dll_handle,
            ),
            glDeleteBuffers: get_func(&egl, "glDeleteBuffers", &self._opengl32_dll_handle),
            glDeleteFencesAPPLE: get_func(&egl, "glDeleteFencesAPPLE", &self._opengl32_dll_handle),
            glDeleteFramebuffers: get_func(
                &egl,
                "glDeleteFramebuffers",
                &self._opengl32_dll_handle,
            ),
            glDeleteLists: get_func(&egl, "glDeleteLists", &self._opengl32_dll_handle),
            glDeleteProgram: get_func(&egl, "glDeleteProgram", &self._opengl32_dll_handle),
            glDeleteQueries: get_func(&egl, "glDeleteQueries", &self._opengl32_dll_handle),
            glDeleteRenderbuffers: get_func(
                &egl,
                "glDeleteRenderbuffers",
                &self._opengl32_dll_handle,
            ),
            glDeleteSamplers: get_func(&egl, "glDeleteSamplers", &self._opengl32_dll_handle),
            glDeleteShader: get_func(&egl, "glDeleteShader", &self._opengl32_dll_handle),
            glDeleteSync: get_func(&egl, "glDeleteSync", &self._opengl32_dll_handle),
            glDeleteTextures: get_func(&egl, "glDeleteTextures", &self._opengl32_dll_handle),
            glDeleteVertexArrays: get_func(
                &egl,
                "glDeleteVertexArrays",
                &self._opengl32_dll_handle,
            ),
            glDeleteVertexArraysAPPLE: get_func(
                &egl,
                "glDeleteVertexArraysAPPLE",
                &self._opengl32_dll_handle,
            ),
            glDepthFunc: get_func(&egl, "glDepthFunc", &self._opengl32_dll_handle),
            glDepthMask: get_func(&egl, "glDepthMask", &self._opengl32_dll_handle),
            glDepthRange: get_func(&egl, "glDepthRange", &self._opengl32_dll_handle),
            glDetachShader: get_func(&egl, "glDetachShader", &self._opengl32_dll_handle),
            glDisable: get_func(&egl, "glDisable", &self._opengl32_dll_handle),
            glDisableClientState: get_func(
                &egl,
                "glDisableClientState",
                &self._opengl32_dll_handle,
            ),
            glDisableVertexAttribArray: get_func(
                &egl,
                "glDisableVertexAttribArray",
                &self._opengl32_dll_handle,
            ),
            glDisablei: get_func(&egl, "glDisablei", &self._opengl32_dll_handle),
            glDrawArrays: get_func(&egl, "glDrawArrays", &self._opengl32_dll_handle),
            glDrawArraysInstanced: get_func(
                &egl,
                "glDrawArraysInstanced",
                &self._opengl32_dll_handle,
            ),
            glDrawBuffer: get_func(&egl, "glDrawBuffer", &self._opengl32_dll_handle),
            glDrawBuffers: get_func(&egl, "glDrawBuffers", &self._opengl32_dll_handle),
            glDrawElements: get_func(&egl, "glDrawElements", &self._opengl32_dll_handle),
            glDrawElementsBaseVertex: get_func(
                &egl,
                "glDrawElementsBaseVertex",
                &self._opengl32_dll_handle,
            ),
            glDrawElementsInstanced: get_func(
                &egl,
                "glDrawElementsInstanced",
                &self._opengl32_dll_handle,
            ),
            glDrawElementsInstancedBaseVertex: get_func(
                &egl,
                "glDrawElementsInstancedBaseVertex",
                &self._opengl32_dll_handle,
            ),
            glDrawPixels: get_func(&egl, "glDrawPixels", &self._opengl32_dll_handle),
            glDrawRangeElements: get_func(&egl, "glDrawRangeElements", &self._opengl32_dll_handle),
            glDrawRangeElementsBaseVertex: get_func(
                &egl,
                "glDrawRangeElementsBaseVertex",
                &self._opengl32_dll_handle,
            ),
            glEdgeFlag: get_func(&egl, "glEdgeFlag", &self._opengl32_dll_handle),
            glEdgeFlagPointer: get_func(&egl, "glEdgeFlagPointer", &self._opengl32_dll_handle),
            glEdgeFlagv: get_func(&egl, "glEdgeFlagv", &self._opengl32_dll_handle),
            glEnable: get_func(&egl, "glEnable", &self._opengl32_dll_handle),
            glEnableClientState: get_func(&egl, "glEnableClientState", &self._opengl32_dll_handle),
            glEnableVertexAttribArray: get_func(
                &egl,
                "glEnableVertexAttribArray",
                &self._opengl32_dll_handle,
            ),
            glEnablei: get_func(&egl, "glEnablei", &self._opengl32_dll_handle),
            glEnd: get_func(&egl, "glEnd", &self._opengl32_dll_handle),
            glEndConditionalRender: get_func(
                &egl,
                "glEndConditionalRender",
                &self._opengl32_dll_handle,
            ),
            glEndList: get_func(&egl, "glEndList", &self._opengl32_dll_handle),
            glEndQuery: get_func(&egl, "glEndQuery", &self._opengl32_dll_handle),
            glEndTransformFeedback: get_func(
                &egl,
                "glEndTransformFeedback",
                &self._opengl32_dll_handle,
            ),
            glEvalCoord1d: get_func(&egl, "glEvalCoord1d", &self._opengl32_dll_handle),
            glEvalCoord1dv: get_func(&egl, "glEvalCoord1dv", &self._opengl32_dll_handle),
            glEvalCoord1f: get_func(&egl, "glEvalCoord1f", &self._opengl32_dll_handle),
            glEvalCoord1fv: get_func(&egl, "glEvalCoord1fv", &self._opengl32_dll_handle),
            glEvalCoord2d: get_func(&egl, "glEvalCoord2d", &self._opengl32_dll_handle),
            glEvalCoord2dv: get_func(&egl, "glEvalCoord2dv", &self._opengl32_dll_handle),
            glEvalCoord2f: get_func(&egl, "glEvalCoord2f", &self._opengl32_dll_handle),
            glEvalCoord2fv: get_func(&egl, "glEvalCoord2fv", &self._opengl32_dll_handle),
            glEvalMesh1: get_func(&egl, "glEvalMesh1", &self._opengl32_dll_handle),
            glEvalMesh2: get_func(&egl, "glEvalMesh2", &self._opengl32_dll_handle),
            glEvalPoint1: get_func(&egl, "glEvalPoint1", &self._opengl32_dll_handle),
            glEvalPoint2: get_func(&egl, "glEvalPoint2", &self._opengl32_dll_handle),
            glFeedbackBuffer: get_func(&egl, "glFeedbackBuffer", &self._opengl32_dll_handle),
            glFenceSync: get_func(&egl, "glFenceSync", &self._opengl32_dll_handle),
            glFinish: get_func(&egl, "glFinish", &self._opengl32_dll_handle),
            glFinishFenceAPPLE: get_func(&egl, "glFinishFenceAPPLE", &self._opengl32_dll_handle),
            glFinishObjectAPPLE: get_func(&egl, "glFinishObjectAPPLE", &self._opengl32_dll_handle),
            glFlush: get_func(&egl, "glFlush", &self._opengl32_dll_handle),
            glFlushMappedBufferRange: get_func(
                &egl,
                "glFlushMappedBufferRange",
                &self._opengl32_dll_handle,
            ),
            glFogCoordPointer: get_func(&egl, "glFogCoordPointer", &self._opengl32_dll_handle),
            glFogCoordd: get_func(&egl, "glFogCoordd", &self._opengl32_dll_handle),
            glFogCoorddv: get_func(&egl, "glFogCoorddv", &self._opengl32_dll_handle),
            glFogCoordf: get_func(&egl, "glFogCoordf", &self._opengl32_dll_handle),
            glFogCoordfv: get_func(&egl, "glFogCoordfv", &self._opengl32_dll_handle),
            glFogf: get_func(&egl, "glFogf", &self._opengl32_dll_handle),
            glFogfv: get_func(&egl, "glFogfv", &self._opengl32_dll_handle),
            glFogi: get_func(&egl, "glFogi", &self._opengl32_dll_handle),
            glFogiv: get_func(&egl, "glFogiv", &self._opengl32_dll_handle),
            glFramebufferRenderbuffer: get_func(
                &egl,
                "glFramebufferRenderbuffer",
                &self._opengl32_dll_handle,
            ),
            glFramebufferTexture: get_func(
                &egl,
                "glFramebufferTexture",
                &self._opengl32_dll_handle,
            ),
            glFramebufferTexture1D: get_func(
                &egl,
                "glFramebufferTexture1D",
                &self._opengl32_dll_handle,
            ),
            glFramebufferTexture2D: get_func(
                &egl,
                "glFramebufferTexture2D",
                &self._opengl32_dll_handle,
            ),
            glFramebufferTexture3D: get_func(
                &egl,
                "glFramebufferTexture3D",
                &self._opengl32_dll_handle,
            ),
            glFramebufferTextureLayer: get_func(
                &egl,
                "glFramebufferTextureLayer",
                &self._opengl32_dll_handle,
            ),
            glFrontFace: get_func(&egl, "glFrontFace", &self._opengl32_dll_handle),
            glFrustum: get_func(&egl, "glFrustum", &self._opengl32_dll_handle),
            glGenBuffers: get_func(&egl, "glGenBuffers", &self._opengl32_dll_handle),
            glGenFencesAPPLE: get_func(&egl, "glGenFencesAPPLE", &self._opengl32_dll_handle),
            glGenFramebuffers: get_func(&egl, "glGenFramebuffers", &self._opengl32_dll_handle),
            glGenLists: get_func(&egl, "glGenLists", &self._opengl32_dll_handle),
            glGenQueries: get_func(&egl, "glGenQueries", &self._opengl32_dll_handle),
            glGenRenderbuffers: get_func(&egl, "glGenRenderbuffers", &self._opengl32_dll_handle),
            glGenSamplers: get_func(&egl, "glGenSamplers", &self._opengl32_dll_handle),
            glGenTextures: get_func(&egl, "glGenTextures", &self._opengl32_dll_handle),
            glGenVertexArrays: get_func(&egl, "glGenVertexArrays", &self._opengl32_dll_handle),
            glGenVertexArraysAPPLE: get_func(
                &egl,
                "glGenVertexArraysAPPLE",
                &self._opengl32_dll_handle,
            ),
            glGenerateMipmap: get_func(&egl, "glGenerateMipmap", &self._opengl32_dll_handle),
            glGetActiveAttrib: get_func(&egl, "glGetActiveAttrib", &self._opengl32_dll_handle),
            glGetActiveUniform: get_func(&egl, "glGetActiveUniform", &self._opengl32_dll_handle),
            glGetActiveUniformBlockName: get_func(
                &egl,
                "glGetActiveUniformBlockName",
                &self._opengl32_dll_handle,
            ),
            glGetActiveUniformBlockiv: get_func(
                &egl,
                "glGetActiveUniformBlockiv",
                &self._opengl32_dll_handle,
            ),
            glGetActiveUniformName: get_func(
                &egl,
                "glGetActiveUniformName",
                &self._opengl32_dll_handle,
            ),
            glGetActiveUniformsiv: get_func(
                &egl,
                "glGetActiveUniformsiv",
                &self._opengl32_dll_handle,
            ),
            glGetAttachedShaders: get_func(
                &egl,
                "glGetAttachedShaders",
                &self._opengl32_dll_handle,
            ),
            glGetAttribLocation: get_func(&egl, "glGetAttribLocation", &self._opengl32_dll_handle),
            glGetBooleani_v: get_func(&egl, "glGetBooleani_v", &self._opengl32_dll_handle),
            glGetBooleanv: get_func(&egl, "glGetBooleanv", &self._opengl32_dll_handle),
            glGetBufferParameteri64v: get_func(
                &egl,
                "glGetBufferParameteri64v",
                &self._opengl32_dll_handle,
            ),
            glGetBufferParameteriv: get_func(
                &egl,
                "glGetBufferParameteriv",
                &self._opengl32_dll_handle,
            ),
            glGetBufferPointerv: get_func(&egl, "glGetBufferPointerv", &self._opengl32_dll_handle),
            glGetBufferSubData: get_func(&egl, "glGetBufferSubData", &self._opengl32_dll_handle),
            glGetClipPlane: get_func(&egl, "glGetClipPlane", &self._opengl32_dll_handle),
            glGetCompressedTexImage: get_func(
                &egl,
                "glGetCompressedTexImage",
                &self._opengl32_dll_handle,
            ),
            glGetDebugMessageLog: get_func(
                &egl,
                "glGetDebugMessageLog",
                &self._opengl32_dll_handle,
            ),
            glGetDebugMessageLogKHR: get_func(
                &egl,
                "glGetDebugMessageLogKHR",
                &self._opengl32_dll_handle,
            ),
            glGetDoublev: get_func(&egl, "glGetDoublev", &self._opengl32_dll_handle),
            glGetError: get_func(&egl, "glGetError", &self._opengl32_dll_handle),
            glGetFloatv: get_func(&egl, "glGetFloatv", &self._opengl32_dll_handle),
            glGetFragDataIndex: get_func(&egl, "glGetFragDataIndex", &self._opengl32_dll_handle),
            glGetFragDataLocation: get_func(
                &egl,
                "glGetFragDataLocation",
                &self._opengl32_dll_handle,
            ),
            glGetFramebufferAttachmentParameteriv: get_func(
                &egl,
                "glGetFramebufferAttachmentParameteriv",
                &self._opengl32_dll_handle,
            ),
            glGetInteger64i_v: get_func(&egl, "glGetInteger64i_v", &self._opengl32_dll_handle),
            glGetInteger64v: get_func(&egl, "glGetInteger64v", &self._opengl32_dll_handle),
            glGetIntegeri_v: get_func(&egl, "glGetIntegeri_v", &self._opengl32_dll_handle),
            glGetIntegerv: get_func(&egl, "glGetIntegerv", &self._opengl32_dll_handle),
            glGetLightfv: get_func(&egl, "glGetLightfv", &self._opengl32_dll_handle),
            glGetLightiv: get_func(&egl, "glGetLightiv", &self._opengl32_dll_handle),
            glGetMapdv: get_func(&egl, "glGetMapdv", &self._opengl32_dll_handle),
            glGetMapfv: get_func(&egl, "glGetMapfv", &self._opengl32_dll_handle),
            glGetMapiv: get_func(&egl, "glGetMapiv", &self._opengl32_dll_handle),
            glGetMaterialfv: get_func(&egl, "glGetMaterialfv", &self._opengl32_dll_handle),
            glGetMaterialiv: get_func(&egl, "glGetMaterialiv", &self._opengl32_dll_handle),
            glGetMultisamplefv: get_func(&egl, "glGetMultisamplefv", &self._opengl32_dll_handle),
            glGetObjectLabel: get_func(&egl, "glGetObjectLabel", &self._opengl32_dll_handle),
            glGetObjectLabelKHR: get_func(&egl, "glGetObjectLabelKHR", &self._opengl32_dll_handle),
            glGetObjectPtrLabel: get_func(&egl, "glGetObjectPtrLabel", &self._opengl32_dll_handle),
            glGetObjectPtrLabelKHR: get_func(
                &egl,
                "glGetObjectPtrLabelKHR",
                &self._opengl32_dll_handle,
            ),
            glGetPixelMapfv: get_func(&egl, "glGetPixelMapfv", &self._opengl32_dll_handle),
            glGetPixelMapuiv: get_func(&egl, "glGetPixelMapuiv", &self._opengl32_dll_handle),
            glGetPixelMapusv: get_func(&egl, "glGetPixelMapusv", &self._opengl32_dll_handle),
            glGetPointerv: get_func(&egl, "glGetPointerv", &self._opengl32_dll_handle),
            glGetPointervKHR: get_func(&egl, "glGetPointervKHR", &self._opengl32_dll_handle),
            glGetPolygonStipple: get_func(&egl, "glGetPolygonStipple", &self._opengl32_dll_handle),
            glGetProgramBinary: get_func(&egl, "glGetProgramBinary", &self._opengl32_dll_handle),
            glGetProgramInfoLog: get_func(&egl, "glGetProgramInfoLog", &self._opengl32_dll_handle),
            glGetProgramiv: get_func(&egl, "glGetProgramiv", &self._opengl32_dll_handle),
            glGetQueryObjecti64v: get_func(
                &egl,
                "glGetQueryObjecti64v",
                &self._opengl32_dll_handle,
            ),
            glGetQueryObjectiv: get_func(&egl, "glGetQueryObjectiv", &self._opengl32_dll_handle),
            glGetQueryObjectui64v: get_func(
                &egl,
                "glGetQueryObjectui64v",
                &self._opengl32_dll_handle,
            ),
            glGetQueryObjectuiv: get_func(&egl, "glGetQueryObjectuiv", &self._opengl32_dll_handle),
            glGetQueryiv: get_func(&egl, "glGetQueryiv", &self._opengl32_dll_handle),
            glGetRenderbufferParameteriv: get_func(
                &egl,
                "glGetRenderbufferParameteriv",
                &self._opengl32_dll_handle,
            ),
            glGetSamplerParameterIiv: get_func(
                &egl,
                "glGetSamplerParameterIiv",
                &self._opengl32_dll_handle,
            ),
            glGetSamplerParameterIuiv: get_func(
                &egl,
                "glGetSamplerParameterIuiv",
                &self._opengl32_dll_handle,
            ),
            glGetSamplerParameterfv: get_func(
                &egl,
                "glGetSamplerParameterfv",
                &self._opengl32_dll_handle,
            ),
            glGetSamplerParameteriv: get_func(
                &egl,
                "glGetSamplerParameteriv",
                &self._opengl32_dll_handle,
            ),
            glGetShaderInfoLog: get_func(&egl, "glGetShaderInfoLog", &self._opengl32_dll_handle),
            glGetShaderSource: get_func(&egl, "glGetShaderSource", &self._opengl32_dll_handle),
            glGetShaderiv: get_func(&egl, "glGetShaderiv", &self._opengl32_dll_handle),
            glGetString: get_func(&egl, "glGetString", &self._opengl32_dll_handle),
            glGetStringi: get_func(&egl, "glGetStringi", &self._opengl32_dll_handle),
            glGetSynciv: get_func(&egl, "glGetSynciv", &self._opengl32_dll_handle),
            glGetTexEnvfv: get_func(&egl, "glGetTexEnvfv", &self._opengl32_dll_handle),
            glGetTexEnviv: get_func(&egl, "glGetTexEnviv", &self._opengl32_dll_handle),
            glGetTexGendv: get_func(&egl, "glGetTexGendv", &self._opengl32_dll_handle),
            glGetTexGenfv: get_func(&egl, "glGetTexGenfv", &self._opengl32_dll_handle),
            glGetTexGeniv: get_func(&egl, "glGetTexGeniv", &self._opengl32_dll_handle),
            glGetTexImage: get_func(&egl, "glGetTexImage", &self._opengl32_dll_handle),
            glGetTexLevelParameterfv: get_func(
                &egl,
                "glGetTexLevelParameterfv",
                &self._opengl32_dll_handle,
            ),
            glGetTexLevelParameteriv: get_func(
                &egl,
                "glGetTexLevelParameteriv",
                &self._opengl32_dll_handle,
            ),
            glGetTexParameterIiv: get_func(
                &egl,
                "glGetTexParameterIiv",
                &self._opengl32_dll_handle,
            ),
            glGetTexParameterIuiv: get_func(
                &egl,
                "glGetTexParameterIuiv",
                &self._opengl32_dll_handle,
            ),
            glGetTexParameterPointervAPPLE: get_func(
                &egl,
                "glGetTexParameterPointervAPPLE",
                &self._opengl32_dll_handle,
            ),
            glGetTexParameterfv: get_func(&egl, "glGetTexParameterfv", &self._opengl32_dll_handle),
            glGetTexParameteriv: get_func(&egl, "glGetTexParameteriv", &self._opengl32_dll_handle),
            glGetTransformFeedbackVarying: get_func(
                &egl,
                "glGetTransformFeedbackVarying",
                &self._opengl32_dll_handle,
            ),
            glGetUniformBlockIndex: get_func(
                &egl,
                "glGetUniformBlockIndex",
                &self._opengl32_dll_handle,
            ),
            glGetUniformIndices: get_func(&egl, "glGetUniformIndices", &self._opengl32_dll_handle),
            glGetUniformLocation: get_func(
                &egl,
                "glGetUniformLocation",
                &self._opengl32_dll_handle,
            ),
            glGetUniformfv: get_func(&egl, "glGetUniformfv", &self._opengl32_dll_handle),
            glGetUniformiv: get_func(&egl, "glGetUniformiv", &self._opengl32_dll_handle),
            glGetUniformuiv: get_func(&egl, "glGetUniformuiv", &self._opengl32_dll_handle),
            glGetVertexAttribIiv: get_func(
                &egl,
                "glGetVertexAttribIiv",
                &self._opengl32_dll_handle,
            ),
            glGetVertexAttribIuiv: get_func(
                &egl,
                "glGetVertexAttribIuiv",
                &self._opengl32_dll_handle,
            ),
            glGetVertexAttribPointerv: get_func(
                &egl,
                "glGetVertexAttribPointerv",
                &self._opengl32_dll_handle,
            ),
            glGetVertexAttribdv: get_func(&egl, "glGetVertexAttribdv", &self._opengl32_dll_handle),
            glGetVertexAttribfv: get_func(&egl, "glGetVertexAttribfv", &self._opengl32_dll_handle),
            glGetVertexAttribiv: get_func(&egl, "glGetVertexAttribiv", &self._opengl32_dll_handle),
            glHint: get_func(&egl, "glHint", &self._opengl32_dll_handle),
            glIndexMask: get_func(&egl, "glIndexMask", &self._opengl32_dll_handle),
            glIndexPointer: get_func(&egl, "glIndexPointer", &self._opengl32_dll_handle),
            glIndexd: get_func(&egl, "glIndexd", &self._opengl32_dll_handle),
            glIndexdv: get_func(&egl, "glIndexdv", &self._opengl32_dll_handle),
            glIndexf: get_func(&egl, "glIndexf", &self._opengl32_dll_handle),
            glIndexfv: get_func(&egl, "glIndexfv", &self._opengl32_dll_handle),
            glIndexi: get_func(&egl, "glIndexi", &self._opengl32_dll_handle),
            glIndexiv: get_func(&egl, "glIndexiv", &self._opengl32_dll_handle),
            glIndexs: get_func(&egl, "glIndexs", &self._opengl32_dll_handle),
            glIndexsv: get_func(&egl, "glIndexsv", &self._opengl32_dll_handle),
            glIndexub: get_func(&egl, "glIndexub", &self._opengl32_dll_handle),
            glIndexubv: get_func(&egl, "glIndexubv", &self._opengl32_dll_handle),
            glInitNames: get_func(&egl, "glInitNames", &self._opengl32_dll_handle),
            glInsertEventMarkerEXT: get_func(
                &egl,
                "glInsertEventMarkerEXT",
                &self._opengl32_dll_handle,
            ),
            glInterleavedArrays: get_func(&egl, "glInterleavedArrays", &self._opengl32_dll_handle),
            glInvalidateBufferData: get_func(
                &egl,
                "glInvalidateBufferData",
                &self._opengl32_dll_handle,
            ),
            glInvalidateBufferSubData: get_func(
                &egl,
                "glInvalidateBufferSubData",
                &self._opengl32_dll_handle,
            ),
            glInvalidateFramebuffer: get_func(
                &egl,
                "glInvalidateFramebuffer",
                &self._opengl32_dll_handle,
            ),
            glInvalidateSubFramebuffer: get_func(
                &egl,
                "glInvalidateSubFramebuffer",
                &self._opengl32_dll_handle,
            ),
            glInvalidateTexImage: get_func(
                &egl,
                "glInvalidateTexImage",
                &self._opengl32_dll_handle,
            ),
            glInvalidateTexSubImage: get_func(
                &egl,
                "glInvalidateTexSubImage",
                &self._opengl32_dll_handle,
            ),
            glIsBuffer: get_func(&egl, "glIsBuffer", &self._opengl32_dll_handle),
            glIsEnabled: get_func(&egl, "glIsEnabled", &self._opengl32_dll_handle),
            glIsEnabledi: get_func(&egl, "glIsEnabledi", &self._opengl32_dll_handle),
            glIsFenceAPPLE: get_func(&egl, "glIsFenceAPPLE", &self._opengl32_dll_handle),
            glIsFramebuffer: get_func(&egl, "glIsFramebuffer", &self._opengl32_dll_handle),
            glIsList: get_func(&egl, "glIsList", &self._opengl32_dll_handle),
            glIsProgram: get_func(&egl, "glIsProgram", &self._opengl32_dll_handle),
            glIsQuery: get_func(&egl, "glIsQuery", &self._opengl32_dll_handle),
            glIsRenderbuffer: get_func(&egl, "glIsRenderbuffer", &self._opengl32_dll_handle),
            glIsSampler: get_func(&egl, "glIsSampler", &self._opengl32_dll_handle),
            glIsShader: get_func(&egl, "glIsShader", &self._opengl32_dll_handle),
            glIsSync: get_func(&egl, "glIsSync", &self._opengl32_dll_handle),
            glIsTexture: get_func(&egl, "glIsTexture", &self._opengl32_dll_handle),
            glIsVertexArray: get_func(&egl, "glIsVertexArray", &self._opengl32_dll_handle),
            glIsVertexArrayAPPLE: get_func(
                &egl,
                "glIsVertexArrayAPPLE",
                &self._opengl32_dll_handle,
            ),
            glLightModelf: get_func(&egl, "glLightModelf", &self._opengl32_dll_handle),
            glLightModelfv: get_func(&egl, "glLightModelfv", &self._opengl32_dll_handle),
            glLightModeli: get_func(&egl, "glLightModeli", &self._opengl32_dll_handle),
            glLightModeliv: get_func(&egl, "glLightModeliv", &self._opengl32_dll_handle),
            glLightf: get_func(&egl, "glLightf", &self._opengl32_dll_handle),
            glLightfv: get_func(&egl, "glLightfv", &self._opengl32_dll_handle),
            glLighti: get_func(&egl, "glLighti", &self._opengl32_dll_handle),
            glLightiv: get_func(&egl, "glLightiv", &self._opengl32_dll_handle),
            glLineStipple: get_func(&egl, "glLineStipple", &self._opengl32_dll_handle),
            glLineWidth: get_func(&egl, "glLineWidth", &self._opengl32_dll_handle),
            glLinkProgram: get_func(&egl, "glLinkProgram", &self._opengl32_dll_handle),
            glListBase: get_func(&egl, "glListBase", &self._opengl32_dll_handle),
            glLoadIdentity: get_func(&egl, "glLoadIdentity", &self._opengl32_dll_handle),
            glLoadMatrixd: get_func(&egl, "glLoadMatrixd", &self._opengl32_dll_handle),
            glLoadMatrixf: get_func(&egl, "glLoadMatrixf", &self._opengl32_dll_handle),
            glLoadName: get_func(&egl, "glLoadName", &self._opengl32_dll_handle),
            glLoadTransposeMatrixd: get_func(
                &egl,
                "glLoadTransposeMatrixd",
                &self._opengl32_dll_handle,
            ),
            glLoadTransposeMatrixf: get_func(
                &egl,
                "glLoadTransposeMatrixf",
                &self._opengl32_dll_handle,
            ),
            glLogicOp: get_func(&egl, "glLogicOp", &self._opengl32_dll_handle),
            glMap1d: get_func(&egl, "glMap1d", &self._opengl32_dll_handle),
            glMap1f: get_func(&egl, "glMap1f", &self._opengl32_dll_handle),
            glMap2d: get_func(&egl, "glMap2d", &self._opengl32_dll_handle),
            glMap2f: get_func(&egl, "glMap2f", &self._opengl32_dll_handle),
            glMapBuffer: get_func(&egl, "glMapBuffer", &self._opengl32_dll_handle),
            glMapBufferRange: get_func(&egl, "glMapBufferRange", &self._opengl32_dll_handle),
            glMapGrid1d: get_func(&egl, "glMapGrid1d", &self._opengl32_dll_handle),
            glMapGrid1f: get_func(&egl, "glMapGrid1f", &self._opengl32_dll_handle),
            glMapGrid2d: get_func(&egl, "glMapGrid2d", &self._opengl32_dll_handle),
            glMapGrid2f: get_func(&egl, "glMapGrid2f", &self._opengl32_dll_handle),
            glMaterialf: get_func(&egl, "glMaterialf", &self._opengl32_dll_handle),
            glMaterialfv: get_func(&egl, "glMaterialfv", &self._opengl32_dll_handle),
            glMateriali: get_func(&egl, "glMateriali", &self._opengl32_dll_handle),
            glMaterialiv: get_func(&egl, "glMaterialiv", &self._opengl32_dll_handle),
            glMatrixMode: get_func(&egl, "glMatrixMode", &self._opengl32_dll_handle),
            glMultMatrixd: get_func(&egl, "glMultMatrixd", &self._opengl32_dll_handle),
            glMultMatrixf: get_func(&egl, "glMultMatrixf", &self._opengl32_dll_handle),
            glMultTransposeMatrixd: get_func(
                &egl,
                "glMultTransposeMatrixd",
                &self._opengl32_dll_handle,
            ),
            glMultTransposeMatrixf: get_func(
                &egl,
                "glMultTransposeMatrixf",
                &self._opengl32_dll_handle,
            ),
            glMultiDrawArrays: get_func(&egl, "glMultiDrawArrays", &self._opengl32_dll_handle),
            glMultiDrawElements: get_func(&egl, "glMultiDrawElements", &self._opengl32_dll_handle),
            glMultiDrawElementsBaseVertex: get_func(
                &egl,
                "glMultiDrawElementsBaseVertex",
                &self._opengl32_dll_handle,
            ),
            glMultiTexCoord1d: get_func(&egl, "glMultiTexCoord1d", &self._opengl32_dll_handle),
            glMultiTexCoord1dv: get_func(&egl, "glMultiTexCoord1dv", &self._opengl32_dll_handle),
            glMultiTexCoord1f: get_func(&egl, "glMultiTexCoord1f", &self._opengl32_dll_handle),
            glMultiTexCoord1fv: get_func(&egl, "glMultiTexCoord1fv", &self._opengl32_dll_handle),
            glMultiTexCoord1i: get_func(&egl, "glMultiTexCoord1i", &self._opengl32_dll_handle),
            glMultiTexCoord1iv: get_func(&egl, "glMultiTexCoord1iv", &self._opengl32_dll_handle),
            glMultiTexCoord1s: get_func(&egl, "glMultiTexCoord1s", &self._opengl32_dll_handle),
            glMultiTexCoord1sv: get_func(&egl, "glMultiTexCoord1sv", &self._opengl32_dll_handle),
            glMultiTexCoord2d: get_func(&egl, "glMultiTexCoord2d", &self._opengl32_dll_handle),
            glMultiTexCoord2dv: get_func(&egl, "glMultiTexCoord2dv", &self._opengl32_dll_handle),
            glMultiTexCoord2f: get_func(&egl, "glMultiTexCoord2f", &self._opengl32_dll_handle),
            glMultiTexCoord2fv: get_func(&egl, "glMultiTexCoord2fv", &self._opengl32_dll_handle),
            glMultiTexCoord2i: get_func(&egl, "glMultiTexCoord2i", &self._opengl32_dll_handle),
            glMultiTexCoord2iv: get_func(&egl, "glMultiTexCoord2iv", &self._opengl32_dll_handle),
            glMultiTexCoord2s: get_func(&egl, "glMultiTexCoord2s", &self._opengl32_dll_handle),
            glMultiTexCoord2sv: get_func(&egl, "glMultiTexCoord2sv", &self._opengl32_dll_handle),
            glMultiTexCoord3d: get_func(&egl, "glMultiTexCoord3d", &self._opengl32_dll_handle),
            glMultiTexCoord3dv: get_func(&egl, "glMultiTexCoord3dv", &self._opengl32_dll_handle),
            glMultiTexCoord3f: get_func(&egl, "glMultiTexCoord3f", &self._opengl32_dll_handle),
            glMultiTexCoord3fv: get_func(&egl, "glMultiTexCoord3fv", &self._opengl32_dll_handle),
            glMultiTexCoord3i: get_func(&egl, "glMultiTexCoord3i", &self._opengl32_dll_handle),
            glMultiTexCoord3iv: get_func(&egl, "glMultiTexCoord3iv", &self._opengl32_dll_handle),
            glMultiTexCoord3s: get_func(&egl, "glMultiTexCoord3s", &self._opengl32_dll_handle),
            glMultiTexCoord3sv: get_func(&egl, "glMultiTexCoord3sv", &self._opengl32_dll_handle),
            glMultiTexCoord4d: get_func(&egl, "glMultiTexCoord4d", &self._opengl32_dll_handle),
            glMultiTexCoord4dv: get_func(&egl, "glMultiTexCoord4dv", &self._opengl32_dll_handle),
            glMultiTexCoord4f: get_func(&egl, "glMultiTexCoord4f", &self._opengl32_dll_handle),
            glMultiTexCoord4fv: get_func(&egl, "glMultiTexCoord4fv", &self._opengl32_dll_handle),
            glMultiTexCoord4i: get_func(&egl, "glMultiTexCoord4i", &self._opengl32_dll_handle),
            glMultiTexCoord4iv: get_func(&egl, "glMultiTexCoord4iv", &self._opengl32_dll_handle),
            glMultiTexCoord4s: get_func(&egl, "glMultiTexCoord4s", &self._opengl32_dll_handle),
            glMultiTexCoord4sv: get_func(&egl, "glMultiTexCoord4sv", &self._opengl32_dll_handle),
            glMultiTexCoordP1ui: get_func(&egl, "glMultiTexCoordP1ui", &self._opengl32_dll_handle),
            glMultiTexCoordP1uiv: get_func(
                &egl,
                "glMultiTexCoordP1uiv",
                &self._opengl32_dll_handle,
            ),
            glMultiTexCoordP2ui: get_func(&egl, "glMultiTexCoordP2ui", &self._opengl32_dll_handle),
            glMultiTexCoordP2uiv: get_func(
                &egl,
                "glMultiTexCoordP2uiv",
                &self._opengl32_dll_handle,
            ),
            glMultiTexCoordP3ui: get_func(&egl, "glMultiTexCoordP3ui", &self._opengl32_dll_handle),
            glMultiTexCoordP3uiv: get_func(
                &egl,
                "glMultiTexCoordP3uiv",
                &self._opengl32_dll_handle,
            ),
            glMultiTexCoordP4ui: get_func(&egl, "glMultiTexCoordP4ui", &self._opengl32_dll_handle),
            glMultiTexCoordP4uiv: get_func(
                &egl,
                "glMultiTexCoordP4uiv",
                &self._opengl32_dll_handle,
            ),
            glNewList: get_func(&egl, "glNewList", &self._opengl32_dll_handle),
            glNormal3b: get_func(&egl, "glNormal3b", &self._opengl32_dll_handle),
            glNormal3bv: get_func(&egl, "glNormal3bv", &self._opengl32_dll_handle),
            glNormal3d: get_func(&egl, "glNormal3d", &self._opengl32_dll_handle),
            glNormal3dv: get_func(&egl, "glNormal3dv", &self._opengl32_dll_handle),
            glNormal3f: get_func(&egl, "glNormal3f", &self._opengl32_dll_handle),
            glNormal3fv: get_func(&egl, "glNormal3fv", &self._opengl32_dll_handle),
            glNormal3i: get_func(&egl, "glNormal3i", &self._opengl32_dll_handle),
            glNormal3iv: get_func(&egl, "glNormal3iv", &self._opengl32_dll_handle),
            glNormal3s: get_func(&egl, "glNormal3s", &self._opengl32_dll_handle),
            glNormal3sv: get_func(&egl, "glNormal3sv", &self._opengl32_dll_handle),
            glNormalP3ui: get_func(&egl, "glNormalP3ui", &self._opengl32_dll_handle),
            glNormalP3uiv: get_func(&egl, "glNormalP3uiv", &self._opengl32_dll_handle),
            glNormalPointer: get_func(&egl, "glNormalPointer", &self._opengl32_dll_handle),
            glObjectLabel: get_func(&egl, "glObjectLabel", &self._opengl32_dll_handle),
            glObjectLabelKHR: get_func(&egl, "glObjectLabelKHR", &self._opengl32_dll_handle),
            glObjectPtrLabel: get_func(&egl, "glObjectPtrLabel", &self._opengl32_dll_handle),
            glObjectPtrLabelKHR: get_func(&egl, "glObjectPtrLabelKHR", &self._opengl32_dll_handle),
            glOrtho: get_func(&egl, "glOrtho", &self._opengl32_dll_handle),
            glPassThrough: get_func(&egl, "glPassThrough", &self._opengl32_dll_handle),
            glPixelMapfv: get_func(&egl, "glPixelMapfv", &self._opengl32_dll_handle),
            glPixelMapuiv: get_func(&egl, "glPixelMapuiv", &self._opengl32_dll_handle),
            glPixelMapusv: get_func(&egl, "glPixelMapusv", &self._opengl32_dll_handle),
            glPixelStoref: get_func(&egl, "glPixelStoref", &self._opengl32_dll_handle),
            glPixelStorei: get_func(&egl, "glPixelStorei", &self._opengl32_dll_handle),
            glPixelTransferf: get_func(&egl, "glPixelTransferf", &self._opengl32_dll_handle),
            glPixelTransferi: get_func(&egl, "glPixelTransferi", &self._opengl32_dll_handle),
            glPixelZoom: get_func(&egl, "glPixelZoom", &self._opengl32_dll_handle),
            glPointParameterf: get_func(&egl, "glPointParameterf", &self._opengl32_dll_handle),
            glPointParameterfv: get_func(&egl, "glPointParameterfv", &self._opengl32_dll_handle),
            glPointParameteri: get_func(&egl, "glPointParameteri", &self._opengl32_dll_handle),
            glPointParameteriv: get_func(&egl, "glPointParameteriv", &self._opengl32_dll_handle),
            glPointSize: get_func(&egl, "glPointSize", &self._opengl32_dll_handle),
            glPolygonMode: get_func(&egl, "glPolygonMode", &self._opengl32_dll_handle),
            glPolygonOffset: get_func(&egl, "glPolygonOffset", &self._opengl32_dll_handle),
            glPolygonStipple: get_func(&egl, "glPolygonStipple", &self._opengl32_dll_handle),
            glPopAttrib: get_func(&egl, "glPopAttrib", &self._opengl32_dll_handle),
            glPopClientAttrib: get_func(&egl, "glPopClientAttrib", &self._opengl32_dll_handle),
            glPopDebugGroup: get_func(&egl, "glPopDebugGroup", &self._opengl32_dll_handle),
            glPopDebugGroupKHR: get_func(&egl, "glPopDebugGroupKHR", &self._opengl32_dll_handle),
            glPopGroupMarkerEXT: get_func(&egl, "glPopGroupMarkerEXT", &self._opengl32_dll_handle),
            glPopMatrix: get_func(&egl, "glPopMatrix", &self._opengl32_dll_handle),
            glPopName: get_func(&egl, "glPopName", &self._opengl32_dll_handle),
            glPrimitiveRestartIndex: get_func(
                &egl,
                "glPrimitiveRestartIndex",
                &self._opengl32_dll_handle,
            ),
            glPrioritizeTextures: get_func(
                &egl,
                "glPrioritizeTextures",
                &self._opengl32_dll_handle,
            ),
            glProgramBinary: get_func(&egl, "glProgramBinary", &self._opengl32_dll_handle),
            glProgramParameteri: get_func(&egl, "glProgramParameteri", &self._opengl32_dll_handle),
            glProvokingVertex: get_func(&egl, "glProvokingVertex", &self._opengl32_dll_handle),
            glPushAttrib: get_func(&egl, "glPushAttrib", &self._opengl32_dll_handle),
            glPushClientAttrib: get_func(&egl, "glPushClientAttrib", &self._opengl32_dll_handle),
            glPushDebugGroup: get_func(&egl, "glPushDebugGroup", &self._opengl32_dll_handle),
            glPushDebugGroupKHR: get_func(&egl, "glPushDebugGroupKHR", &self._opengl32_dll_handle),
            glPushGroupMarkerEXT: get_func(
                &egl,
                "glPushGroupMarkerEXT",
                &self._opengl32_dll_handle,
            ),
            glPushMatrix: get_func(&egl, "glPushMatrix", &self._opengl32_dll_handle),
            glPushName: get_func(&egl, "glPushName", &self._opengl32_dll_handle),
            glQueryCounter: get_func(&egl, "glQueryCounter", &self._opengl32_dll_handle),
            glRasterPos2d: get_func(&egl, "glRasterPos2d", &self._opengl32_dll_handle),
            glRasterPos2dv: get_func(&egl, "glRasterPos2dv", &self._opengl32_dll_handle),
            glRasterPos2f: get_func(&egl, "glRasterPos2f", &self._opengl32_dll_handle),
            glRasterPos2fv: get_func(&egl, "glRasterPos2fv", &self._opengl32_dll_handle),
            glRasterPos2i: get_func(&egl, "glRasterPos2i", &self._opengl32_dll_handle),
            glRasterPos2iv: get_func(&egl, "glRasterPos2iv", &self._opengl32_dll_handle),
            glRasterPos2s: get_func(&egl, "glRasterPos2s", &self._opengl32_dll_handle),
            glRasterPos2sv: get_func(&egl, "glRasterPos2sv", &self._opengl32_dll_handle),
            glRasterPos3d: get_func(&egl, "glRasterPos3d", &self._opengl32_dll_handle),
            glRasterPos3dv: get_func(&egl, "glRasterPos3dv", &self._opengl32_dll_handle),
            glRasterPos3f: get_func(&egl, "glRasterPos3f", &self._opengl32_dll_handle),
            glRasterPos3fv: get_func(&egl, "glRasterPos3fv", &self._opengl32_dll_handle),
            glRasterPos3i: get_func(&egl, "glRasterPos3i", &self._opengl32_dll_handle),
            glRasterPos3iv: get_func(&egl, "glRasterPos3iv", &self._opengl32_dll_handle),
            glRasterPos3s: get_func(&egl, "glRasterPos3s", &self._opengl32_dll_handle),
            glRasterPos3sv: get_func(&egl, "glRasterPos3sv", &self._opengl32_dll_handle),
            glRasterPos4d: get_func(&egl, "glRasterPos4d", &self._opengl32_dll_handle),
            glRasterPos4dv: get_func(&egl, "glRasterPos4dv", &self._opengl32_dll_handle),
            glRasterPos4f: get_func(&egl, "glRasterPos4f", &self._opengl32_dll_handle),
            glRasterPos4fv: get_func(&egl, "glRasterPos4fv", &self._opengl32_dll_handle),
            glRasterPos4i: get_func(&egl, "glRasterPos4i", &self._opengl32_dll_handle),
            glRasterPos4iv: get_func(&egl, "glRasterPos4iv", &self._opengl32_dll_handle),
            glRasterPos4s: get_func(&egl, "glRasterPos4s", &self._opengl32_dll_handle),
            glRasterPos4sv: get_func(&egl, "glRasterPos4sv", &self._opengl32_dll_handle),
            glReadBuffer: get_func(&egl, "glReadBuffer", &self._opengl32_dll_handle),
            glReadPixels: get_func(&egl, "glReadPixels", &self._opengl32_dll_handle),
            glRectd: get_func(&egl, "glRectd", &self._opengl32_dll_handle),
            glRectdv: get_func(&egl, "glRectdv", &self._opengl32_dll_handle),
            glRectf: get_func(&egl, "glRectf", &self._opengl32_dll_handle),
            glRectfv: get_func(&egl, "glRectfv", &self._opengl32_dll_handle),
            glRecti: get_func(&egl, "glRecti", &self._opengl32_dll_handle),
            glRectiv: get_func(&egl, "glRectiv", &self._opengl32_dll_handle),
            glRects: get_func(&egl, "glRects", &self._opengl32_dll_handle),
            glRectsv: get_func(&egl, "glRectsv", &self._opengl32_dll_handle),
            glRenderMode: get_func(&egl, "glRenderMode", &self._opengl32_dll_handle),
            glRenderbufferStorage: get_func(
                &egl,
                "glRenderbufferStorage",
                &self._opengl32_dll_handle,
            ),
            glRenderbufferStorageMultisample: get_func(
                &egl,
                "glRenderbufferStorageMultisample",
                &self._opengl32_dll_handle,
            ),
            glRotated: get_func(&egl, "glRotated", &self._opengl32_dll_handle),
            glRotatef: get_func(&egl, "glRotatef", &self._opengl32_dll_handle),
            glSampleCoverage: get_func(&egl, "glSampleCoverage", &self._opengl32_dll_handle),
            glSampleMaski: get_func(&egl, "glSampleMaski", &self._opengl32_dll_handle),
            glSamplerParameterIiv: get_func(
                &egl,
                "glSamplerParameterIiv",
                &self._opengl32_dll_handle,
            ),
            glSamplerParameterIuiv: get_func(
                &egl,
                "glSamplerParameterIuiv",
                &self._opengl32_dll_handle,
            ),
            glSamplerParameterf: get_func(&egl, "glSamplerParameterf", &self._opengl32_dll_handle),
            glSamplerParameterfv: get_func(
                &egl,
                "glSamplerParameterfv",
                &self._opengl32_dll_handle,
            ),
            glSamplerParameteri: get_func(&egl, "glSamplerParameteri", &self._opengl32_dll_handle),
            glSamplerParameteriv: get_func(
                &egl,
                "glSamplerParameteriv",
                &self._opengl32_dll_handle,
            ),
            glScaled: get_func(&egl, "glScaled", &self._opengl32_dll_handle),
            glScalef: get_func(&egl, "glScalef", &self._opengl32_dll_handle),
            glScissor: get_func(&egl, "glScissor", &self._opengl32_dll_handle),
            glSecondaryColor3b: get_func(&egl, "glSecondaryColor3b", &self._opengl32_dll_handle),
            glSecondaryColor3bv: get_func(&egl, "glSecondaryColor3bv", &self._opengl32_dll_handle),
            glSecondaryColor3d: get_func(&egl, "glSecondaryColor3d", &self._opengl32_dll_handle),
            glSecondaryColor3dv: get_func(&egl, "glSecondaryColor3dv", &self._opengl32_dll_handle),
            glSecondaryColor3f: get_func(&egl, "glSecondaryColor3f", &self._opengl32_dll_handle),
            glSecondaryColor3fv: get_func(&egl, "glSecondaryColor3fv", &self._opengl32_dll_handle),
            glSecondaryColor3i: get_func(&egl, "glSecondaryColor3i", &self._opengl32_dll_handle),
            glSecondaryColor3iv: get_func(&egl, "glSecondaryColor3iv", &self._opengl32_dll_handle),
            glSecondaryColor3s: get_func(&egl, "glSecondaryColor3s", &self._opengl32_dll_handle),
            glSecondaryColor3sv: get_func(&egl, "glSecondaryColor3sv", &self._opengl32_dll_handle),
            glSecondaryColor3ub: get_func(&egl, "glSecondaryColor3ub", &self._opengl32_dll_handle),
            glSecondaryColor3ubv: get_func(
                &egl,
                "glSecondaryColor3ubv",
                &self._opengl32_dll_handle,
            ),
            glSecondaryColor3ui: get_func(&egl, "glSecondaryColor3ui", &self._opengl32_dll_handle),
            glSecondaryColor3uiv: get_func(
                &egl,
                "glSecondaryColor3uiv",
                &self._opengl32_dll_handle,
            ),
            glSecondaryColor3us: get_func(&egl, "glSecondaryColor3us", &self._opengl32_dll_handle),
            glSecondaryColor3usv: get_func(
                &egl,
                "glSecondaryColor3usv",
                &self._opengl32_dll_handle,
            ),
            glSecondaryColorP3ui: get_func(
                &egl,
                "glSecondaryColorP3ui",
                &self._opengl32_dll_handle,
            ),
            glSecondaryColorP3uiv: get_func(
                &egl,
                "glSecondaryColorP3uiv",
                &self._opengl32_dll_handle,
            ),
            glSecondaryColorPointer: get_func(
                &egl,
                "glSecondaryColorPointer",
                &self._opengl32_dll_handle,
            ),
            glSelectBuffer: get_func(&egl, "glSelectBuffer", &self._opengl32_dll_handle),
            glSetFenceAPPLE: get_func(&egl, "glSetFenceAPPLE", &self._opengl32_dll_handle),
            glShadeModel: get_func(&egl, "glShadeModel", &self._opengl32_dll_handle),
            glShaderSource: get_func(&egl, "glShaderSource", &self._opengl32_dll_handle),
            glShaderStorageBlockBinding: get_func(
                &egl,
                "glShaderStorageBlockBinding",
                &self._opengl32_dll_handle,
            ),
            glStencilFunc: get_func(&egl, "glStencilFunc", &self._opengl32_dll_handle),
            glStencilFuncSeparate: get_func(
                &egl,
                "glStencilFuncSeparate",
                &self._opengl32_dll_handle,
            ),
            glStencilMask: get_func(&egl, "glStencilMask", &self._opengl32_dll_handle),
            glStencilMaskSeparate: get_func(
                &egl,
                "glStencilMaskSeparate",
                &self._opengl32_dll_handle,
            ),
            glStencilOp: get_func(&egl, "glStencilOp", &self._opengl32_dll_handle),
            glStencilOpSeparate: get_func(&egl, "glStencilOpSeparate", &self._opengl32_dll_handle),
            glTestFenceAPPLE: get_func(&egl, "glTestFenceAPPLE", &self._opengl32_dll_handle),
            glTestObjectAPPLE: get_func(&egl, "glTestObjectAPPLE", &self._opengl32_dll_handle),
            glTexBuffer: get_func(&egl, "glTexBuffer", &self._opengl32_dll_handle),
            glTexCoord1d: get_func(&egl, "glTexCoord1d", &self._opengl32_dll_handle),
            glTexCoord1dv: get_func(&egl, "glTexCoord1dv", &self._opengl32_dll_handle),
            glTexCoord1f: get_func(&egl, "glTexCoord1f", &self._opengl32_dll_handle),
            glTexCoord1fv: get_func(&egl, "glTexCoord1fv", &self._opengl32_dll_handle),
            glTexCoord1i: get_func(&egl, "glTexCoord1i", &self._opengl32_dll_handle),
            glTexCoord1iv: get_func(&egl, "glTexCoord1iv", &self._opengl32_dll_handle),
            glTexCoord1s: get_func(&egl, "glTexCoord1s", &self._opengl32_dll_handle),
            glTexCoord1sv: get_func(&egl, "glTexCoord1sv", &self._opengl32_dll_handle),
            glTexCoord2d: get_func(&egl, "glTexCoord2d", &self._opengl32_dll_handle),
            glTexCoord2dv: get_func(&egl, "glTexCoord2dv", &self._opengl32_dll_handle),
            glTexCoord2f: get_func(&egl, "glTexCoord2f", &self._opengl32_dll_handle),
            glTexCoord2fv: get_func(&egl, "glTexCoord2fv", &self._opengl32_dll_handle),
            glTexCoord2i: get_func(&egl, "glTexCoord2i", &self._opengl32_dll_handle),
            glTexCoord2iv: get_func(&egl, "glTexCoord2iv", &self._opengl32_dll_handle),
            glTexCoord2s: get_func(&egl, "glTexCoord2s", &self._opengl32_dll_handle),
            glTexCoord2sv: get_func(&egl, "glTexCoord2sv", &self._opengl32_dll_handle),
            glTexCoord3d: get_func(&egl, "glTexCoord3d", &self._opengl32_dll_handle),
            glTexCoord3dv: get_func(&egl, "glTexCoord3dv", &self._opengl32_dll_handle),
            glTexCoord3f: get_func(&egl, "glTexCoord3f", &self._opengl32_dll_handle),
            glTexCoord3fv: get_func(&egl, "glTexCoord3fv", &self._opengl32_dll_handle),
            glTexCoord3i: get_func(&egl, "glTexCoord3i", &self._opengl32_dll_handle),
            glTexCoord3iv: get_func(&egl, "glTexCoord3iv", &self._opengl32_dll_handle),
            glTexCoord3s: get_func(&egl, "glTexCoord3s", &self._opengl32_dll_handle),
            glTexCoord3sv: get_func(&egl, "glTexCoord3sv", &self._opengl32_dll_handle),
            glTexCoord4d: get_func(&egl, "glTexCoord4d", &self._opengl32_dll_handle),
            glTexCoord4dv: get_func(&egl, "glTexCoord4dv", &self._opengl32_dll_handle),
            glTexCoord4f: get_func(&egl, "glTexCoord4f", &self._opengl32_dll_handle),
            glTexCoord4fv: get_func(&egl, "glTexCoord4fv", &self._opengl32_dll_handle),
            glTexCoord4i: get_func(&egl, "glTexCoord4i", &self._opengl32_dll_handle),
            glTexCoord4iv: get_func(&egl, "glTexCoord4iv", &self._opengl32_dll_handle),
            glTexCoord4s: get_func(&egl, "glTexCoord4s", &self._opengl32_dll_handle),
            glTexCoord4sv: get_func(&egl, "glTexCoord4sv", &self._opengl32_dll_handle),
            glTexCoordP1ui: get_func(&egl, "glTexCoordP1ui", &self._opengl32_dll_handle),
            glTexCoordP1uiv: get_func(&egl, "glTexCoordP1uiv", &self._opengl32_dll_handle),
            glTexCoordP2ui: get_func(&egl, "glTexCoordP2ui", &self._opengl32_dll_handle),
            glTexCoordP2uiv: get_func(&egl, "glTexCoordP2uiv", &self._opengl32_dll_handle),
            glTexCoordP3ui: get_func(&egl, "glTexCoordP3ui", &self._opengl32_dll_handle),
            glTexCoordP3uiv: get_func(&egl, "glTexCoordP3uiv", &self._opengl32_dll_handle),
            glTexCoordP4ui: get_func(&egl, "glTexCoordP4ui", &self._opengl32_dll_handle),
            glTexCoordP4uiv: get_func(&egl, "glTexCoordP4uiv", &self._opengl32_dll_handle),
            glTexCoordPointer: get_func(&egl, "glTexCoordPointer", &self._opengl32_dll_handle),
            glTexEnvf: get_func(&egl, "glTexEnvf", &self._opengl32_dll_handle),
            glTexEnvfv: get_func(&egl, "glTexEnvfv", &self._opengl32_dll_handle),
            glTexEnvi: get_func(&egl, "glTexEnvi", &self._opengl32_dll_handle),
            glTexEnviv: get_func(&egl, "glTexEnviv", &self._opengl32_dll_handle),
            glTexGend: get_func(&egl, "glTexGend", &self._opengl32_dll_handle),
            glTexGendv: get_func(&egl, "glTexGendv", &self._opengl32_dll_handle),
            glTexGenf: get_func(&egl, "glTexGenf", &self._opengl32_dll_handle),
            glTexGenfv: get_func(&egl, "glTexGenfv", &self._opengl32_dll_handle),
            glTexGeni: get_func(&egl, "glTexGeni", &self._opengl32_dll_handle),
            glTexGeniv: get_func(&egl, "glTexGeniv", &self._opengl32_dll_handle),
            glTexImage1D: get_func(&egl, "glTexImage1D", &self._opengl32_dll_handle),
            glTexImage2D: get_func(&egl, "glTexImage2D", &self._opengl32_dll_handle),
            glTexImage2DMultisample: get_func(
                &egl,
                "glTexImage2DMultisample",
                &self._opengl32_dll_handle,
            ),
            glTexImage3D: get_func(&egl, "glTexImage3D", &self._opengl32_dll_handle),
            glTexImage3DMultisample: get_func(
                &egl,
                "glTexImage3DMultisample",
                &self._opengl32_dll_handle,
            ),
            glTexParameterIiv: get_func(&egl, "glTexParameterIiv", &self._opengl32_dll_handle),
            glTexParameterIuiv: get_func(&egl, "glTexParameterIuiv", &self._opengl32_dll_handle),
            glTexParameterf: get_func(&egl, "glTexParameterf", &self._opengl32_dll_handle),
            glTexParameterfv: get_func(&egl, "glTexParameterfv", &self._opengl32_dll_handle),
            glTexParameteri: get_func(&egl, "glTexParameteri", &self._opengl32_dll_handle),
            glTexParameteriv: get_func(&egl, "glTexParameteriv", &self._opengl32_dll_handle),
            glTexStorage1D: get_func(&egl, "glTexStorage1D", &self._opengl32_dll_handle),
            glTexStorage2D: get_func(&egl, "glTexStorage2D", &self._opengl32_dll_handle),
            glTexStorage3D: get_func(&egl, "glTexStorage3D", &self._opengl32_dll_handle),
            glTexSubImage1D: get_func(&egl, "glTexSubImage1D", &self._opengl32_dll_handle),
            glTexSubImage2D: get_func(&egl, "glTexSubImage2D", &self._opengl32_dll_handle),
            glTexSubImage3D: get_func(&egl, "glTexSubImage3D", &self._opengl32_dll_handle),
            glTextureRangeAPPLE: get_func(&egl, "glTextureRangeAPPLE", &self._opengl32_dll_handle),
            glTransformFeedbackVaryings: get_func(
                &egl,
                "glTransformFeedbackVaryings",
                &self._opengl32_dll_handle,
            ),
            glTranslated: get_func(&egl, "glTranslated", &self._opengl32_dll_handle),
            glTranslatef: get_func(&egl, "glTranslatef", &self._opengl32_dll_handle),
            glUniform1f: get_func(&egl, "glUniform1f", &self._opengl32_dll_handle),
            glUniform1fv: get_func(&egl, "glUniform1fv", &self._opengl32_dll_handle),
            glUniform1i: get_func(&egl, "glUniform1i", &self._opengl32_dll_handle),
            glUniform1iv: get_func(&egl, "glUniform1iv", &self._opengl32_dll_handle),
            glUniform1ui: get_func(&egl, "glUniform1ui", &self._opengl32_dll_handle),
            glUniform1uiv: get_func(&egl, "glUniform1uiv", &self._opengl32_dll_handle),
            glUniform2f: get_func(&egl, "glUniform2f", &self._opengl32_dll_handle),
            glUniform2fv: get_func(&egl, "glUniform2fv", &self._opengl32_dll_handle),
            glUniform2i: get_func(&egl, "glUniform2i", &self._opengl32_dll_handle),
            glUniform2iv: get_func(&egl, "glUniform2iv", &self._opengl32_dll_handle),
            glUniform2ui: get_func(&egl, "glUniform2ui", &self._opengl32_dll_handle),
            glUniform2uiv: get_func(&egl, "glUniform2uiv", &self._opengl32_dll_handle),
            glUniform3f: get_func(&egl, "glUniform3f", &self._opengl32_dll_handle),
            glUniform3fv: get_func(&egl, "glUniform3fv", &self._opengl32_dll_handle),
            glUniform3i: get_func(&egl, "glUniform3i", &self._opengl32_dll_handle),
            glUniform3iv: get_func(&egl, "glUniform3iv", &self._opengl32_dll_handle),
            glUniform3ui: get_func(&egl, "glUniform3ui", &self._opengl32_dll_handle),
            glUniform3uiv: get_func(&egl, "glUniform3uiv", &self._opengl32_dll_handle),
            glUniform4f: get_func(&egl, "glUniform4f", &self._opengl32_dll_handle),
            glUniform4fv: get_func(&egl, "glUniform4fv", &self._opengl32_dll_handle),
            glUniform4i: get_func(&egl, "glUniform4i", &self._opengl32_dll_handle),
            glUniform4iv: get_func(&egl, "glUniform4iv", &self._opengl32_dll_handle),
            glUniform4ui: get_func(&egl, "glUniform4ui", &self._opengl32_dll_handle),
            glUniform4uiv: get_func(&egl, "glUniform4uiv", &self._opengl32_dll_handle),
            glUniformBlockBinding: get_func(
                &egl,
                "glUniformBlockBinding",
                &self._opengl32_dll_handle,
            ),
            glUniformMatrix2fv: get_func(&egl, "glUniformMatrix2fv", &self._opengl32_dll_handle),
            glUniformMatrix2x3fv: get_func(
                &egl,
                "glUniformMatrix2x3fv",
                &self._opengl32_dll_handle,
            ),
            glUniformMatrix2x4fv: get_func(
                &egl,
                "glUniformMatrix2x4fv",
                &self._opengl32_dll_handle,
            ),
            glUniformMatrix3fv: get_func(&egl, "glUniformMatrix3fv", &self._opengl32_dll_handle),
            glUniformMatrix3x2fv: get_func(
                &egl,
                "glUniformMatrix3x2fv",
                &self._opengl32_dll_handle,
            ),
            glUniformMatrix3x4fv: get_func(
                &egl,
                "glUniformMatrix3x4fv",
                &self._opengl32_dll_handle,
            ),
            glUniformMatrix4fv: get_func(&egl, "glUniformMatrix4fv", &self._opengl32_dll_handle),
            glUniformMatrix4x2fv: get_func(
                &egl,
                "glUniformMatrix4x2fv",
                &self._opengl32_dll_handle,
            ),
            glUniformMatrix4x3fv: get_func(
                &egl,
                "glUniformMatrix4x3fv",
                &self._opengl32_dll_handle,
            ),
            glUnmapBuffer: get_func(&egl, "glUnmapBuffer", &self._opengl32_dll_handle),
            glUseProgram: get_func(&egl, "glUseProgram", &self._opengl32_dll_handle),
            glValidateProgram: get_func(&egl, "glValidateProgram", &self._opengl32_dll_handle),
            glVertex2d: get_func(&egl, "glVertex2d", &self._opengl32_dll_handle),
            glVertex2dv: get_func(&egl, "glVertex2dv", &self._opengl32_dll_handle),
            glVertex2f: get_func(&egl, "glVertex2f", &self._opengl32_dll_handle),
            glVertex2fv: get_func(&egl, "glVertex2fv", &self._opengl32_dll_handle),
            glVertex2i: get_func(&egl, "glVertex2i", &self._opengl32_dll_handle),
            glVertex2iv: get_func(&egl, "glVertex2iv", &self._opengl32_dll_handle),
            glVertex2s: get_func(&egl, "glVertex2s", &self._opengl32_dll_handle),
            glVertex2sv: get_func(&egl, "glVertex2sv", &self._opengl32_dll_handle),
            glVertex3d: get_func(&egl, "glVertex3d", &self._opengl32_dll_handle),
            glVertex3dv: get_func(&egl, "glVertex3dv", &self._opengl32_dll_handle),
            glVertex3f: get_func(&egl, "glVertex3f", &self._opengl32_dll_handle),
            glVertex3fv: get_func(&egl, "glVertex3fv", &self._opengl32_dll_handle),
            glVertex3i: get_func(&egl, "glVertex3i", &self._opengl32_dll_handle),
            glVertex3iv: get_func(&egl, "glVertex3iv", &self._opengl32_dll_handle),
            glVertex3s: get_func(&egl, "glVertex3s", &self._opengl32_dll_handle),
            glVertex3sv: get_func(&egl, "glVertex3sv", &self._opengl32_dll_handle),
            glVertex4d: get_func(&egl, "glVertex4d", &self._opengl32_dll_handle),
            glVertex4dv: get_func(&egl, "glVertex4dv", &self._opengl32_dll_handle),
            glVertex4f: get_func(&egl, "glVertex4f", &self._opengl32_dll_handle),
            glVertex4fv: get_func(&egl, "glVertex4fv", &self._opengl32_dll_handle),
            glVertex4i: get_func(&egl, "glVertex4i", &self._opengl32_dll_handle),
            glVertex4iv: get_func(&egl, "glVertex4iv", &self._opengl32_dll_handle),
            glVertex4s: get_func(&egl, "glVertex4s", &self._opengl32_dll_handle),
            glVertex4sv: get_func(&egl, "glVertex4sv", &self._opengl32_dll_handle),
            glVertexAttrib1d: get_func(&egl, "glVertexAttrib1d", &self._opengl32_dll_handle),
            glVertexAttrib1dv: get_func(&egl, "glVertexAttrib1dv", &self._opengl32_dll_handle),
            glVertexAttrib1f: get_func(&egl, "glVertexAttrib1f", &self._opengl32_dll_handle),
            glVertexAttrib1fv: get_func(&egl, "glVertexAttrib1fv", &self._opengl32_dll_handle),
            glVertexAttrib1s: get_func(&egl, "glVertexAttrib1s", &self._opengl32_dll_handle),
            glVertexAttrib1sv: get_func(&egl, "glVertexAttrib1sv", &self._opengl32_dll_handle),
            glVertexAttrib2d: get_func(&egl, "glVertexAttrib2d", &self._opengl32_dll_handle),
            glVertexAttrib2dv: get_func(&egl, "glVertexAttrib2dv", &self._opengl32_dll_handle),
            glVertexAttrib2f: get_func(&egl, "glVertexAttrib2f", &self._opengl32_dll_handle),
            glVertexAttrib2fv: get_func(&egl, "glVertexAttrib2fv", &self._opengl32_dll_handle),
            glVertexAttrib2s: get_func(&egl, "glVertexAttrib2s", &self._opengl32_dll_handle),
            glVertexAttrib2sv: get_func(&egl, "glVertexAttrib2sv", &self._opengl32_dll_handle),
            glVertexAttrib3d: get_func(&egl, "glVertexAttrib3d", &self._opengl32_dll_handle),
            glVertexAttrib3dv: get_func(&egl, "glVertexAttrib3dv", &self._opengl32_dll_handle),
            glVertexAttrib3f: get_func(&egl, "glVertexAttrib3f", &self._opengl32_dll_handle),
            glVertexAttrib3fv: get_func(&egl, "glVertexAttrib3fv", &self._opengl32_dll_handle),
            glVertexAttrib3s: get_func(&egl, "glVertexAttrib3s", &self._opengl32_dll_handle),
            glVertexAttrib3sv: get_func(&egl, "glVertexAttrib3sv", &self._opengl32_dll_handle),
            glVertexAttrib4Nbv: get_func(&egl, "glVertexAttrib4Nbv", &self._opengl32_dll_handle),
            glVertexAttrib4Niv: get_func(&egl, "glVertexAttrib4Niv", &self._opengl32_dll_handle),
            glVertexAttrib4Nsv: get_func(&egl, "glVertexAttrib4Nsv", &self._opengl32_dll_handle),
            glVertexAttrib4Nub: get_func(&egl, "glVertexAttrib4Nub", &self._opengl32_dll_handle),
            glVertexAttrib4Nubv: get_func(&egl, "glVertexAttrib4Nubv", &self._opengl32_dll_handle),
            glVertexAttrib4Nuiv: get_func(&egl, "glVertexAttrib4Nuiv", &self._opengl32_dll_handle),
            glVertexAttrib4Nusv: get_func(&egl, "glVertexAttrib4Nusv", &self._opengl32_dll_handle),
            glVertexAttrib4bv: get_func(&egl, "glVertexAttrib4bv", &self._opengl32_dll_handle),
            glVertexAttrib4d: get_func(&egl, "glVertexAttrib4d", &self._opengl32_dll_handle),
            glVertexAttrib4dv: get_func(&egl, "glVertexAttrib4dv", &self._opengl32_dll_handle),
            glVertexAttrib4f: get_func(&egl, "glVertexAttrib4f", &self._opengl32_dll_handle),
            glVertexAttrib4fv: get_func(&egl, "glVertexAttrib4fv", &self._opengl32_dll_handle),
            glVertexAttrib4iv: get_func(&egl, "glVertexAttrib4iv", &self._opengl32_dll_handle),
            glVertexAttrib4s: get_func(&egl, "glVertexAttrib4s", &self._opengl32_dll_handle),
            glVertexAttrib4sv: get_func(&egl, "glVertexAttrib4sv", &self._opengl32_dll_handle),
            glVertexAttrib4ubv: get_func(&egl, "glVertexAttrib4ubv", &self._opengl32_dll_handle),
            glVertexAttrib4uiv: get_func(&egl, "glVertexAttrib4uiv", &self._opengl32_dll_handle),
            glVertexAttrib4usv: get_func(&egl, "glVertexAttrib4usv", &self._opengl32_dll_handle),
            glVertexAttribDivisor: get_func(
                &egl,
                "glVertexAttribDivisor",
                &self._opengl32_dll_handle,
            ),
            glVertexAttribI1i: get_func(&egl, "glVertexAttribI1i", &self._opengl32_dll_handle),
            glVertexAttribI1iv: get_func(&egl, "glVertexAttribI1iv", &self._opengl32_dll_handle),
            glVertexAttribI1ui: get_func(&egl, "glVertexAttribI1ui", &self._opengl32_dll_handle),
            glVertexAttribI1uiv: get_func(&egl, "glVertexAttribI1uiv", &self._opengl32_dll_handle),
            glVertexAttribI2i: get_func(&egl, "glVertexAttribI2i", &self._opengl32_dll_handle),
            glVertexAttribI2iv: get_func(&egl, "glVertexAttribI2iv", &self._opengl32_dll_handle),
            glVertexAttribI2ui: get_func(&egl, "glVertexAttribI2ui", &self._opengl32_dll_handle),
            glVertexAttribI2uiv: get_func(&egl, "glVertexAttribI2uiv", &self._opengl32_dll_handle),
            glVertexAttribI3i: get_func(&egl, "glVertexAttribI3i", &self._opengl32_dll_handle),
            glVertexAttribI3iv: get_func(&egl, "glVertexAttribI3iv", &self._opengl32_dll_handle),
            glVertexAttribI3ui: get_func(&egl, "glVertexAttribI3ui", &self._opengl32_dll_handle),
            glVertexAttribI3uiv: get_func(&egl, "glVertexAttribI3uiv", &self._opengl32_dll_handle),
            glVertexAttribI4bv: get_func(&egl, "glVertexAttribI4bv", &self._opengl32_dll_handle),
            glVertexAttribI4i: get_func(&egl, "glVertexAttribI4i", &self._opengl32_dll_handle),
            glVertexAttribI4iv: get_func(&egl, "glVertexAttribI4iv", &self._opengl32_dll_handle),
            glVertexAttribI4sv: get_func(&egl, "glVertexAttribI4sv", &self._opengl32_dll_handle),
            glVertexAttribI4ubv: get_func(&egl, "glVertexAttribI4ubv", &self._opengl32_dll_handle),
            glVertexAttribI4ui: get_func(&egl, "glVertexAttribI4ui", &self._opengl32_dll_handle),
            glVertexAttribI4uiv: get_func(&egl, "glVertexAttribI4uiv", &self._opengl32_dll_handle),
            glVertexAttribI4usv: get_func(&egl, "glVertexAttribI4usv", &self._opengl32_dll_handle),
            glVertexAttribIPointer: get_func(
                &egl,
                "glVertexAttribIPointer",
                &self._opengl32_dll_handle,
            ),
            glVertexAttribP1ui: get_func(&egl, "glVertexAttribP1ui", &self._opengl32_dll_handle),
            glVertexAttribP1uiv: get_func(&egl, "glVertexAttribP1uiv", &self._opengl32_dll_handle),
            glVertexAttribP2ui: get_func(&egl, "glVertexAttribP2ui", &self._opengl32_dll_handle),
            glVertexAttribP2uiv: get_func(&egl, "glVertexAttribP2uiv", &self._opengl32_dll_handle),
            glVertexAttribP3ui: get_func(&egl, "glVertexAttribP3ui", &self._opengl32_dll_handle),
            glVertexAttribP3uiv: get_func(&egl, "glVertexAttribP3uiv", &self._opengl32_dll_handle),
            glVertexAttribP4ui: get_func(&egl, "glVertexAttribP4ui", &self._opengl32_dll_handle),
            glVertexAttribP4uiv: get_func(&egl, "glVertexAttribP4uiv", &self._opengl32_dll_handle),
            glVertexAttribPointer: get_func(
                &egl,
                "glVertexAttribPointer",
                &self._opengl32_dll_handle,
            ),
            glVertexP2ui: get_func(&egl, "glVertexP2ui", &self._opengl32_dll_handle),
            glVertexP2uiv: get_func(&egl, "glVertexP2uiv", &self._opengl32_dll_handle),
            glVertexP3ui: get_func(&egl, "glVertexP3ui", &self._opengl32_dll_handle),
            glVertexP3uiv: get_func(&egl, "glVertexP3uiv", &self._opengl32_dll_handle),
            glVertexP4ui: get_func(&egl, "glVertexP4ui", &self._opengl32_dll_handle),
            glVertexP4uiv: get_func(&egl, "glVertexP4uiv", &self._opengl32_dll_handle),
            glVertexPointer: get_func(&egl, "glVertexPointer", &self._opengl32_dll_handle),
            glViewport: get_func(&egl, "glViewport", &self._opengl32_dll_handle),
            glWaitSync: get_func(&egl, "glWaitSync", &self._opengl32_dll_handle),
            glWindowPos2d: get_func(&egl, "glWindowPos2d", &self._opengl32_dll_handle),
            glWindowPos2dv: get_func(&egl, "glWindowPos2dv", &self._opengl32_dll_handle),
            glWindowPos2f: get_func(&egl, "glWindowPos2f", &self._opengl32_dll_handle),
            glWindowPos2fv: get_func(&egl, "glWindowPos2fv", &self._opengl32_dll_handle),
            glWindowPos2i: get_func(&egl, "glWindowPos2i", &self._opengl32_dll_handle),
            glWindowPos2iv: get_func(&egl, "glWindowPos2iv", &self._opengl32_dll_handle),
            glWindowPos2s: get_func(&egl, "glWindowPos2s", &self._opengl32_dll_handle),
            glWindowPos2sv: get_func(&egl, "glWindowPos2sv", &self._opengl32_dll_handle),
            glWindowPos3d: get_func(&egl, "glWindowPos3d", &self._opengl32_dll_handle),
            glWindowPos3dv: get_func(&egl, "glWindowPos3dv", &self._opengl32_dll_handle),
            glWindowPos3f: get_func(&egl, "glWindowPos3f", &self._opengl32_dll_handle),
            glWindowPos3fv: get_func(&egl, "glWindowPos3fv", &self._opengl32_dll_handle),
            glWindowPos3i: get_func(&egl, "glWindowPos3i", &self._opengl32_dll_handle),
            glWindowPos3iv: get_func(&egl, "glWindowPos3iv", &self._opengl32_dll_handle),
            glWindowPos3s: get_func(&egl, "glWindowPos3s", &self._opengl32_dll_handle),
            glWindowPos3sv: get_func(&egl, "glWindowPos3sv", &self._opengl32_dll_handle),
        });
    }
}
