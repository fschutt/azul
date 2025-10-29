//! C-style definitions for X11, EGL, and xkbcommon.

#![allow(non_camel_case_types, non_snake_case)]

use std::ffi::{c_char, c_int, c_long, c_uchar, c_uint, c_ulong, c_void};

// Basic X11 types
pub type Display = c_void;
pub type Window = c_ulong;
pub type Colormap = c_ulong;
pub type Visual = c_void;
pub type Atom = c_ulong;
pub type Drawable = c_ulong;
pub type Cursor = c_ulong;
pub type GC = *mut c_void;
pub type XIM = *mut c_void;
pub type XIC = *mut c_void;
pub type KeySym = c_ulong;
pub type XErrorHandler = Option<unsafe extern "C" fn(*mut Display, *mut XErrorEvent) -> c_int>;

#[repr(C)]
#[derive(Clone, Copy)]
pub union XEvent {
    pub type_: c_int,
    pub any: XAnyEvent,
    pub key: XKeyEvent,
    pub button: XButtonEvent,
    pub motion: XMotionEvent,
    pub crossing: XCrossingEvent,
    pub focus: XFocusChangeEvent,
    pub expose: XExposeEvent,
    pub configure: XConfigureEvent,
    pub client_message: XClientMessageEvent,
    pad: [c_long; 24],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct XAnyEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: c_int,
    pub display: *mut Display,
    pub window: Window,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XKeyEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: c_int,
    pub display: *mut Display,
    pub window: Window,
    pub root: Window,
    pub subwindow: Window,
    pub time: c_ulong,
    pub x: c_int,
    pub y: c_int,
    pub x_root: c_int,
    pub y_root: c_int,
    pub state: c_uint,
    pub keycode: c_uint,
    pub same_screen: c_int,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XButtonEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: c_int,
    pub display: *mut Display,
    pub window: Window,
    pub root: Window,
    pub subwindow: Window,
    pub time: c_ulong,
    pub x: c_int,
    pub y: c_int,
    pub x_root: c_int,
    pub y_root: c_int,
    pub state: c_uint,
    pub button: c_uint,
    pub same_screen: c_int,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XMotionEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: c_int,
    pub display: *mut Display,
    pub window: Window,
    pub root: Window,
    pub subwindow: Window,
    pub time: c_ulong,
    pub x: c_int,
    pub y: c_int,
    pub x_root: c_int,
    pub y_root: c_int,
    pub state: c_uint,
    pub is_hint: c_char,
    pub same_screen: c_int,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XCrossingEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: c_int,
    pub display: *mut Display,
    pub window: Window,
    pub root: Window,
    pub subwindow: Window,
    pub time: c_ulong,
    pub x: c_int,
    pub y: c_int,
    pub x_root: c_int,
    pub y_root: c_int,
    pub mode: c_int,
    pub detail: c_int,
    pub same_screen: c_int,
    pub focus: c_int,
    pub state: c_uint,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XFocusChangeEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: c_int,
    pub display: *mut Display,
    pub window: Window,
    pub mode: c_int,
    pub detail: c_int,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XExposeEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: c_int,
    pub display: *mut Display,
    pub window: Window,
    pub x: c_int,
    pub y: c_int,
    pub width: c_int,
    pub height: c_int,
    pub count: c_int,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XConfigureEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: c_int,
    pub display: *mut Display,
    pub event: Window,
    pub window: Window,
    pub x: c_int,
    pub y: c_int,
    pub width: c_int,
    pub height: c_int,
    pub border_width: c_int,
    pub above: Window,
    pub override_redirect: c_int,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XErrorEvent {
    pub type_: c_int,
    pub display: *mut Display,
    pub resourceid: c_ulong,
    pub serial: c_ulong,
    pub error_code: u8,
    pub request_code: u8,
    pub minor_code: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union XClientMessageData {
    pub b: [c_char; 20],
    pub s: [i16; 10],
    pub l: [c_long; 5],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XClientMessageEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: c_int,
    pub display: *mut Display,
    pub window: Window,
    pub message_type: Atom,
    pub format: c_int,
    pub data: XClientMessageData,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XSetWindowAttributes {
    pub background_pixmap: c_ulong,
    pub background_pixel: c_ulong,
    pub border_pixmap: c_ulong,
    pub border_pixel: c_ulong,
    pub bit_gravity: c_int,
    pub win_gravity: c_int,
    pub backing_store: c_int,
    pub backing_planes: c_ulong,
    pub backing_pixel: c_ulong,
    pub save_under: c_int,
    pub event_mask: c_long,
    pub do_not_propagate_mask: c_long,
    pub override_redirect: c_int,
    pub colormap: Colormap,
    pub cursor: c_ulong,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct XComposeStatus {
    pub compose_ptr: *mut c_void,
    pub chars_matched: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct XGCValues {
    pub function: c_int,
    pub plane_mask: c_ulong,
    pub foreground: c_ulong,
    pub background: c_ulong,
    pub line_width: c_int,
    pub line_style: c_int,
    pub cap_style: c_int,
    pub join_style: c_int,
    pub fill_style: c_int,
    pub fill_rule: c_int,
    pub arc_mode: c_int,
    pub tile: c_ulong,
    pub stipple: c_ulong,
    pub ts_x_origin: c_int,
    pub ts_y_origin: c_int,
    pub font: c_ulong,
    pub subwindow_mode: c_int,
    pub graphics_exposures: c_int,
    pub clip_x_origin: c_int,
    pub clip_y_origin: c_int,
    pub clip_mask: c_ulong,
    pub dash_offset: c_int,
    pub dashes: c_char,
}

#[repr(C)]
pub struct XkbDescRec {
    pub dpy: *mut Display,
    pub flags: c_uint,
    pub device_spec: c_uint,
    pub min_key_code: c_uchar,
    pub max_key_code: c_uchar,
    // Simplified - full struct has many more fields
    // Add fields as needed
}

// Event masks
pub const KeyPressMask: c_long = 1 << 0;
pub const KeyReleaseMask: c_long = 1 << 1;
pub const ButtonPressMask: c_long = 1 << 2;
pub const ButtonReleaseMask: c_long = 1 << 3;
pub const EnterWindowMask: c_long = 1 << 4;
pub const LeaveWindowMask: c_long = 1 << 5;
pub const PointerMotionMask: c_long = 1 << 6;
pub const ExposureMask: c_long = 1 << 15;
pub const StructureNotifyMask: c_long = 1 << 17;
pub const FocusChangeMask: c_long = 1 << 21;

// Event types
pub const KeyPress: c_int = 2;
pub const KeyRelease: c_int = 3;
pub const ButtonPress: c_int = 4;
pub const ButtonRelease: c_int = 5;
pub const MotionNotify: c_int = 6;
pub const EnterNotify: c_int = 7;
pub const LeaveNotify: c_int = 8;
pub const FocusIn: c_int = 9;
pub const FocusOut: c_int = 10;
pub const Expose: c_int = 12;
pub const ConfigureNotify: c_int = 22;
pub const ClientMessage: c_int = 33;

// Window classes and attributes
pub const InputOutput: c_uint = 1;
pub const InputOnly: c_uint = 2;
pub const CopyFromParent: c_int = 0;
pub const CWEventMask: c_ulong = 1 << 11;
pub const CWOverrideRedirect: c_ulong = 1 << 9;
pub const SubstructureRedirectMask: c_long = 1 << 20;
pub const SubstructureNotifyMask: c_long = 1 << 19;

// IME
pub const XIMPreeditNothing: c_ulong = 0x0008;
pub const XIMStatusNothing: c_ulong = 0x0010;

// Keysyms
pub const XK_BackSpace: u32 = 0xFF08;
pub const XK_Tab: u32 = 0xFF09;
pub const XK_Return: u32 = 0xFF0D;
pub const XK_Pause: u32 = 0xFF13;
pub const XK_Scroll_Lock: u32 = 0xFF14;
pub const XK_Escape: u32 = 0xFF1B;
pub const XK_Home: u32 = 0xFF50;
pub const XK_Left: u32 = 0xFF51;
pub const XK_Up: u32 = 0xFF52;
pub const XK_Right: u32 = 0xFF53;
pub const XK_Down: u32 = 0xFF54;
pub const XK_Page_Up: u32 = 0xFF55;
pub const XK_Page_Down: u32 = 0xFF56;
pub const XK_End: u32 = 0xFF57;
pub const XK_Insert: u32 = 0xFF63;
pub const XK_Delete: u32 = 0xFFFF;
pub const XK_space: u32 = 0x0020;
pub const XK_0: u32 = 0x0030;
pub const XK_1: u32 = 0x0031;
pub const XK_2: u32 = 0x0032;
pub const XK_3: u32 = 0x0033;
pub const XK_4: u32 = 0x0034;
pub const XK_5: u32 = 0x0035;
pub const XK_6: u32 = 0x0036;
pub const XK_7: u32 = 0x0037;
pub const XK_8: u32 = 0x0038;
pub const XK_9: u32 = 0x0039;
pub const XK_a: u32 = 0x0061;
pub const XK_A: u32 = 0x0041;
pub const XK_b: u32 = 0x0062;
pub const XK_B: u32 = 0x0042;
pub const XK_c: u32 = 0x0063;
pub const XK_C: u32 = 0x0043;
pub const XK_d: u32 = 0x0064;
pub const XK_D: u32 = 0x0044;
pub const XK_e: u32 = 0x0065;
pub const XK_E: u32 = 0x0045;
pub const XK_f: u32 = 0x0066;
pub const XK_F: u32 = 0x0046;
pub const XK_g: u32 = 0x0067;
pub const XK_G: u32 = 0x0047;
pub const XK_h: u32 = 0x0068;
pub const XK_H: u32 = 0x0048;
pub const XK_i: u32 = 0x0069;
pub const XK_I: u32 = 0x0049;
pub const XK_j: u32 = 0x006a;
pub const XK_J: u32 = 0x004a;
pub const XK_k: u32 = 0x006b;
pub const XK_K: u32 = 0x004b;
pub const XK_l: u32 = 0x006c;
pub const XK_L: u32 = 0x004c;
pub const XK_m: u32 = 0x006d;
pub const XK_M: u32 = 0x004d;
pub const XK_n: u32 = 0x006e;
pub const XK_N: u32 = 0x004e;
pub const XK_o: u32 = 0x006f;
pub const XK_O: u32 = 0x004f;
pub const XK_p: u32 = 0x0070;
pub const XK_P: u32 = 0x0050;
pub const XK_q: u32 = 0x0071;
pub const XK_Q: u32 = 0x0051;
pub const XK_r: u32 = 0x0072;
pub const XK_R: u32 = 0x0052;
pub const XK_s: u32 = 0x0073;
pub const XK_S: u32 = 0x0053;
pub const XK_t: u32 = 0x0074;
pub const XK_T: u32 = 0x0054;
pub const XK_u: u32 = 0x0075;
pub const XK_U: u32 = 0x0055;
pub const XK_v: u32 = 0x0076;
pub const XK_V: u32 = 0x0056;
pub const XK_w: u32 = 0x0077;
pub const XK_W: u32 = 0x0057;
pub const XK_x: u32 = 0x0078;
pub const XK_X: u32 = 0x0058;
pub const XK_y: u32 = 0x0079;
pub const XK_Y: u32 = 0x0059;
pub const XK_z: u32 = 0x007a;
pub const XK_Z: u32 = 0x005a;
pub const XK_F1: u32 = 0xFFBE;
pub const XK_F2: u32 = 0xFFBF;
pub const XK_F3: u32 = 0xFFC0;
pub const XK_F4: u32 = 0xFFC1;
pub const XK_F5: u32 = 0xFFC2;
pub const XK_F6: u32 = 0xFFC3;
pub const XK_F7: u32 = 0xFFC4;
pub const XK_F8: u32 = 0xFFC5;
pub const XK_F9: u32 = 0xFFC6;
pub const XK_F10: u32 = 0xFFC7;
pub const XK_F11: u32 = 0xFFC8;
pub const XK_F12: u32 = 0xFFC9;
pub const XK_Shift_L: u32 = 0xFFE1;
pub const XK_Shift_R: u32 = 0xFFE2;
pub const XK_Control_L: u32 = 0xFFE3;
pub const XK_Control_R: u32 = 0xFFE4;
pub const XK_Alt_L: u32 = 0xFFE9;
pub const XK_Alt_R: u32 = 0xFFEA;
pub const XK_Super_L: u32 = 0xFFEB;
pub const XK_Super_R: u32 = 0xFFEC;

// EGL types
pub type EGLDisplay = *mut c_void;
pub type EGLConfig = *mut c_void;
pub type EGLContext = *mut c_void;
pub type EGLSurface = *mut c_void;
pub type EGLNativeDisplayType = *mut c_void;
pub type EGLNativeWindowType = c_ulong;
pub const EGL_DEFAULT_DISPLAY: EGLNativeDisplayType = std::ptr::null_mut();

// EGL constants
pub const EGL_RED_SIZE: u32 = 0x3024;
pub const EGL_GREEN_SIZE: u32 = 0x3023;
pub const EGL_BLUE_SIZE: u32 = 0x3022;
pub const EGL_ALPHA_SIZE: u32 = 0x3021;
pub const EGL_DEPTH_SIZE: u32 = 0x3025;
pub const EGL_STENCIL_SIZE: u32 = 0x3026;
pub const EGL_SURFACE_TYPE: u32 = 0x3033;
pub const EGL_WINDOW_BIT: u32 = 0x0004;
pub const EGL_RENDERABLE_TYPE: u32 = 0x3040;
pub const EGL_OPENGL_BIT: u32 = 0x0008;
pub const EGL_NONE: u32 = 0x3038;
pub const EGL_OPENGL_API: u32 = 0x30A0;
pub const EGL_CONTEXT_MAJOR_VERSION: u32 = 0x3098;
pub const EGL_CONTEXT_MINOR_VERSION: u32 = 0x30FB;
pub const EGL_CONTEXT_OPENGL_PROFILE_MASK: u32 = 0x30FD;
pub const EGL_CONTEXT_OPENGL_CORE_PROFILE_BIT: u32 = 0x00000001;

// EGL function pointer types
pub type eglGetDisplay = unsafe extern "C" fn(EGLNativeDisplayType) -> EGLDisplay;
pub type eglInitialize = unsafe extern "C" fn(EGLDisplay, *mut i32, *mut i32) -> u32;
pub type eglBindAPI = unsafe extern "C" fn(u32) -> u32;
pub type eglChooseConfig =
    unsafe extern "C" fn(EGLDisplay, *const i32, *mut EGLConfig, i32, *mut i32) -> u32;
pub type eglCreateContext =
    unsafe extern "C" fn(EGLDisplay, EGLConfig, EGLContext, *const i32) -> EGLContext;
pub type eglCreateWindowSurface =
    unsafe extern "C" fn(EGLDisplay, EGLConfig, EGLNativeWindowType, *const i32) -> EGLSurface;
pub type eglMakeCurrent =
    unsafe extern "C" fn(EGLDisplay, EGLSurface, EGLSurface, EGLContext) -> u32;
pub type eglSwapBuffers = unsafe extern "C" fn(EGLDisplay, EGLSurface) -> u32;
pub type eglGetError = unsafe extern "C" fn() -> u32;
pub type eglGetProcAddress = unsafe extern "C" fn(*const c_char) -> *const c_void;
pub type eglDestroySurface = unsafe extern "C" fn(EGLDisplay, EGLSurface) -> u32;
pub type eglDestroyContext = unsafe extern "C" fn(EGLDisplay, EGLContext) -> u32;

// XKB types
#[repr(C)]
#[derive(Clone, Copy)]
pub struct xkb_context {
    _private: [u8; 0],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct xkb_keymap {
    _private: [u8; 0],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct xkb_state {
    _private: [u8; 0],
}
pub type xkb_keycode_t = u32;
pub type xkb_keysym_t = u32;
#[repr(C)]
pub struct xkb_rule_names {
    pub rules: *const c_char,
    pub model: *const c_char,
    pub layout: *const c_char,
    pub variant: *const c_char,
    pub options: *const c_char,
}

// Xlib function pointer types
pub type XOpenDisplay = unsafe extern "C" fn(*const c_char) -> *mut Display;
pub type XCloseDisplay = unsafe extern "C" fn(*mut Display) -> c_int;
pub type XDefaultScreen = unsafe extern "C" fn(*mut Display) -> c_int;
pub type XRootWindow = unsafe extern "C" fn(*mut Display, c_int) -> Window;
pub type XCreateWindow = unsafe extern "C" fn(
    *mut Display,
    Window,
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
) -> Window;
pub type XCreateSimpleWindow = unsafe extern "C" fn(
    *mut Display,
    Window,
    c_int,
    c_int,
    c_uint,
    c_uint,
    c_uint,
    c_ulong,
    c_ulong,
) -> Window;
pub type XMapWindow = unsafe extern "C" fn(*mut Display, Window) -> c_int;
pub type XUnmapWindow = unsafe extern "C" fn(*mut Display, Window) -> c_int;
pub type XStoreName = unsafe extern "C" fn(*mut Display, Window, *const c_char) -> c_int;
pub type XInternAtom = unsafe extern "C" fn(*mut Display, *const c_char, c_int) -> Atom;
pub type XSetWMProtocols = unsafe extern "C" fn(*mut Display, Window, *mut Atom, c_int) -> c_int;
pub type XSelectInput = unsafe extern "C" fn(*mut Display, Window, c_long) -> c_int;
pub type XPending = unsafe extern "C" fn(*mut Display) -> c_int;
pub type XNextEvent = unsafe extern "C" fn(*mut Display, *mut XEvent) -> c_int;
pub type XFilterEvent = unsafe extern "C" fn(*mut XEvent, Window) -> c_int;
pub type XLookupString = unsafe extern "C" fn(
    *mut XKeyEvent,
    *mut c_char,
    c_int,
    *mut KeySym,
    *mut XComposeStatus,
) -> c_int;
pub type XMoveResizeWindow =
    unsafe extern "C" fn(*mut Display, Window, c_int, c_int, c_uint, c_uint) -> c_int;
pub type XDestroyWindow = unsafe extern "C" fn(*mut Display, Window) -> c_int;
pub type XSendEvent =
    unsafe extern "C" fn(*mut Display, Window, c_int, c_long, *mut XEvent) -> c_int;
pub type XCreateGC = unsafe extern "C" fn(*mut Display, Drawable, c_ulong, *mut XGCValues) -> GC;
pub type XFreeGC = unsafe extern "C" fn(*mut Display, GC) -> c_int;
pub type XSetForeground = unsafe extern "C" fn(*mut Display, GC, c_ulong) -> c_int;
pub type XFillRectangle =
    unsafe extern "C" fn(*mut Display, Drawable, GC, c_int, c_int, c_uint, c_uint) -> c_int;
pub type XFlush = unsafe extern "C" fn(*mut Display) -> c_int;
pub type XSync = unsafe extern "C" fn(*mut Display, c_int) -> c_int;
pub type XConnectionNumber = unsafe extern "C" fn(*mut Display) -> c_int;
pub type XSetLocaleModifiers = unsafe extern "C" fn(*const c_char) -> *mut c_char;
pub type XOpenIM = unsafe extern "C" fn(*mut Display, *mut c_void, *mut c_char, *mut c_char) -> XIM;
pub type XCloseIM = unsafe extern "C" fn(XIM) -> c_int;
pub type XCreateIC = unsafe extern "C" fn(XIM, ...) -> XIC;
pub type XDestroyIC = unsafe extern "C" fn(XIC);
pub type XmbLookupString =
    unsafe extern "C" fn(XIC, *mut XKeyEvent, *mut c_char, c_int, *mut KeySym, *mut c_int) -> c_int;
pub type XSetICFocus = unsafe extern "C" fn(XIC);
pub type XUnsetICFocus = unsafe extern "C" fn(XIC);
pub type XGetInputFocus = unsafe extern "C" fn(*mut Display, *mut Window, *mut c_int) -> c_int;
pub type XGetErrorText = unsafe extern "C" fn(*mut Display, c_int, *mut c_char, c_int) -> c_int;
pub type XSetErrorHandler = unsafe extern "C" fn(
    Option<unsafe extern "C" fn(*mut Display, *mut XErrorEvent) -> c_int>,
) -> Option<
    unsafe extern "C" fn(*mut Display, *mut XErrorEvent) -> c_int,
>;
pub type XChangeProperty = unsafe extern "C" fn(
    *mut Display,
    Window,
    Atom,
    Atom,
    c_int,
    c_int,
    *const c_uchar,
    c_int,
) -> c_int;
pub type XChangeWindowAttributes =
    unsafe extern "C" fn(*mut Display, Window, c_ulong, *mut XSetWindowAttributes) -> c_int;
pub type XMoveWindow = unsafe extern "C" fn(*mut Display, Window, c_int, c_int) -> c_int;
pub type XResizeWindow = unsafe extern "C" fn(*mut Display, Window, c_uint, c_uint) -> c_int;
pub type XGetWindowProperty = unsafe extern "C" fn(
    *mut Display,
    Window,
    Atom,
    c_long,
    c_long,
    c_int,
    Atom,
    *mut Atom,
    *mut c_int,
    *mut c_ulong,
    *mut c_ulong,
    *mut *mut c_uchar,
) -> c_int;
pub type XFree = unsafe extern "C" fn(*mut c_void) -> c_int;
pub type XDefineCursor = unsafe extern "C" fn(*mut Display, Window, Cursor) -> c_int;
pub type XCreateFontCursor = unsafe extern "C" fn(*mut Display, c_uint) -> Cursor;
pub type XFreeCursor = unsafe extern "C" fn(*mut Display, Cursor) -> c_int;
pub type XUndefineCursor = unsafe extern "C" fn(*mut Display, Window) -> c_int;
pub type XkbSetDetectableAutoRepeat =
    unsafe extern "C" fn(*mut Display, c_int, *mut c_int) -> c_int;
pub type XkbGetMap = unsafe extern "C" fn(*mut Display, c_uint, c_uint) -> *mut XkbDescRec;
pub type XkbFreeKeyboard = unsafe extern "C" fn(*mut XkbDescRec, c_uint, c_int);

// X11 Standard Cursor Font Constants (from cursorfont.h)
pub const XC_left_ptr: c_uint = 68;
pub const XC_crosshair: c_uint = 34;
pub const XC_hand2: c_uint = 60;
pub const XC_fleur: c_uint = 52;
pub const XC_xterm: c_uint = 152;
pub const XC_watch: c_uint = 150;
pub const XC_X_cursor: c_uint = 0;
pub const XC_top_side: c_uint = 138;
pub const XC_bottom_side: c_uint = 16;
pub const XC_left_side: c_uint = 70;
pub const XC_right_side: c_uint = 96;
pub const XC_top_left_corner: c_uint = 134;
pub const XC_top_right_corner: c_uint = 136;
pub const XC_bottom_left_corner: c_uint = 12;
pub const XC_bottom_right_corner: c_uint = 14;
pub const XC_sb_h_double_arrow: c_uint = 108;
pub const XC_sb_v_double_arrow: c_uint = 116;
pub const XC_sizing: c_uint = 120;

// Display dimension functions
pub type XDisplayWidth = unsafe extern "C" fn(*mut Display, c_int) -> c_int;
pub type XDisplayHeight = unsafe extern "C" fn(*mut Display, c_int) -> c_int;
pub type XDisplayWidthMM = unsafe extern "C" fn(*mut Display, c_int) -> c_int;
pub type XDisplayHeightMM = unsafe extern "C" fn(*mut Display, c_int) -> c_int;
