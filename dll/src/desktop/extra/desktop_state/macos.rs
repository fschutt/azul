//! macOS: what [`DesktopState`] reads, with the system's own calls and no permission.
//!
//! * The app in front: `-[NSWorkspace frontmostApplication]` (its `bundleIdentifier` and
//!   `localizedName`). Plain Objective-C messages through `objc_msgSend` (the dll links no ObjC
//!   binding crate for this), inside an autorelease pool of their own.
//! * The pointer: `CGEventCreate(NULL)` and `CGEventGetLocation`, the current mouse location in
//!   the global display coordinates (points, origin the TOP left of the primary display, y down).
//!   The location is read; no event is posted or tapped, and no Accessibility or Input Monitoring
//!   permission is asked for (those are for reading keys and clicks, not the position).
//! * A locked screen: the session's dictionary (`CGSessionCopyCurrentDictionary`) says
//!   `CGSSessionScreenIsLocked` while the lock screen shows (the key is absent otherwise), and the
//!   screen saver counts as locked while it is the app in front (`com.apple.ScreenSaver.Engine`).
//! * A sleeping display: `CGDisplayIsAsleep` of the main display.

use std::{
    ffi::{c_char, c_void, CStr},
    mem::transmute,
};

use azul_core::geom::{OptionScreenPosition, ScreenPosition};
use azul_css::{AzString, OptionString};

use super::DesktopState;

type CFTypeRef = *const c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[link(name = "objc")]
extern "C" {
    fn objc_getClass(name: *const c_char) -> *mut c_void;
    fn sel_registerName(name: *const c_char) -> *mut c_void;
    fn objc_msgSend();
    fn objc_autoreleasePoolPush() -> *mut c_void;
    fn objc_autoreleasePoolPop(pool: *mut c_void);
}

// The frameworks are loaded for the app anyway (AppKit by its windows); linking them makes sure the
// classes exist when this runs.
#[link(name = "AppKit", kind = "framework")]
extern "C" {}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventCreate(source: CFTypeRef) -> CFTypeRef;
    fn CGEventGetLocation(event: CFTypeRef) -> CGPoint;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayIsAsleep(display: u32) -> u32;
    fn CGSessionCopyCurrentDictionary() -> CFTypeRef;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: CFTypeRef);
    fn CFStringCreateWithCString(allocator: CFTypeRef, c_str: *const c_char, encoding: u32) -> CFTypeRef;
    fn CFDictionaryGetValue(dict: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
    fn CFBooleanGetValue(boolean: CFTypeRef) -> u8;
}

/// `kCFStringEncodingUTF8`.
const UTF8: u32 = 0x0800_0100;

/// The screen saver's own process, which is the app in front while it shows.
const SCREEN_SAVER_BUNDLE: &str = "com.apple.ScreenSaver.Engine";

/// `[object selector]` for a selector that takes nothing and returns an object (or a C string).
///
/// # Safety
/// `object` is an Objective-C object (or nil) that answers `selector` with a pointer.
unsafe fn send(object: *mut c_void, selector: &CStr) -> *mut c_void {
    if object.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: `objc_msgSend` is called through its real signature for a message with no arguments.
    let f: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void = unsafe { transmute(objc_msgSend as *const c_void) };
    unsafe { f(object, sel_registerName(selector.as_ptr())) }
}

/// An `NSString` (or nil) as text.
///
/// # Safety
/// `string` is an `NSString` or nil.
unsafe fn text_of(string: *mut c_void) -> Option<String> {
    let utf8 = unsafe { send(string, c"UTF8String") } as *const c_char;
    if utf8.is_null() {
        return None;
    }
    // SAFETY: `UTF8String` is a NUL-terminated string that lives as long as the NSString (the pool).
    Some(unsafe { CStr::from_ptr(utf8) }.to_string_lossy().into_owned())
}

/// The bundle id and the name of the app in front, when there is one.
fn front_app() -> Option<(String, String)> {
    // SAFETY: plain messages to NSWorkspace and the application it hands out, inside a pool.
    unsafe {
        let pool = objc_autoreleasePoolPush();
        let class = objc_getClass(c"NSWorkspace".as_ptr());
        let workspace = send(class, c"sharedWorkspace");
        let app = send(workspace, c"frontmostApplication");
        let bundle = text_of(send(app, c"bundleIdentifier"));
        let name = text_of(send(app, c"localizedName"));
        objc_autoreleasePoolPop(pool);
        match (bundle, name) {
            (Some(bundle), name) => {
                let name = name.unwrap_or_else(|| bundle.clone());
                Some((bundle, name))
            }
            _ => None,
        }
    }
}

/// Where the pointer is: points from the top left of the primary display.
fn pointer() -> Option<(f32, f32)> {
    // SAFETY: an event is made from the current state of the mouse (no source), read and released.
    unsafe {
        let event = CGEventCreate(std::ptr::null());
        if event.is_null() {
            return None;
        }
        let at = CGEventGetLocation(event);
        CFRelease(event);
        (at.x.is_finite() && at.y.is_finite()).then_some((at.x as f32, at.y as f32))
    }
}

/// Whether the lock screen shows.
fn screen_locked() -> bool {
    // SAFETY: the session dictionary is owned (copy rule) and released; the key string likewise.
    unsafe {
        let session = CGSessionCopyCurrentDictionary();
        if session.is_null() {
            return false;
        }
        let key = CFStringCreateWithCString(std::ptr::null(), c"CGSSessionScreenIsLocked".as_ptr(), UTF8);
        let locked = !key.is_null() && {
            let value = CFDictionaryGetValue(session, key);
            !value.is_null() && CFBooleanGetValue(value) != 0
        };
        if !key.is_null() {
            CFRelease(key);
        }
        CFRelease(session);
        locked
    }
}

/// The desktop as it is now.
pub(super) fn read() -> DesktopState {
    let front = front_app();
    let screen_saver = front.as_ref().is_some_and(|(bundle, _)| bundle == SCREEN_SAVER_BUNDLE);
    DesktopState {
        front_bundle: front.as_ref().map_or(OptionString::None, |(b, _)| OptionString::Some(AzString::from(b.clone()))),
        front_name: front.as_ref().map_or(OptionString::None, |(_, n)| OptionString::Some(AzString::from(n.clone()))),
        pointer: pointer().map_or(OptionScreenPosition::None, |(x, y)| OptionScreenPosition::Some(ScreenPosition::new(x, y))),
        locked: screen_locked() || screen_saver,
        // SAFETY: a plain query of the main display.
        display_asleep: unsafe { CGDisplayIsAsleep(CGMainDisplayID()) != 0 },
    }
}
