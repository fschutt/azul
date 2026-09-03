//! C-style FFI definitions for X11, EGL, and xkbcommon.
//!
//! These types are consumed by `dlopen.rs` for dynamic symbol loading
//! and used throughout the `x11` module for the X11 windowing backend.

#![allow(non_camel_case_types, non_snake_case)]

use std::ffi::{c_char, c_int, c_long, c_uchar, c_uint, c_ulong, c_ushort, c_void};

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
    pub selection: XSelectionEvent,
    pub keymap: XKeymapEvent,
    pub mapping: XMappingEvent,
    pub xcookie: XGenericEventCookie,
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
/// `KeymapNotify`: the server reports the FULL keyboard state right after every
/// `FocusIn` (when `KeymapStateMask` is selected). `key_vector` is a bit vector
/// indexed by keycode (`key_vector[kc >> 3] & (1 << (kc & 7))`), which is the
/// X11-designed remedy for keys released while another window held focus.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XKeymapEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: c_int,
    pub display: *mut Display,
    pub window: Window,
    pub key_vector: [c_char; 32],
}
/// `MappingNotify`: the keycode → keysym table or the modifier mapping changed
/// (keyboard layout switch, `xmodmap`). Delivered to every client regardless of
/// the selected event mask; the client-side table is only refreshed by passing
/// this event to `XRefreshKeyboardMapping`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XMappingEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: c_int,
    pub display: *mut Display,
    pub window: Window,
    pub request: c_int,
    pub first_keycode: c_int,
    pub count: c_int,
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
/// `SelectionNotify` event (reply to `XConvertSelection`); used by the XDND
/// drop path to learn that the requested `text/uri-list` data has been written
/// into `property` (or `None` = the conversion failed).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XSelectionEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: c_int,
    pub display: *mut Display,
    pub requestor: Window,
    pub selection: Atom,
    pub target: Atom,
    pub property: Atom,
    pub time: Time,
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

// Event masks
pub const KeyPressMask: c_long = 1 << 0;
pub const KeyReleaseMask: c_long = 1 << 1;
pub const ButtonPressMask: c_long = 1 << 2;
pub const ButtonReleaseMask: c_long = 1 << 3;
pub const EnterWindowMask: c_long = 1 << 4;
pub const LeaveWindowMask: c_long = 1 << 5;
pub const PointerMotionMask: c_long = 1 << 6;
/// Ask for `KeymapNotify` after every `FocusIn` — without it the client has no
/// way to learn which keys were released while another window had focus.
pub const KeymapStateMask: c_long = 1 << 14;
pub const ExposureMask: c_long = 1 << 15;
pub const StructureNotifyMask: c_long = 1 << 17;
pub const FocusChangeMask: c_long = 1 << 21;

// X11 modifier masks (from X.h)
// These are used in the 'state' field of XButtonEvent, XMotionEvent, XKeyEvent, XCrossingEvent
pub const SHIFT_MASK: c_uint = 1 << 0;
pub const LOCK_MASK: c_uint = 1 << 1; // Caps Lock
pub const CONTROL_MASK: c_uint = 1 << 2;
pub const MOD1_MASK: c_uint = 1 << 3; // Usually Alt
pub const MOD2_MASK: c_uint = 1 << 4; // Usually Num Lock
pub const MOD3_MASK: c_uint = 1 << 5;
pub const MOD4_MASK: c_uint = 1 << 6; // Usually Super/Windows key
pub const MOD5_MASK: c_uint = 1 << 7;

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
pub const KeymapNotify: c_int = 11;
pub const Expose: c_int = 12;
pub const UnmapNotify: c_int = 18;
pub const MapNotify: c_int = 19;
pub const ConfigureNotify: c_int = 22;
pub const SelectionNotify: c_int = 31;
pub const ClientMessage: c_int = 33;
pub const MappingNotify: c_int = 34;

// Focus/crossing `mode` values (X.h). A grab activating or releasing — including
// this app's OWN menu pointer grab — synthesizes FocusIn/FocusOut and
// EnterNotify/LeaveNotify pairs that do NOT mean the user changed windows.
pub const NotifyNormal: c_int = 0;
pub const NotifyGrab: c_int = 1;
pub const NotifyUngrab: c_int = 2;
pub const NotifyWhileGrabbed: c_int = 3;
/// Focus `detail`: the focus followed the POINTER, not the window.
pub const NotifyPointer: c_int = 5;

// `XMappingEvent.request` values (X.h).
pub const MappingModifier: c_int = 0;
pub const MappingKeyboard: c_int = 1;
pub const MappingPointer: c_int = 2;

// Window classes and attributes
pub const InputOutput: c_uint = 1;
pub const CopyFromParent: c_int = 0;
pub const CWBackPixel: c_ulong = 1 << 1;
pub const CWBorderPixel: c_ulong = 1 << 3;
pub const CWEventMask: c_ulong = 1 << 11;
pub const CWOverrideRedirect: c_ulong = 1 << 9;
pub const SubstructureRedirectMask: c_long = 1 << 20;
pub const SubstructureNotifyMask: c_long = 1 << 19;

// Property modes
pub const PropModeReplace: c_int = 0;
pub const PropModeAppend: c_int = 2;

// Predefined atoms. These are fixed by the protocol (X.h), not interned:
// PRIMARY=1, SECONDARY=2, ARC=3, ATOM=4, BITMAP=5, CARDINAL=6.
pub const XA_ATOM: Atom = 4;
/// The type `_NET_WM_ICON` and most other EWMH numeric properties carry.
pub const XA_CARDINAL: Atom = 6;
/// `AnyPropertyType` wildcard for `XGetWindowProperty` (accept any type).
pub const AnyPropertyType: Atom = 0;

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
pub const XK_Meta_L: u32 = 0xFFE7;
pub const XK_Meta_R: u32 = 0xFFE8;
pub const XK_Hyper_L: u32 = 0xFFED;
pub const XK_Hyper_R: u32 = 0xFFEE;
pub const XK_Caps_Lock: u32 = 0xFFE5;
pub const XK_Shift_Lock: u32 = 0xFFE6;
pub const XK_Num_Lock: u32 = 0xFF7F;
pub const XK_Menu: u32 = 0xFF67;
pub const XK_Print: u32 = 0xFF61;
pub const XK_Sys_Req: u32 = 0xFF15;
/// AltGr (third-level shift). Missing from the table, so every AltGr-composed
/// accelerator was dead on X11.
pub const XK_ISO_Level3_Shift: u32 = 0xFE03;
pub const XK_Mode_switch: u32 = 0xFF7E;

// Punctuation / OEM keys. `Ctrl+-` / `Ctrl+=` (zoom out/in) live here.
pub const XK_minus: u32 = 0x002D;
pub const XK_underscore: u32 = 0x005F;
pub const XK_equal: u32 = 0x003D;
pub const XK_plus: u32 = 0x002B;
pub const XK_comma: u32 = 0x002C;
pub const XK_less: u32 = 0x003C;
pub const XK_period: u32 = 0x002E;
pub const XK_greater: u32 = 0x003E;
pub const XK_semicolon: u32 = 0x003B;
pub const XK_colon: u32 = 0x003A;
pub const XK_apostrophe: u32 = 0x0027;
pub const XK_quotedbl: u32 = 0x0022;
pub const XK_grave: u32 = 0x0060;
pub const XK_asciitilde: u32 = 0x007E;
pub const XK_bracketleft: u32 = 0x005B;
pub const XK_braceleft: u32 = 0x007B;
pub const XK_bracketright: u32 = 0x005D;
pub const XK_braceright: u32 = 0x007D;
pub const XK_backslash: u32 = 0x005C;
pub const XK_bar: u32 = 0x007C;
pub const XK_slash: u32 = 0x002F;
pub const XK_question: u32 = 0x003F;

// Shifted forms of the digit row (US layout). The digit keys were the one group
// whose shifted keysym had no entry, so `1` pressed, Shift pressed, `1`
// released reported XK_exclam on the release and the key never left
// `pressed_virtual_keycodes`.
pub const XK_exclam: u32 = 0x0021;
pub const XK_at: u32 = 0x0040;
pub const XK_numbersign: u32 = 0x0023;
pub const XK_dollar: u32 = 0x0024;
pub const XK_percent: u32 = 0x0025;
pub const XK_asciicircum: u32 = 0x005E;
pub const XK_ampersand: u32 = 0x0026;
pub const XK_asterisk: u32 = 0x002A;
pub const XK_parenleft: u32 = 0x0028;
pub const XK_parenright: u32 = 0x0029;

/// "This keycode produces no symbol" — `XkbKeycodeToKeysym` returns it for an
/// unbound keycode/group/level.
pub const NoSymbol: KeySym = 0;

// Keypad.
pub const XK_KP_Space: u32 = 0xFF80;
pub const XK_KP_Tab: u32 = 0xFF89;
pub const XK_KP_Enter: u32 = 0xFF8D;
pub const XK_KP_Home: u32 = 0xFF95;
pub const XK_KP_Left: u32 = 0xFF96;
pub const XK_KP_Up: u32 = 0xFF97;
pub const XK_KP_Right: u32 = 0xFF98;
pub const XK_KP_Down: u32 = 0xFF99;
pub const XK_KP_Page_Up: u32 = 0xFF9A;
pub const XK_KP_Page_Down: u32 = 0xFF9B;
pub const XK_KP_End: u32 = 0xFF9C;
pub const XK_KP_Begin: u32 = 0xFF9D;
pub const XK_KP_Insert: u32 = 0xFF9E;
pub const XK_KP_Delete: u32 = 0xFF9F;
pub const XK_KP_Equal: u32 = 0xFFBD;
pub const XK_KP_Multiply: u32 = 0xFFAA;
pub const XK_KP_Add: u32 = 0xFFAB;
pub const XK_KP_Separator: u32 = 0xFFAC;
pub const XK_KP_Subtract: u32 = 0xFFAD;
pub const XK_KP_Decimal: u32 = 0xFFAE;
pub const XK_KP_Divide: u32 = 0xFFAF;

// Multimedia keys (XF86keysym.h). Values verified against
// /opt/homebrew/include/X11/XF86keysym.h rather than recalled: a wrong
// constant here maps a REAL key to the wrong action, silently, which is
// worse than not mapping it at all.
//
// These are ordinary keysyms - a media key is not a special event stream on
// X11 or Wayland - so they only need to appear in the translation table.
// They arrive only if the desktop environment has not grabbed them first;
// where it has, the keys drive MPRIS over D-Bus instead and never reach the
// application. That is a separate transport, logged as 9h-i-a.
pub const XF86XK_AudioLowerVolume: u32 = 0x1008FF11;
pub const XF86XK_AudioMute: u32 = 0x1008FF12;
pub const XF86XK_AudioRaiseVolume: u32 = 0x1008FF13;
pub const XF86XK_AudioPlay: u32 = 0x1008FF14;
pub const XF86XK_AudioStop: u32 = 0x1008FF15;
pub const XF86XK_AudioPrev: u32 = 0x1008FF16;
pub const XF86XK_AudioNext: u32 = 0x1008FF17;
pub const XF86XK_HomePage: u32 = 0x1008FF18;
pub const XF86XK_Mail: u32 = 0x1008FF19;
pub const XF86XK_Search: u32 = 0x1008FF1B;
pub const XF86XK_Back: u32 = 0x1008FF26;
pub const XF86XK_Forward: u32 = 0x1008FF27;
pub const XF86XK_Stop: u32 = 0x1008FF28;
pub const XF86XK_Refresh: u32 = 0x1008FF29;
pub const XF86XK_PowerOff: u32 = 0x1008FF2A;
pub const XF86XK_WakeUp: u32 = 0x1008FF2B;
pub const XF86XK_Sleep: u32 = 0x1008FF2F;
pub const XF86XK_Favorites: u32 = 0x1008FF30;
pub const XF86XK_AudioPause: u32 = 0x1008FF31;
pub const XF86XK_AudioMedia: u32 = 0x1008FF32;
pub const XF86XK_MyComputer: u32 = 0x1008FF33;
pub const XF86XK_Explorer: u32 = 0x1008FF5D;
pub const XK_KP_0: u32 = 0xFFB0;
pub const XK_KP_1: u32 = 0xFFB1;
pub const XK_KP_2: u32 = 0xFFB2;
pub const XK_KP_3: u32 = 0xFFB3;
pub const XK_KP_4: u32 = 0xFFB4;
pub const XK_KP_5: u32 = 0xFFB5;
pub const XK_KP_6: u32 = 0xFFB6;
pub const XK_KP_7: u32 = 0xFFB7;
pub const XK_KP_8: u32 = 0xFFB8;
pub const XK_KP_9: u32 = 0xFFB9;

// `Status` values returned through the last out-param of the
// Xmb/Xutf8/XwcLookupString family (Xlib.h). `XBufferOverflow` means NOTHING
// was written and the return value is the required buffer size in bytes.
pub const XBufferOverflow: c_int = -1;
pub const XLookupNone: c_int = 1;
pub const XLookupChars: c_int = 2;
pub const XLookupKeySym: c_int = 3;
pub const XLookupBoth: c_int = 4;

// EGL types
pub type EGLDisplay = *mut c_void;
pub type EGLConfig = *mut c_void;
pub type EGLContext = *mut c_void;
pub type EGLSurface = *mut c_void;
pub type EGLNativeDisplayType = *mut c_void;
pub type EGLNativeWindowType = c_ulong;

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
/// eglQueryString name: display extension list (space-separated).
pub const EGL_EXTENSIONS: u32 = 0x3055;
/// EGL_EXT_buffer_age: eglQuerySurface attribute — age of the back buffer in
/// frames (1 = same buffer as last frame, 0 = undefined/new content).
pub const EGL_BUFFER_AGE_EXT: u32 = 0x313D;

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
pub type eglQuerySurface = unsafe extern "C" fn(EGLDisplay, EGLSurface, i32, *mut i32) -> u32;
pub type eglQueryString = unsafe extern "C" fn(EGLDisplay, i32) -> *const c_char;
/// eglSwapBuffersWithDamageKHR / eglSwapBuffersWithDamageEXT (identical
/// signatures): rects are (x, y, w, h) EGLint quadruples in buffer
/// coordinates with a BOTTOM-LEFT origin, n_rects = number of quadruples.
pub type eglSwapBuffersWithDamage =
    unsafe extern "C" fn(EGLDisplay, EGLSurface, *const i32, i32) -> u32;
pub type eglSwapInterval = unsafe extern "C" fn(EGLDisplay, i32) -> u32;
pub type eglGetError = unsafe extern "C" fn() -> i32;
pub type eglGetProcAddress = unsafe extern "C" fn(*const c_char) -> *const c_void;
pub type eglDestroySurface = unsafe extern "C" fn(EGLDisplay, EGLSurface) -> u32;
pub type eglDestroyContext = unsafe extern "C" fn(EGLDisplay, EGLContext) -> u32;
pub type eglTerminate = unsafe extern "C" fn(EGLDisplay) -> u32;

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
/// `XEventsQueued(display, mode)` — how many events are in the CLIENT queue.
///
/// With [`QueuedAlready`] it never touches the socket and never round-trips,
/// which is what makes it usable in the resize hot path.
pub type XEventsQueued = unsafe extern "C" fn(*mut Display, c_int) -> c_int;
/// `XEventsQueued` mode: answer from the client queue only.
pub const QueuedAlready: c_int = 0;
pub type XNextEvent = unsafe extern "C" fn(*mut Display, *mut XEvent) -> c_int;

// ===== XInput2 (XI2) — touch + pen/tablet. ABI per scripts/ideas/platform/WACOM_TOUCH_API_RESEARCH.md =====
pub const GenericEvent: c_int = 35;
/// `XI_HierarchyChanged` — a device was added, removed, enabled, disabled,
/// attached or detached. XI2's own hotplug notification, selected on
/// `XIAllDevices` (a hierarchy change is not the property of any one device,
/// so selecting it on `XIAllMasterDevices` would miss slave hotplug entirely).
pub const XI_HierarchyChanged: c_int = 11;
/// XInput 2.4 touchpad gesture events. The in-process detector recognises
/// pinch and rotate from TOUCH POINTS, which an X11 touchpad never delivers —
/// the driver synthesizes a pointer and keeps the finger geometry — so these
/// are the only way an X11 client sees a touchpad gesture at all.
pub const XI_GesturePinchBegin: c_int = 27;
pub const XI_GesturePinchUpdate: c_int = 28;
pub const XI_GesturePinchEnd: c_int = 29;
pub const XI_GestureSwipeBegin: c_int = 30;
pub const XI_GestureSwipeUpdate: c_int = 31;
pub const XI_GestureSwipeEnd: c_int = 32;

/// `XIGesturePinchEvent.flags` — the gesture was cancelled rather than
/// completed (the compositor or driver took it over).
pub const XIGesturePinchEventCancelled: c_int = 1 << 0;
/// `XIGestureSwipeEvent.flags` — as above, for swipe.
pub const XIGestureSwipeEventCancelled: c_int = 1 << 0;

/// `XI_RawMotion` — pointer motion BEFORE the pointer-acceleration curve and
/// before clamping to the screen. Delivered against `XIAllMasterDevices` but
/// carrying the slave's `sourceid`.
pub const XI_RawMotion: c_int = 17;

pub const XI_ButtonPress: c_int = 4;
pub const XI_ButtonRelease: c_int = 5;
pub const XI_Motion: c_int = 6;
pub const XI_TouchBegin: c_int = 18;
pub const XI_TouchUpdate: c_int = 19;
pub const XI_TouchEnd: c_int = 20;
pub const XIAllDevices: c_int = 0;
pub const XIAllMasterDevices: c_int = 1;
/// The master pointer every X server has: the "Virtual core pointer". XI2
/// reserves device id 2 for it (and 3 for the virtual core keyboard); every
/// other master pointer - an MPX second cursor - gets an id past those. It is
/// the PRIMARY pointer seat; any other master is a seat of its own (9b-ii).
pub const XI_VIRTUAL_CORE_POINTER: c_int = 2;
pub const XIButtonClass: c_int = 1;
pub const XIValuatorClass: c_int = 2;
pub const XIScrollClass: c_int = 3;
pub const XITouchClass: c_int = 8;

/// `XIButtonClassInfo` — how many buttons a device has (and their labels).
/// (Its `state` reuses the `XIButtonState` defined below.)
#[repr(C)]
pub struct XIButtonClassInfo {
    pub type_: c_int,
    pub sourceid: c_int,
    pub num_buttons: c_int,
    pub labels: *mut Atom,
    pub state: XIButtonState,
}
pub const XIModeAbsolute: c_int = 1;
pub const XIScrollTypeVertical: c_int = 1;
pub const XIScrollTypeHorizontal: c_int = 2;
/// `XIScrollClassInfo.flags`: the driver does NOT emit emulated button 4-7
/// presses for this axis, so the valuator is the only scroll delivery.
pub const XIScrollFlagNoEmulation: c_int = 1;
/// `XIDeviceEvent.flags`: this button event is the legacy wheel emulation of a
/// smooth-scroll valuator. Handling both is what double-scrolls a touchpad.
pub const XIPointerEmulated: c_int = 1 << 16;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct XIEventMask {
    pub deviceid: c_int,
    pub mask_len: c_int,
    pub mask: *mut c_uchar,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XIValuatorState {
    pub mask_len: c_int,
    pub mask: *mut c_uchar,
    pub values: *mut f64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XIButtonState {
    pub mask_len: c_int,
    pub mask: *mut c_uchar,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XIModifierState {
    pub base: c_int,
    pub latched: c_int,
    pub locked: c_int,
    pub effective: c_int,
}
pub type XIGroupState = XIModifierState;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XIAnyClassInfo {
    pub type_: c_int,
    pub sourceid: c_int,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XIValuatorClassInfo {
    pub type_: c_int,
    pub sourceid: c_int,
    pub number: c_int,
    pub label: Atom,
    pub min: f64,
    pub max: f64,
    pub value: f64,
    pub resolution: c_int,
    pub mode: c_int,
}
/// Smooth-scroll axis of a device (XI2.1). `increment` is the valuator delta
/// that equals one legacy wheel detent; the valuator itself carries an
/// ACCUMULATING absolute value, so a scroll delta is
/// `(new - last) / increment` — fractional for touchpads.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XIScrollClassInfo {
    pub type_: c_int,
    pub sourceid: c_int,
    pub number: c_int,
    pub scroll_type: c_int,
    pub increment: f64,
    pub flags: c_int,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XIDeviceInfo {
    pub deviceid: c_int,
    pub name: *mut c_char,
    pub use_: c_int,
    pub attachment: c_int,
    pub enabled: c_int,
    pub num_classes: c_int,
    pub classes: *mut *mut XIAnyClassInfo,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XIDeviceEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: c_int,
    pub display: *mut Display,
    pub extension: c_int,
    pub evtype: c_int,
    pub time: c_ulong,
    pub deviceid: c_int,
    pub sourceid: c_int,
    pub detail: c_int,
    pub root: Window,
    pub event: Window,
    pub child: Window,
    pub root_x: f64,
    pub root_y: f64,
    pub event_x: f64,
    pub event_y: f64,
    pub flags: c_int,
    pub buttons: XIButtonState,
    pub valuators: XIValuatorState,
    pub mods: XIModifierState,
    pub group: XIGroupState,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XGenericEventCookie {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: c_int,
    pub display: *mut Display,
    pub extension: c_int,
    pub evtype: c_int,
    pub cookie: c_uint,
    pub data: *mut c_void,
}

pub type XIQueryVersion = unsafe extern "C" fn(*mut Display, *mut c_int, *mut c_int) -> c_int;
pub type XISelectEvents =
    unsafe extern "C" fn(*mut Display, Window, *mut XIEventMask, c_int) -> c_int;
pub type XIQueryDevice = unsafe extern "C" fn(*mut Display, c_int, *mut c_int) -> *mut XIDeviceInfo;
pub type XIFreeDeviceInfo = unsafe extern "C" fn(*mut XIDeviceInfo);
/// `XIGetProperty` — reads a device property (`Device Product ID`,
/// `Device Node`, ...). The returned `data` is freed with `XFree`.
pub type XIGetProperty = unsafe extern "C" fn(
    *mut Display,
    c_int,                      // deviceid
    Atom,                       // property
    std::os::raw::c_long,       // offset (in 32-bit longwords)
    std::os::raw::c_long,       // length (in 32-bit longwords)
    c_int,                      // delete
    Atom,                       // type filter (0 = AnyPropertyType)
    *mut Atom,                  // type_return
    *mut c_int,                 // format_return
    *mut std::os::raw::c_ulong, // num_items_return
    *mut std::os::raw::c_ulong, // bytes_after_return
    *mut *mut u8,               // data (XFree)
) -> c_int;
pub type XGetEventData = unsafe extern "C" fn(*mut Display, *mut XGenericEventCookie) -> c_int;
pub type XFreeEventData = unsafe extern "C" fn(*mut Display, *mut XGenericEventCookie);
pub type XQueryExtension =
    unsafe extern "C" fn(*mut Display, *const c_char, *mut c_int, *mut c_int, *mut c_int) -> c_int;
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
pub type XCreateGC = unsafe extern "C" fn(*mut Display, Drawable, c_ulong, *mut c_void) -> GC;
pub type XFreeGC = unsafe extern "C" fn(*mut Display, GC) -> c_int;
pub type XSetForeground = unsafe extern "C" fn(*mut Display, GC, c_ulong) -> c_int;
pub type XFillRectangle =
    unsafe extern "C" fn(*mut Display, Drawable, GC, c_int, c_int, c_uint, c_uint) -> c_int;
pub type XClearWindow = unsafe extern "C" fn(*mut Display, Window) -> c_int;
pub type XDrawString =
    unsafe extern "C" fn(*mut Display, Drawable, GC, c_int, c_int, *const c_char, c_int) -> c_int;
pub type XFlush = unsafe extern "C" fn(*mut Display) -> c_int;
pub type XSync = unsafe extern "C" fn(*mut Display, c_int) -> c_int;

// --- Pointer grab (used for menu / popup click-outside dismissal) ---
pub type Time = c_ulong;
pub const CurrentTime: Time = 0;
pub const GrabModeSync: c_int = 0;
pub const GrabModeAsync: c_int = 1;
pub const GrabSuccess: c_int = 0;
/// XGrabPointer(display, grab_window, owner_events, event_mask, pointer_mode,
///   keyboard_mode, confine_to, cursor, time) -> status
pub type XGrabPointer = unsafe extern "C" fn(
    *mut Display,
    Window,
    c_int,
    c_uint,
    c_int,
    c_int,
    Window,
    Cursor,
    Time,
) -> c_int;
pub type XUngrabPointer = unsafe extern "C" fn(*mut Display, Time) -> c_int;
pub type XConnectionNumber = unsafe extern "C" fn(*mut Display) -> c_int;
pub type XSetLocaleModifiers = unsafe extern "C" fn(*const c_char) -> *mut c_char;
pub type XOpenIM = unsafe extern "C" fn(*mut Display, *mut c_void, *mut c_char, *mut c_char) -> XIM;
pub type XCloseIM = unsafe extern "C" fn(XIM) -> c_int;
pub type XCreateIC = unsafe extern "C" fn(XIM, ...) -> XIC;
pub type XDestroyIC = unsafe extern "C" fn(XIC);
pub type XSetICValues = unsafe extern "C" fn(XIC, ...) -> *mut c_char;
pub type XmbLookupString =
    unsafe extern "C" fn(XIC, *mut XKeyEvent, *mut c_char, c_int, *mut KeySym, *mut c_int) -> c_int;
// Like XmbLookupString but the committed bytes are ALWAYS UTF-8 (XmbLookupString
// returns text in the locale codeset, which is only UTF-8 in a UTF-8 locale).
pub type Xutf8LookupString =
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
/// Returns the RESOURCE_MANAGER property of screen 0 (the `xrdb` database,
/// where `Xft.dpi` lives). The returned string is owned by Xlib — do NOT free it.
pub type XResourceManagerString = unsafe extern "C" fn(*mut Display) -> *mut c_char;
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
/// XConvertSelection(display, selection, target, property, requestor, time).
/// Asynchronously requests the selection owner to write the `target`-typed data
/// into `property` on the `requestor` window; the data arrives later as a
/// `SelectionNotify` event (used by the XDND drop path for `text/uri-list`).
pub type XConvertSelection =
    unsafe extern "C" fn(*mut Display, Atom, Atom, Atom, Window, Time) -> c_int;
pub type XFree = unsafe extern "C" fn(*mut c_void) -> c_int;
pub type XDefineCursor = unsafe extern "C" fn(*mut Display, Window, Cursor) -> c_int;
pub type XCreateFontCursor = unsafe extern "C" fn(*mut Display, c_uint) -> Cursor;
pub type XFreeCursor = unsafe extern "C" fn(*mut Display, Cursor) -> c_int;
pub type XUndefineCursor = unsafe extern "C" fn(*mut Display, Window) -> c_int;
pub type XkbSetDetectableAutoRepeat =
    unsafe extern "C" fn(*mut Display, c_int, *mut c_int) -> c_int;
/// X11 keycode. 8 bits on Linux (`NeedWidePrototypes == 0`).
pub type KeyCode = c_uchar;
/// The 8 × `max_keypermod` table returned by `XGetModifierMapping`: row `i`
/// (Shift, Lock, Control, Mod1..Mod5) lists the keycodes bound to that modifier.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XModifierKeymap {
    pub max_keypermod: c_int,
    pub modifiermap: *mut KeyCode,
}
pub type XGetModifierMapping = unsafe extern "C" fn(*mut Display) -> *mut XModifierKeymap;
pub type XFreeModifiermap = unsafe extern "C" fn(*mut XModifierKeymap) -> c_int;
/// Refresh the CLIENT-side keycode → keysym table after a `MappingNotify`.
/// Without it every translation stays on the layout that was active at connect.
pub type XRefreshKeyboardMapping = unsafe extern "C" fn(*mut XMappingEvent) -> c_int;
/// Full keyboard state as a 32-byte keycode bit vector (`keys_return`).
pub type XQueryKeymap = unsafe extern "C" fn(*mut Display, *mut c_char) -> c_int;
/// XKB keycode → keysym for a given group/level. Used to identify which
/// `ModN` bit actually carries Alt / Super / AltGr on THIS keyboard.
pub type XkbKeycodeToKeysym = unsafe extern "C" fn(*mut Display, KeyCode, c_int, c_int) -> KeySym;
/// XTranslateCoordinates(display, src_w, dest_w, src_x, src_y,
///   dest_x_return, dest_y_return, child_return) -> Bool (0 = different screens).
pub type XTranslateCoordinates = unsafe extern "C" fn(
    *mut Display,
    Window,
    Window,
    c_int,
    c_int,
    *mut c_int,
    *mut c_int,
    *mut Window,
) -> c_int;

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

// XIM (X Input Method) structures and constants
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct XPoint {
    pub x: i16,
    pub y: i16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct XRectangle {
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
}

// XIM style constants
pub const XIMPreeditArea: c_ulong = 0x0001;
pub const XIMPreeditCallbacks: c_ulong = 0x0002;
pub const XIMPreeditPosition: c_ulong = 0x0004;
pub const XIMPreeditNothing: c_ulong = 0x0008;
pub const XIMPreeditNone: c_ulong = 0x0010;
pub const XIMStatusArea: c_ulong = 0x0100;
pub const XIMStatusCallbacks: c_ulong = 0x0200;
pub const XIMStatusNothing: c_ulong = 0x0400;
pub const XIMStatusNone: c_ulong = 0x0800;

// XIM attribute name strings (passed to XCreateIC / XSetICValues / XGetIMValues).
// Defined here as NUL-terminated byte slices; callers cast `.as_ptr()` to `*const c_char`.
pub const XN_QUERY_INPUT_STYLE: &[u8] = b"queryInputStyle\0";
pub const XN_INPUT_STYLE: &[u8] = b"inputStyle\0";
pub const XN_CLIENT_WINDOW: &[u8] = b"clientWindow\0";
pub const XN_FOCUS_WINDOW: &[u8] = b"focusWindow\0";
pub const XN_PREEDIT_ATTRIBUTES: &[u8] = b"preeditAttributes\0";
pub const XN_STATUS_ATTRIBUTES: &[u8] = b"statusAttributes\0";
pub const XN_SPOT_LOCATION: &[u8] = b"spotLocation\0";
pub const XN_PREEDIT_START_CALLBACK: &[u8] = b"preeditStartCallback\0";
pub const XN_PREEDIT_DONE_CALLBACK: &[u8] = b"preeditDoneCallback\0";
pub const XN_PREEDIT_DRAW_CALLBACK: &[u8] = b"preeditDrawCallback\0";
pub const XN_PREEDIT_CARET_CALLBACK: &[u8] = b"preeditCaretCallback\0";
pub const XN_STATUS_START_CALLBACK: &[u8] = b"statusStartCallback\0";
pub const XN_STATUS_DONE_CALLBACK: &[u8] = b"statusDoneCallback\0";
pub const XN_STATUS_DRAW_CALLBACK: &[u8] = b"statusDrawCallback\0";

/// List of input styles supported by the XIM. Returned by XGetIMValues(XNQueryInputStyle).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XIMStyles {
    pub count_styles: c_ushort,
    pub supported_styles: *mut c_ulong,
}

/// Per-character feedback flags (highlight, underline, etc.).
pub type XIMFeedback = c_ulong;

/// Pre-edit string passed to the draw callback. Modern IMs fill the
/// `multi_byte` side of the original union; we model the field as a single
/// `*mut c_char` and ignore the `wide_char` branch (we never see it because
/// our locale is UTF-8 and `encoding_is_wchar == False`).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XIMText {
    pub length: c_ushort,
    pub feedback: *mut XIMFeedback,
    pub encoding_is_wchar: c_int, // Xlib Bool
    pub string: *mut c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct XIMPreeditDrawCallbackStruct {
    pub caret: c_int,
    pub chg_first: c_int,
    pub chg_length: c_int,
    pub text: *mut XIMText,
}

pub type XIMCaretDirection = c_int;
pub type XIMCaretStyle = c_int;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct XIMPreeditCaretCallbackStruct {
    pub position: c_int,
    pub direction: XIMCaretDirection,
    pub style: XIMCaretStyle,
}

/// XIM callback function pointer. `client_data` is the pointer we stash in
/// the `XIMCallback` struct; `call_data` is the per-callback payload (e.g.
/// `XIMPreeditDrawCallbackStruct*` for the draw callback).
pub type XIMProc = unsafe extern "C" fn(XIC, *mut c_void, *mut c_void);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct XIMCallback {
    pub client_data: *mut c_void,
    pub callback: Option<XIMProc>,
}

/// Opaque handle returned by XVaCreateNestedList for use as the value of
/// XNPreeditAttributes / XNStatusAttributes.
pub type XVaNestedList = *mut c_void;

pub type XGetIMValues = unsafe extern "C" fn(XIM, ...) -> *mut c_char;
pub type XVaCreateNestedList = unsafe extern "C" fn(c_int, ...) -> XVaNestedList;

// Display dimension functions
pub type XDisplayWidth = unsafe extern "C" fn(*mut Display, c_int) -> c_int;
/// (display, drawable, x, y, w, h, plane_mask, format) - the eyedropper's
/// one-shot read of the root window (`ZPixmap`).
pub type XGetImage = unsafe extern "C" fn(
    *mut Display,
    Drawable,
    c_int,
    c_int,
    c_uint,
    c_uint,
    c_ulong,
    c_int,
) -> *mut XImage;
/// `XGetImage` format: pixels packed as in the framebuffer.
pub const ZPixmap: c_int = 2;
/// `XGetImage` plane mask: every plane.
pub const AllPlanes: c_ulong = !0;
pub type XDisplayHeight = unsafe extern "C" fn(*mut Display, c_int) -> c_int;
pub type XDisplayWidthMM = unsafe extern "C" fn(*mut Display, c_int) -> c_int;
pub type XDisplayHeightMM = unsafe extern "C" fn(*mut Display, c_int) -> c_int;

// XVisualInfo structure for ARGB visual selection
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct XVisualInfo {
    pub visual: *mut Visual,
    pub visualid: c_ulong,
    pub screen: c_int,
    pub depth: c_int,
    pub class: c_int,
    pub red_mask: c_ulong,
    pub green_mask: c_ulong,
    pub blue_mask: c_ulong,
    pub colormap_size: c_int,
    pub bits_per_rgb: c_int,
}

// XRender types for ARGB visual detection
// See: https://stackoverflow.com/a/9215724 (inspired by datenwolf/FTB)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct XRenderDirectFormat {
    pub red: i16,
    pub red_mask: i16,
    pub green: i16,
    pub green_mask: i16,
    pub blue: i16,
    pub blue_mask: i16,
    pub alpha: i16,
    pub alpha_mask: i16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct XRenderPictFormat {
    pub id: c_ulong,
    pub type_: c_int,
    pub depth: c_int,
    pub direct: XRenderDirectFormat,
    pub colormap: Colormap,
}

// Xlib function types for ARGB visual / colormap
pub type XCreateColormap =
    unsafe extern "C" fn(*mut Display, Window, *mut Visual, c_int) -> Colormap;
pub type XDefaultVisual = unsafe extern "C" fn(*mut Display, c_int) -> *mut Visual;
pub type XDefaultColormap = unsafe extern "C" fn(*mut Display, c_int) -> Colormap;
pub type XDefaultDepth = unsafe extern "C" fn(*mut Display, c_int) -> c_int;
pub type XMatchVisualInfo =
    unsafe extern "C" fn(*mut Display, c_int, c_int, c_int, *mut XVisualInfo) -> c_int;
pub type XFreeColormap = unsafe extern "C" fn(*mut Display, Colormap) -> c_int;

// XShape (libXext) function types - the window's bounding + input shape
// from the rendered alpha (`WindowFlags::shape_from_alpha`).
pub type XShapeQueryExtension = unsafe extern "C" fn(*mut Display, *mut c_int, *mut c_int) -> c_int;
/// (display, window, dest_kind, x_off, y_off, rects, n_rects, op, ordering)
pub type XShapeCombineRectangles = unsafe extern "C" fn(
    *mut Display,
    Window,
    c_int,
    c_int,
    c_int,
    *mut XRectangle,
    c_int,
    c_int,
    c_int,
) -> c_int;
/// `dest_kind`: what is drawn.
pub const ShapeBounding: c_int = 0;
/// `dest_kind`: what receives input.
pub const ShapeInput: c_int = 2;
/// `op`: replace the shape.
pub const ShapeSet: c_int = 0;
/// `ordering`: rects sorted by y, then x, non-overlapping bands - what
/// the alpha scan produces.
pub const YXBanded: c_int = 3;

// XRender function types
pub type XRenderFindVisualFormat =
    unsafe extern "C" fn(*mut Display, *const Visual) -> *mut XRenderPictFormat;

// XImage function types for CPU rendering
pub type XCreateImage = unsafe extern "C" fn(
    *mut Display,
    *mut c_void,
    c_uint,
    c_int,
    c_int,
    *mut c_char,
    c_uint,
    c_uint,
    c_int,
    c_int,
) -> *mut XImage;
pub type XPutImage = unsafe extern "C" fn(
    *mut Display,
    Drawable,
    GC,
    *mut XImage,
    c_int,
    c_int,
    c_int,
    c_int,
    c_uint,
    c_uint,
) -> c_int;
pub type XDestroyImage = unsafe extern "C" fn(*mut XImage) -> c_int;

// Additional CW (change window) attribute flags for XCreateWindow
pub const CWBackPixmap: c_ulong = 1 << 0;
pub const CWColormap: c_ulong = 1 << 13;

// XImage structure for XCreateImage/XPutImage/XDestroyImage
#[repr(C)]
pub struct XImage {
    pub width: c_int,
    pub height: c_int,
    pub xoffset: c_int,
    pub format: c_int,
    pub data: *mut c_char,
    pub byte_order: c_int,
    pub bitmap_unit: c_int,
    pub bitmap_bit_order: c_int,
    pub bitmap_pad: c_int,
    pub depth: c_int,
    pub bytes_per_line: c_int,
    pub bits_per_pixel: c_int,
    pub red_mask: c_ulong,
    pub green_mask: c_ulong,
    pub blue_mask: c_ulong,
    pub obdata: *mut c_char,
    // Private fields (function pointers used by Xlib internally)
    _create_image: *mut c_void,
    _destroy_image: *mut c_void,
    _get_pixel: *mut c_void,
    _put_pixel: *mut c_void,
    _sub_image: *mut c_void,
    _add_pixel: *mut c_void,
}

// Colormap allocation modes
pub const AllocNone: c_int = 0;

// Visual class for XMatchVisualInfo
pub const TrueColor: c_int = 4;

/// `XIHierarchyEvent.info[].flags` — a slave or master device was added.
pub const XISlaveAdded: c_int = 1 << 2;
/// `XIHierarchyEvent.info[].flags` — a slave or master device was removed.
pub const XISlaveRemoved: c_int = 1 << 3;
/// `XIHierarchyEvent.info[].flags` — a device was enabled (plugged in and
/// usable). Paired with `XIDeviceDisabled`; these are the transitions a user
/// actually causes, whereas Added/Removed also fire for X server bookkeeping.
pub const XIDeviceEnabled: c_int = 1 << 6;
/// `XIHierarchyEvent.info[].flags` — a device was disabled (unplugged).
pub const XIDeviceDisabled: c_int = 1 << 7;

/// One device's entry in an `XIHierarchyEvent`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct XIHierarchyInfo {
    pub deviceid: c_int,
    pub attachment: c_int,
    pub use_: c_int,
    pub enabled: c_int,
    pub flags: c_int,
}

/// XI2 hotplug event. Read out of the `XGenericEventCookie` data pointer when
/// `evtype == XI_HierarchyChanged`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct XIHierarchyEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: c_int,
    pub display: *mut Display,
    pub extension: c_int,
    pub evtype: c_int,
    pub time: c_ulong,
    pub flags: c_int,
    pub num_info: c_int,
    pub info: *mut XIHierarchyInfo,
}

/// XI 2.4 pinch gesture event. `scale` is absolute (1.0 at begin), `delta_angle`
/// is a per-update delta in DEGREES — the same shape Wayland's pinch uses.
// No Debug: it embeds XIModifierState, which is a raw C struct without one.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XIGesturePinchEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: c_int,
    pub display: *mut Display,
    pub extension: c_int,
    pub evtype: c_int,
    pub time: c_ulong,
    pub deviceid: c_int,
    pub sourceid: c_int,
    pub detail: c_int,
    pub root: c_ulong,
    pub event: c_ulong,
    pub child: c_ulong,
    pub root_x: f64,
    pub root_y: f64,
    pub event_x: f64,
    pub event_y: f64,
    pub delta_x: f64,
    pub delta_y: f64,
    pub delta_unaccel_x: f64,
    pub delta_unaccel_y: f64,
    pub scale: f64,
    pub delta_angle: f64,
    pub flags: c_int,
    pub mods: XIModifierState,
    pub group: XIGroupState,
}

/// XI 2.4 swipe gesture event. Same layout as the pinch event minus `scale`
/// and `delta_angle`.
// No Debug — see XIGesturePinchEvent.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XIGestureSwipeEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: c_int,
    pub display: *mut Display,
    pub extension: c_int,
    pub evtype: c_int,
    pub time: c_ulong,
    pub deviceid: c_int,
    pub sourceid: c_int,
    pub detail: c_int,
    pub root: c_ulong,
    pub event: c_ulong,
    pub child: c_ulong,
    pub root_x: f64,
    pub root_y: f64,
    pub event_x: f64,
    pub event_y: f64,
    pub delta_x: f64,
    pub delta_y: f64,
    pub delta_unaccel_x: f64,
    pub delta_unaccel_y: f64,
    pub flags: c_int,
    pub mods: XIModifierState,
    pub group: XIGroupState,
}

/// XI2 raw event. `valuators` is a sparse axis set exactly like a normal
/// device event's, and `raw_values` carries the UNACCELERATED figures — which
/// is the entire reason to use this event rather than differencing positions.
// No Debug: it embeds XIValuatorState, a raw C struct without one.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XIRawEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: c_int,
    pub display: *mut Display,
    pub extension: c_int,
    pub evtype: c_int,
    pub time: c_ulong,
    pub deviceid: c_int,
    pub sourceid: c_int,
    pub detail: c_int,
    pub flags: c_int,
    pub valuators: XIValuatorState,
    pub raw_values: *mut f64,
}
