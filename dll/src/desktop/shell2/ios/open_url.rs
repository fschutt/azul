//! `Url::open` on iOS: ask the system to open a URL (`UIApplication openURL:options:completionHandler:`).
//!
//! iOS starts no process, so the core crate's opener has no program to run there and `Url::open` answered `false`: an
//! app could not open a page, and the "permission refused: open the settings" path of every app had nowhere to go (the
//! settings page of the app is the URL `app-settings:`, `UIApplicationOpenSettingsURLString`). The core crate declares
//! this symbol (`extern "Rust"` in `core/src/url.rs`) and calls it for a URL; it is defined here, in the shell that
//! has UIKit, and is the only place the call is made.
//!
//! UIKit is main-thread only: a call from another thread (an app may open a URL from a worker) is handed to the main
//! queue and answers `true` ("the system was asked") at once; a call on the main thread asks directly. The system does
//! not say whether the page opened (the completion handler is not used), so `true` means asked, not shown.

use std::ffi::{c_void, CString};

use objc::{class, msg_send, runtime::Object, sel, sel_impl};

use crate::{desktop::shell2::common::debug_server::LogCategory, log_warn};

extern "C" {
    /// libdispatch's main queue (`dispatch_get_main_queue()` is a macro for its address).
    static _dispatch_main_q: c_void;
    fn dispatch_async_f(queue: *const c_void, context: *mut c_void, work: extern "C" fn(*mut c_void));
}

/// Whether the calling thread is the main thread.
fn on_main_thread() -> bool {
    // SAFETY: a class method of Foundation, valid on any thread.
    unsafe {
        let is_main: bool = msg_send![class!(NSThread), isMainThread];
        is_main
    }
}

/// `[[UIApplication sharedApplication] openURL:options:completionHandler:]` for `url`, on the main thread. `false` for
/// a string that is no URL.
fn open_on_main(url: &str) -> bool {
    let Ok(text) = CString::new(url) else {
        return false;
    };
    // SAFETY: UIKit / Foundation calls on the main thread; every object is checked for nil before it is used. The
    // NSString and the NSURL are autoreleased by their factories; the options dictionary is empty and the completion
    // handler is nil (both allowed by the API).
    unsafe {
        let app: *mut Object = msg_send![class!(UIApplication), sharedApplication];
        if app.is_null() {
            return false;
        }
        let string: *mut Object = msg_send![class!(NSString), stringWithUTF8String: text.as_ptr()];
        if string.is_null() {
            return false;
        }
        let nsurl: *mut Object = msg_send![class!(NSURL), URLWithString: string];
        if nsurl.is_null() {
            log_warn!(LogCategory::Platform, "[open] {:?} is not a URL the system can open", url);
            return false;
        }
        let options: *mut Object = msg_send![class!(NSDictionary), dictionary];
        let no_handler: *mut Object = core::ptr::null_mut();
        let _: () = msg_send![app, openURL: nsurl options: options completionHandler: no_handler];
    }
    true
}

/// The URL handed to the main queue by a call from another thread.
struct Pending {
    url: String,
}

extern "C" fn open_from_queue(context: *mut c_void) {
    // SAFETY: `context` is the `Box<Pending>` `azul_ios_open_url` leaked for exactly this call.
    let pending = unsafe { Box::from_raw(context.cast::<Pending>()) };
    let _ = open_on_main(&pending.url);
}

/// The hook of `azul_core::url::Url::open` for iOS: `true` once the system was asked to open `url`.
#[no_mangle]
pub fn azul_ios_open_url(url: &str) -> bool {
    if on_main_thread() {
        return open_on_main(url);
    }
    if CString::new(url).is_err() {
        return false;
    }
    let context = Box::into_raw(Box::new(Pending { url: url.to_string() }));
    // SAFETY: the main queue lives for the process's life; the context is freed by `open_from_queue`.
    unsafe {
        dispatch_async_f(core::ptr::addr_of!(_dispatch_main_q), context.cast(), open_from_queue);
    }
    true
}
